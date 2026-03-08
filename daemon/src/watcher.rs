use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn, error, debug};

use crate::config::Config;
use crate::cursor::CursorManager;
use crate::parser::{parse_jsonl_line, project_path_from_file, ParsedEntry};
use crate::sender::Sender;

pub struct Watcher {
    config: Arc<Config>,
    cursor: Arc<Mutex<CursorManager>>,
    sender: Arc<Sender>,
}

impl Watcher {
    pub fn new(
        config: Arc<Config>,
        cursor: Arc<Mutex<CursorManager>>,
        sender: Arc<Sender>,
    ) -> Self {
        Watcher { config, cursor, sender }
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
            for chunk in all_entries.chunks(self.config.batch_size) {
                self.sender.send_batch(chunk.to_vec()).await;
            }
            debug!(path = %path_str, entries = count, "Processed file");
        }

        // Update cursor
        {
            let cursor = self.cursor.lock().await;
            cursor.set_offset(&path_str, new_offset)?;
        }

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

    /// Start the file watcher. Runs until cancelled.
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
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

        loop {
            match tokio::time::timeout(tokio::time::Duration::from_secs(1), rx.recv()).await {
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

        Ok(())
    }
}
