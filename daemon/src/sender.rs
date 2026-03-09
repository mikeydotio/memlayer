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
