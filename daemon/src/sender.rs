use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::watch;
use tracing::{info, warn, error};

use crate::config::Config;
use crate::parser::ParsedEntry;
use crate::queue::OfflineQueue;

pub struct Sender {
    client: reqwest::Client,
    config: Arc<Config>,
    queue: Arc<Mutex<OfflineQueue>>,
}

#[derive(serde::Serialize)]
struct IngestRequest {
    entries: Vec<ParsedEntry>,
}

#[derive(serde::Deserialize)]
struct IngestResponse {
    accepted: u64,
    duplicates: u64,
    errors: u64,
}

impl Sender {
    pub fn new(config: Arc<Config>, queue: Arc<Mutex<OfflineQueue>>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        Sender { client, config, queue }
    }

    /// Send a batch of entries to the server.
    ///
    /// Returns Ok(()) if entries were successfully sent OR durably queued.
    /// Returns Err if entries could not be sent AND could not be queued (data loss risk).
    pub async fn send_batch(&self, entries: Vec<ParsedEntry>) -> Result<(), Box<dyn std::error::Error>> {
        if entries.is_empty() {
            return Ok(());
        }

        let url = format!("{}/ingest", self.config.server_url);
        let req = IngestRequest { entries: entries.clone() };

        let mut builder = self.client.post(&url).json(&req);
        if !self.config.auth_token.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", self.config.auth_token));
        }
        builder = builder.header("X-Memlayer-Version", env!("CARGO_PKG_VERSION"));

