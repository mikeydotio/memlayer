use std::sync::Arc;
use tokio::sync::Mutex;
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

    pub async fn drain_queue_loop(&self) {
        let mut delay_secs = 30u64;

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;

            let batch = {
                let q = self.queue.lock().await;
                let count = q.count();
                if count == 0 {
                    delay_secs = 30;
                    continue;
                }
                info!(queued = count, "Draining offline queue");
                match q.dequeue_batch(self.config.batch_size) {
                    Ok(b) => b,
                    Err(e) => {
                        error!(error = %e, "Failed to dequeue batch");
                        continue;
                    }
                }
            };

            if batch.is_empty() {
                continue;
            }

            let ids: Vec<i64> = batch.iter().map(|(id, _)| *id).collect();
            let entries: Vec<ParsedEntry> = batch.into_iter().map(|(_, e)| e).collect();

            let url = format!("{}/ingest", self.config.server_url);
            let req = IngestRequest { entries };

            let mut builder = self.client.post(&url).json(&req);
            if !self.config.auth_token.is_empty() {
                builder = builder.header("Authorization", format!("Bearer {}", self.config.auth_token));
            }

            match builder.send().await {
                Ok(resp) if resp.status().is_success() => {
                    let q = self.queue.lock().await;
                    if let Err(e) = q.remove(&ids) {
                        error!(error = %e, "Failed to remove drained entries");
                    }
                    info!(count = ids.len(), "Queue drained successfully");
                    delay_secs = 30; // Reset on success
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
                    delay_secs = 30; // Reset — not a transient error
                }
                Ok(resp) => {
                    // 5xx: server error — back off and retry
                    warn!(status = %resp.status(), "Queue drain failed with server error, backing off");
                    delay_secs = (delay_secs * 2).min(self.config.max_retry_delay_secs);
                }
                Err(e) => {
                    warn!(error = %e, "Queue drain failed, backing off");
                    delay_secs = (delay_secs * 2).min(self.config.max_retry_delay_secs);
                }
            }
        }
    }
}
