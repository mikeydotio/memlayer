use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use tokio::sync::watch;
use tracing::{info, warn, error, debug};

use crate::config::Config;
use crate::cursor::CursorManager;
use crate::parser::{parse_jsonl_line, project_path_from_file, ParsedEntry};
use crate::sender::Sender;

pub struct Watcher {
    config: Arc<Config>,
    cursor: Arc<Mutex<CursorManager>>,
    sender: Arc<Sender>,
    stats_files_processed: AtomicU64,
    stats_entries_parsed: AtomicU64,
}

impl Watcher {
    pub fn new(
        config: Arc<Config>,
        cursor: Arc<Mutex<CursorManager>>,
        sender: Arc<Sender>,
    ) -> Self {
        Watcher {
            config,
            cursor,
            sender,
            stats_files_processed: AtomicU64::new(0),
            stats_entries_parsed: AtomicU64::new(0),
        }
    }

    /// Process a single JSONL file from its cursor position.
    pub async fn process_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let path_str = path.to_string_lossy().to_string();
        let project_path = project_path_from_file(path);

        // Skip subagent files
        if path_str.contains("/subagents/") {
            return Ok(());
        }

        let file_len = std::fs::metadata(path)?.len();

        let offset = {
            let cursor = self.cursor.lock().await;
            cursor.get_offset(&path_str)
        };

        // Handle file truncation (shouldn't happen, but be defensive)
        let offset = if offset > file_len {
            warn!(path = %path_str, "File truncated, resetting cursor");
            0
        } else {
            offset
        };

        if offset >= file_len {
            return Ok(()); // No new data
        }

        let mut file = std::fs::File::open(path)?;
        file.seek(SeekFrom::Start(offset))?;