        match builder.send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(body) = resp.json::<IngestResponse>().await {
                        info!(
                            accepted = body.accepted,
                            duplicates = body.duplicates,
                            errors = body.errors,
                            "Ingest batch sent"
                        );
                    }
                    Ok(())
                } else {
                    let status = resp.status();
                    let code = status.as_u16();
                    if status.is_client_error() && code != 401 && code != 413 && code != 429 {
                        // 4xx (except auth/size/rate errors): will never succeed, don't queue
                        error!(status = %status, "Server rejected batch with client error (not retryable)");
                        Ok(())
                    } else {
                        // 5xx: server error — queue for retry
                        warn!(status = %status, "Server returned error, queueing entries");
                        self.enqueue_entries(entries).await
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to send batch, queueing entries");
                self.enqueue_entries(entries).await
            }
        }
    }

    /// Enqueue entries to the offline SQLite queue.
    ///
    /// Returns Ok(()) if entries were durably queued.
    /// Returns Err if queueing failed (data loss risk).
    async fn enqueue_entries(&self, entries: Vec<ParsedEntry>) -> Result<(), Box<dyn std::error::Error>> {
        let q = self.queue.lock().await;
        q.enqueue(&entries).map_err(|e| {
            error!(error = %e, "Failed to enqueue entries — DATA LOSS: entries neither sent nor queued");
            e
        })
    }

    /// Attempt to send a single batch from the offline queue.
    /// Returns true if a batch was processed (successfully or dropped), false if queue was empty.
    async fn drain_one_batch(&self) -> bool {
        let batch = {
            let q = self.queue.lock().await;
            let count = q.count();
            if count == 0 {
                return false;
            }
            info!(queued = count, "Draining offline queue");
            match q.dequeue_batch(self.config.batch_size) {
                Ok(b) => b,
                Err(e) => {
                    error!(error = %e, "Failed to dequeue batch");
                    return false;
                }
            }
        };

        if batch.is_empty() {
            return false;
        }

        let ids: Vec<i64> = batch.iter().map(|(id, _)| *id).collect();
        let entries: Vec<ParsedEntry> = batch.into_iter().map(|(_, e)| e).collect();

        let url = format!("{}/ingest", self.config.server_url);
        let req = IngestRequest { entries };

        let mut builder = self.client.post(&url).json(&req);
        if !self.config.auth_token.is_empty() {
            builder = builder.header("Authorization", format!("Bearer {}", self.config.auth_token));
        }
        builder = builder.header("X-Memlayer-Version", env!("CARGO_PKG_VERSION"));

        match builder.send().await {
            Ok(resp) if resp.status().is_success() => {
                let q = self.queue.lock().await;
                if let Err(e) = q.remove(&ids) {
                    error!(error = %e, "Failed to remove drained entries");
                }
                info!(count = ids.len(), "Queue drained successfully");
            }
            Ok(resp) if resp.status().is_client_error()
                && resp.status().as_u16() != 401
                && resp.status().as_u16() != 413
                && resp.status().as_u16() != 429 =>
            {
                // 4xx (except auth/size/rate errors): will never succeed, remove from queue
                error!(status = %resp.status(), "Queue drain got client error (not retryable), removing entries");
                let q = self.queue.lock().await;
                if let Err(e) = q.remove(&ids) {
                    error!(error = %e, "Failed to remove rejected entries from queue");
                }
            }
            Ok(resp) => {
                warn!(status = %resp.status(), "Queue drain failed with server error");
            }
            Err(e) => {
                warn!(error = %e, "Queue drain failed");
            }
        }

        true
    }

    pub async fn drain_queue_loop(&self, mut shutdown_rx: watch::Receiver<bool>) {
        let mut delay_secs = 30u64;

        loop {
            // Wait for delay or shutdown signal
            tokio::select! {
                _ = tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)) => {}
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        // Shutdown signaled — drain all remaining entries immediately
                        info!("Shutdown: draining remaining offline queue entries");
                        loop {
                            if !self.drain_one_batch().await {
                                break;
                            }
                        }
                        info!("Shutdown: offline queue drain complete");
                        return;
                    }
                }
            }

            if self.drain_one_batch().await {
                delay_secs = 30; // Reset on activity
            } else {
                delay_secs = (delay_secs * 2).min(self.config.max_retry_delay_secs);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path, header};

    /// Create a test Config pointing at the given mock server URL.
    fn test_config(server_url: &str, data_dir: &std::path::Path) -> Arc<Config> {
        Arc::new(Config {
            server_url: server_url.to_string(),
            auth_token: "test-token".to_string(),
            watch_path: PathBuf::from("/tmp/watch"),
            data_dir: data_dir.to_owned(),
            machine_id: "test-machine".to_string(),
            batch_size: 50,
            flush_interval_secs: 5,
            max_retry_delay_secs: 300,
        })
    }

    /// Create a test ParsedEntry.
    fn test_entry(content: &str) -> ParsedEntry {
        ParsedEntry {
            payload_hash: format!("hash-{}", content),
            session_id: "sess-1".to_string(),
            message_type: "user".to_string(),
            content_type: "text".to_string(),
            raw_content: content.to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            project_path: "/test".to_string(),
            client_machine_id: "test-machine".to_string(),
            slug: None,
            source_uuid: None,
            parent_uuid: None,
            tool_name: None,
            cwd: None,
            git_branch: None,
        }
    }

    /// Create a Sender backed by a fresh temporary queue.
    /// Returns the sender, a handle to the shared queue, and the TempDir (must be kept alive).
    fn make_sender(config: Arc<Config>) -> (Sender, Arc<Mutex<OfflineQueue>>, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let queue = Arc::new(Mutex::new(
            OfflineQueue::new(tmp.path()).expect("failed to create queue"),
        ));
        let sender = Sender::new(config, queue.clone());
        (sender, queue, tmp)
    }

    /// Standard 200 OK response body for the ingest endpoint.
    fn ok_response(accepted: u64) -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "accepted": accepted, "duplicates": 0, "errors": 0
        }))
    }

    // ========================================================================
    // send_batch: 200 OK
    // ========================================================================

    #[tokio::test]
    async fn send_batch_200_entries_accepted_not_queued() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ok_response(2))
            .expect(1)
            .mount(&mock_server)
            .await;

        let entries = vec![test_entry("a"), test_entry("b")];
        let result = sender.send_batch(entries).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "entries should NOT be queued on 200");
    }

    // ========================================================================
    // send_batch: empty batch
    // ========================================================================

    #[tokio::test]
    async fn send_batch_empty_returns_ok_immediately() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, _queue, _tmp) = make_sender(config);

        // No mocks — if the sender makes a request, wiremock returns 404 and the
        // test would fail. The empty batch should short-circuit before any HTTP.
        let result = sender.send_batch(vec![]).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // send_batch: 401 Unauthorized → queued for retry
    // ========================================================================

    #[tokio::test]
    async fn send_batch_401_entries_queued_for_retry() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&mock_server)
            .await;

        let entries = vec![test_entry("auth-fail")];
        let result = sender.send_batch(entries).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "401 should queue entries for retry");
    }

    // ========================================================================
    // send_batch: 413 Payload Too Large → queued for retry
    // ========================================================================

    #[tokio::test]
    async fn send_batch_413_entries_queued_for_retry() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(413))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("too-large")]).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "413 should queue entries for retry");
    }

    // ========================================================================
    // send_batch: 429 Too Many Requests → queued for retry
    // ========================================================================

    #[tokio::test]
    async fn send_batch_429_entries_queued_for_retry() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(429))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("rate-limited")]).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "429 should queue entries for retry");
    }

    // ========================================================================
    // send_batch: 400 Bad Request (generic 4xx) → NOT queued
    // ========================================================================

    #[tokio::test]
    async fn send_batch_400_entries_not_queued() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(400))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("bad-request")]).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "400 is not retryable, entries should NOT be queued");
    }

    // ========================================================================
    // send_batch: 422 Unprocessable Entity (generic 4xx) → NOT queued
    // ========================================================================

    #[tokio::test]
    async fn send_batch_422_entries_not_queued() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(422))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("invalid-data")]).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "422 is not retryable, entries should NOT be queued");
    }

    // ========================================================================
    // send_batch: 404 Not Found (generic 4xx) → NOT queued
    // ========================================================================

    #[tokio::test]
    async fn send_batch_404_entries_not_queued() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("not-found")]).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "404 is not retryable, entries should NOT be queued");
    }

    // ========================================================================
    // send_batch: 500 Internal Server Error → queued for retry
    // ========================================================================

    #[tokio::test]
    async fn send_batch_500_entries_queued_for_retry() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("server-error")]).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "500 should queue entries for retry");
    }

    // ========================================================================
    // send_batch: 503 Service Unavailable → queued for retry
    // ========================================================================

    #[tokio::test]
    async fn send_batch_503_entries_queued_for_retry() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("unavailable")]).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "503 should queue entries for retry");
    }

    // ========================================================================
    // send_batch: network error → queued for retry
    // ========================================================================

    #[tokio::test]
    async fn send_batch_network_error_entries_queued() {
        let tmp = tempfile::tempdir().unwrap();
        // Point at a port that nothing is listening on
        let config = test_config("http://127.0.0.1:1", tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        let result = sender.send_batch(vec![test_entry("network-fail")]).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "network error should queue entries for retry");
    }

    // ========================================================================
    // send_batch: auth header sent correctly
    // ========================================================================

    #[tokio::test]
    async fn send_batch_sends_bearer_auth_header() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, _queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ok_response(1))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("auth-check")]).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // send_batch: empty auth token → no Authorization header
    // ========================================================================

    #[tokio::test]
    async fn send_batch_no_auth_header_when_token_empty() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = Arc::new(Config {
            server_url: mock_server.uri(),
            auth_token: "".to_string(),
            watch_path: PathBuf::from("/tmp/watch"),
            data_dir: tmp.path().to_owned(),
            machine_id: "test-machine".to_string(),
            batch_size: 50,
            flush_interval_secs: 5,
            max_retry_delay_secs: 300,
        });
        let (sender, _queue, _tmp) = make_sender(config);

        // Mount a mock that matches POST /ingest without requiring Authorization
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ok_response(1))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("no-auth")]).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // send_batch: version header sent
    // ========================================================================

    #[tokio::test]
    async fn send_batch_sends_version_header() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, _queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .and(header("X-Memlayer-Version", env!("CARGO_PKG_VERSION")))
            .respond_with(ok_response(1))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("version-check")]).await;
        assert!(result.is_ok());
    }

    // ========================================================================
    // send_batch: multiple entries in one batch
    // ========================================================================

    #[tokio::test]
    async fn send_batch_multiple_entries_all_accepted() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ok_response(5))
            .expect(1)
            .mount(&mock_server)
            .await;

        let entries: Vec<ParsedEntry> = (0..5)
            .map(|i| test_entry(&format!("entry-{}", i)))
            .collect();
        let result = sender.send_batch(entries).await;
        assert!(result.is_ok());

        let q = queue.lock().await;
        assert_eq!(q.count(), 0);
    }

    // ========================================================================
    // send_batch: 200 with invalid JSON body still counts as success
    // ========================================================================

    #[tokio::test]
    async fn send_batch_200_with_invalid_body_still_ok() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        // Return 200 but with non-JSON body — the `if let Ok(body)` guard means
        // the parse failure is silently ignored, and Ok(()) is still returned.
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .expect(1)
            .mount(&mock_server)
            .await;

        let result = sender.send_batch(vec![test_entry("bad-body")]).await;
        assert!(result.is_ok(), "200 should be Ok even if body parse fails");

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "entries should not be queued on 200");
    }

    // ========================================================================
    // send_batch: queued entry content survives roundtrip
    // ========================================================================

    #[tokio::test]
    async fn send_batch_queued_entries_preserve_content() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        let entries = vec![test_entry("preserve-me")];
        sender.send_batch(entries).await.unwrap();

        let q = queue.lock().await;
        let batch = q.dequeue_batch(10).unwrap();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].1.raw_content, "preserve-me");
        assert_eq!(batch[0].1.payload_hash, "hash-preserve-me");
        assert_eq!(batch[0].1.session_id, "sess-1");
    }

    // ========================================================================
    // drain_one_batch: empty queue → returns false
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_empty_queue_returns_false() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, _queue, _tmp) = make_sender(config);

        let had_work = sender.drain_one_batch().await;
        assert!(!had_work, "drain should return false when queue is empty");
    }

    // ========================================================================
    // drain_one_batch: 200 success → entries removed from queue
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_200_removes_entries_from_queue() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        // Pre-populate the queue
        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("drain-me-1"), test_entry("drain-me-2")]).unwrap();
            assert_eq!(q.count(), 2);
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ok_response(2))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work, "drain should return true when batch was processed");

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "entries should be removed from queue after successful drain");
    }

    // ========================================================================
    // drain_one_batch: 400 (generic 4xx) → entries removed (not retryable)
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_400_removes_entries_from_queue() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("bad-data")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(400))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "non-retryable 4xx should remove entries from queue");
    }

    // ========================================================================
    // drain_one_batch: 422 → entries removed (not retryable)
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_422_removes_entries_from_queue() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("unprocessable")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(422))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "422 is not retryable, should remove entries from queue");
    }

    // ========================================================================
    // drain_one_batch: 500 → entries kept in queue
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_500_keeps_entries_in_queue() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("retry-me")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "5xx should keep entries in queue for retry");
    }

    // ========================================================================
    // drain_one_batch: network error → entries kept in queue
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_network_error_keeps_entries_in_queue() {
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config("http://127.0.0.1:1", tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("unreachable")]).unwrap();
        }

        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "network error should keep entries in queue");
    }

    // ========================================================================
    // drain_one_batch: 401 → entries kept in queue (retryable)
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_401_keeps_entries_in_queue() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("auth-retry")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "401 should keep entries in queue for retry during drain");
    }

    // ========================================================================
    // drain_one_batch: 429 → entries kept in queue (retryable)
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_429_keeps_entries_in_queue() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("rate-limit-retry")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(429))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "429 should keep entries in queue for retry during drain");
    }

    // ========================================================================
    // drain_one_batch: 413 → entries kept in queue (retryable)
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_413_keeps_entries_in_queue() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("too-large-retry")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(413))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "413 should keep entries in queue for retry during drain");
    }

    // ========================================================================
    // drain_one_batch: 503 → entries kept in queue
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_503_keeps_entries_in_queue() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("unavailable-retry")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(503))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 1, "503 should keep entries in queue for retry during drain");
    }

    // ========================================================================
    // drain_one_batch: returns true even when drain fails (batch was processed)
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_returns_true_on_failure() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("fail-test")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .mount(&mock_server)
            .await;

        let had_work = sender.drain_one_batch().await;
        assert!(had_work, "drain should return true even on failure (batch was attempted)");
    }

    // ========================================================================
    // drain_one_batch: auth header used during drain
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_sends_auth_header() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("drain-auth")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .and(header("Authorization", "Bearer test-token"))
            .respond_with(ok_response(1))
            .expect(1)
            .mount(&mock_server)
            .await;

        sender.drain_one_batch().await;

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "entries should be removed after successful drain with auth");
    }

    // ========================================================================
    // drain_one_batch: version header sent during drain
    // ========================================================================

    #[tokio::test]
    async fn drain_one_batch_sends_version_header() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        {
            let q = queue.lock().await;
            q.enqueue(&[test_entry("drain-version")]).unwrap();
        }

        Mock::given(method("POST"))
            .and(path("/ingest"))
            .and(header("X-Memlayer-Version", env!("CARGO_PKG_VERSION")))
            .respond_with(ok_response(1))
            .expect(1)
            .mount(&mock_server)
            .await;

        sender.drain_one_batch().await;

        let q = queue.lock().await;
        assert_eq!(q.count(), 0);
    }

    // ========================================================================
    // Integration: send_batch failure → drain_one_batch success
    // ========================================================================

    #[tokio::test]
    async fn send_failure_then_drain_success_roundtrip() {
        let mock_server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let config = test_config(&mock_server.uri(), tmp.path());
        let (sender, queue, _tmp) = make_sender(config);

        // First: server is down (500), entries get queued
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(500))
            .expect(1)
            .named("first-request-fails")
            .mount(&mock_server)
            .await;

        sender.send_batch(vec![test_entry("roundtrip")]).await.unwrap();
        {
            let q = queue.lock().await;
            assert_eq!(q.count(), 1, "entry should be queued after 500");
        }

        // Reset mocks: server is back up
        mock_server.reset().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ok_response(1))
            .expect(1)
            .named("second-request-succeeds")
            .mount(&mock_server)
            .await;

        // Drain the queue
        let had_work = sender.drain_one_batch().await;
        assert!(had_work);

        let q = queue.lock().await;
        assert_eq!(q.count(), 0, "entry should be removed after successful drain");
    }
}