        let reader = BufReader::new(&file);
        let mut all_entries: Vec<ParsedEntry> = Vec::new();
        let mut new_offset = offset;

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    warn!(error = %e, path = %path_str, "Error reading line");
                    break;
                }
            };

            new_offset += line.len() as u64 + 1; // +1 for newline

            if line.trim().is_empty() {
                continue;
            }

            let entries = parse_jsonl_line(line.as_bytes(), &project_path, &self.config.machine_id);
            all_entries.extend(entries);
        }

        // Send in batches
        if !all_entries.is_empty() {
            let count = all_entries.len();
            let mut all_ok = true;
            for chunk in all_entries.chunks(self.config.batch_size) {
                if let Err(e) = self.sender.send_batch(chunk.to_vec()).await {
                    error!(
                        path = %path_str,
                        error = %e,
                        "Failed to send or queue batch — not advancing cursor to prevent data loss"
                    );
                    all_ok = false;
                    break;
                }
            }
            if all_ok {
                debug!(path = %path_str, entries = count, "Processed file");
                self.stats_entries_parsed.fetch_add(count as u64, Ordering::Relaxed);
            } else {
                // Do NOT advance cursor — entries will be re-read on next cycle
                // (idempotent payload_hash dedup prevents duplicates for already-sent chunks)
                return Ok(());
            }
        }

        // Update cursor — only reached if all batches were sent or durably queued
        {
            let cursor = self.cursor.lock().await;
            cursor.set_offset(&path_str, new_offset)?;
        }

        self.stats_files_processed.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Scan all existing JSONL files and process from last cursor.
    pub async fn initial_scan(&self) -> Result<(), Box<dyn std::error::Error>> {
        let pattern = self.config.watch_path.join("*/*.jsonl");
        let pattern_str = pattern.to_string_lossy().to_string();

        let paths: Vec<PathBuf> = glob::glob(&pattern_str)?
            .filter_map(|r| r.ok())
            .filter(|p| !p.to_string_lossy().contains("/subagents/"))
            .collect();

        if paths.is_empty() {
            info!("No JSONL files found for initial scan");
            return Ok(());
        }

        info!(count = paths.len(), "Running initial scan");
        for path in paths {
            if let Err(e) = self.process_file(&path).await {
                error!(path = %path.display(), error = %e, "Error processing file in initial scan");
            }
        }

        Ok(())
    }

    /// Reset and return stats counters for periodic logging.
    fn take_stats(&self) -> (u64, u64) {
        let files = self.stats_files_processed.swap(0, Ordering::Relaxed);
        let entries = self.stats_entries_parsed.swap(0, Ordering::Relaxed);
        (files, entries)
    }

    /// Start the file watcher. Runs until shutdown is signaled or the channel closes.
    pub async fn run(&self, mut shutdown_rx: watch::Receiver<bool>) -> Result<(), Box<dyn std::error::Error>> {
        use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher as _};

        // Initial scan first
        self.initial_scan().await?;

        let (tx, mut rx) = tokio::sync::mpsc::channel::<PathBuf>(100);

        let watch_path = self.config.watch_path.clone();

        // Capture the Tokio handle before spawning the thread
        let rt = tokio::runtime::Handle::current();

        // Spawn blocking watcher in a dedicated thread
        std::thread::spawn(move || {
            let tx = tx;

            let mut watcher = RecommendedWatcher::new(
                move |res: Result<Event, notify::Error>| {
                    if let Ok(event) = res {
                        match event.kind {
                            EventKind::Modify(_) | EventKind::Create(_) => {
                                for path in event.paths {
                                    if path.extension().is_some_and(|e| e == "jsonl") {
                                        let tx = tx.clone();
                                        rt.spawn(async move {
                                            let _ = tx.send(path).await;
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                },
                NotifyConfig::default(),
            )
            .expect("Failed to create file watcher");

            watcher
                .watch(&watch_path, RecursiveMode::Recursive)
                .expect("Failed to watch directory");

            info!(path = %watch_path.display(), "File watcher started");

            // Keep the watcher alive
            loop {
                std::thread::park();
            }
        });

        // Process file change events with debouncing
        let mut debounce_map: std::collections::HashMap<PathBuf, tokio::time::Instant> =
            std::collections::HashMap::new();
        let debounce_duration = tokio::time::Duration::from_millis(500);
        let stats_interval = tokio::time::Duration::from_secs(60);
        let mut stats_timer = tokio::time::interval(stats_interval);
        // Skip the first tick (fires immediately)
        stats_timer.tick().await;

        loop {
            tokio::select! {
                result = tokio::time::timeout(tokio::time::Duration::from_secs(1), rx.recv()) => {
                    match result {
                        Ok(Some(path)) => {
                            let now = tokio::time::Instant::now();
                            if let Some(last) = debounce_map.get(&path) {
                                if now.duration_since(*last) < debounce_duration {
                                    continue;
                                }
                            }
                            debounce_map.insert(path.clone(), now);

                            if let Err(e) = self.process_file(&path).await {
                                error!(path = %path.display(), error = %e, "Error processing file change");
                            }
                        }
                        Ok(None) => break, // Channel closed
                        Err(_) => {} // Timeout, loop again
                    }
                }
                _ = stats_timer.tick() => {
                    let (files, entries) = self.take_stats();
                    let queue_depth = {
                        // We don't have direct access to queue here, so report file/entry stats
                        0u64 // Queue depth reported separately by drain loop
                    };
                    info!(
                        files_processed = files,
                        entries_parsed = entries,
                        "Periodic stats (last 60s)"
                    );
                    let _ = queue_depth; // suppress unused warning
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        info!("Shutdown: processing any remaining file events");
                        // Drain any pending events from the channel
                        while let Ok(path) = rx.try_recv() {
                            if let Err(e) = self.process_file(&path).await {
                                error!(path = %path.display(), error = %e, "Error processing file during shutdown");
                            }
                        }
                        info!("Shutdown: file watcher stopped");
                        return Ok(());
                    }
                }
            }
        }

        Ok(())
    }
}
