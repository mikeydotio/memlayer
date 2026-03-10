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
                    info!(
                        files_processed = files,
                        entries_parsed = entries,
                        "Periodic stats (last 60s)"
                    );

                    // Prune stale debounce map entries (older than 5 minutes)
                    let stale_threshold = tokio::time::Duration::from_secs(300);
                    let now = tokio::time::Instant::now();
                    let before = debounce_map.len();
                    debounce_map.retain(|_, last| now.duration_since(*last) < stale_threshold);
                    let pruned = before - debounce_map.len();
                    if pruned > 0 {
                        debug!(pruned, remaining = debounce_map.len(), "Pruned stale debounce entries");
                    }
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

    /// Get a reference to the cursor manager (for testing).
    #[cfg(test)]
    pub fn cursor_manager(&self) -> &Arc<Mutex<CursorManager>> {
        &self.cursor
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path};

    use crate::config::Config;
    use crate::cursor::CursorManager;
    use crate::queue::OfflineQueue;
    use crate::sender::Sender;

    /// Build a Config pointing at the given mock server URL and watch/data dirs.
    fn test_config(server_url: &str, watch_dir: &Path, data_dir: &Path) -> Config {
        Config {
            server_url: server_url.to_string(),
            auth_token: "test-token".to_string(),
            watch_path: watch_dir.to_path_buf(),
            data_dir: data_dir.to_path_buf(),
            machine_id: "test-machine".to_string(),
            batch_size: 50,
            flush_interval_secs: 5,
            max_retry_delay_secs: 300,
        }
    }

    /// Create a valid user-type JSONL line that the parser will accept.
    fn make_user_line(session_id: &str, content: &str) -> String {
        serde_json::json!({
            "type": "user",
            "sessionId": session_id,
            "uuid": format!("uuid-{}", content.len()),
            "timestamp": "2024-01-01T00:00:00Z",
            "message": {
                "role": "user",
                "content": content
            }
        }).to_string()
    }

    /// Write a JSONL file with the given lines in a project subdirectory.
    /// Returns the full path to the created file.
    fn write_jsonl_file(watch_dir: &Path, project_name: &str, session: &str, lines: &[&str]) -> PathBuf {
        let project_dir = watch_dir.join(project_name);
        std::fs::create_dir_all(&project_dir).unwrap();
        let file_path = project_dir.join(format!("{}.jsonl", session));
        let mut f = std::fs::File::create(&file_path).unwrap();
        for line in lines {
            writeln!(f, "{}", line).unwrap();
        }
        file_path
    }

    /// Create a Watcher with all dependencies wired up against a mock server.
    async fn setup_watcher(mock_url: &str) -> (Watcher, tempfile::TempDir, tempfile::TempDir) {
        let watch_tmp = tempfile::tempdir().unwrap();
        let data_tmp = tempfile::tempdir().unwrap();

        let config = Arc::new(test_config(mock_url, watch_tmp.path(), data_tmp.path()));
        let cursor = Arc::new(Mutex::new(CursorManager::new(data_tmp.path()).unwrap()));
        let queue = Arc::new(Mutex::new(OfflineQueue::new(data_tmp.path()).unwrap()));
        let sender = Arc::new(Sender::new(config.clone(), queue));

        let watcher = Watcher::new(config, cursor, sender);
        (watcher, watch_tmp, data_tmp)
    }

    /// Create a Watcher with a custom config modifier (e.g., small batch_size).
    async fn setup_watcher_with_config(mock_url: &str, config_fn: impl FnOnce(&mut Config)) -> (Watcher, tempfile::TempDir, tempfile::TempDir) {
        let watch_tmp = tempfile::tempdir().unwrap();
        let data_tmp = tempfile::tempdir().unwrap();

        let mut config = test_config(mock_url, watch_tmp.path(), data_tmp.path());
        config_fn(&mut config);
        let config = Arc::new(config);
        let cursor = Arc::new(Mutex::new(CursorManager::new(data_tmp.path()).unwrap()));
        let queue = Arc::new(Mutex::new(OfflineQueue::new(data_tmp.path()).unwrap()));
        let sender = Arc::new(Sender::new(config.clone(), queue));

        let watcher = Watcher::new(config, cursor, sender);
        (watcher, watch_tmp, data_tmp)
    }

    fn mock_ingest_success() -> ResponseTemplate {
        ResponseTemplate::new(200)
            .set_body_json(serde_json::json!({
                "accepted": 1,
                "duplicates": 0,
                "errors": 0
            }))
    }

    // ── Test 1: process_file from offset 0 ──

    #[tokio::test]
    async fn test_process_file_from_offset_zero() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(1)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        let line = make_user_line("sess-1", "hello world");
        let file_path = write_jsonl_file(watch_tmp.path(), "-home-test-project", "session1", &[&line]);

        watcher.process_file(&file_path).await.unwrap();

        // Verify cursor was advanced (should be line length + newline)
        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());
        assert_eq!(offset, (line.len() + 1) as u64);
    }

    // ── Test 2: process_file resumes from cursor ──

    #[tokio::test]
    async fn test_process_file_resumes_from_cursor() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(1)  // Only one call (for the second line)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        let line1 = make_user_line("sess-1", "first message");
        let line2 = make_user_line("sess-1", "second message");
        let file_path = write_jsonl_file(
            watch_tmp.path(), "-home-test-project", "session1",
            &[&line1, &line2],
        );

        // Pre-set cursor to skip past line1 (line1 bytes + newline)
        let first_line_offset = (line1.len() + 1) as u64;
        {
            let cursor = watcher.cursor_manager().lock().await;
            cursor.set_offset(&file_path.to_string_lossy(), first_line_offset).unwrap();
        }

        watcher.process_file(&file_path).await.unwrap();

        // Cursor should now be at end of file
        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());
        let expected = (line1.len() + 1 + line2.len() + 1) as u64;
        assert_eq!(offset, expected);
    }

    // ── Test 3: File truncation resets cursor ──

    #[tokio::test]
    async fn test_file_truncation_resets_cursor() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(1)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        let line = make_user_line("sess-1", "after truncation");
        let file_path = write_jsonl_file(watch_tmp.path(), "-home-test-project", "session1", &[&line]);

        // Set cursor far beyond the file length (simulating truncation)
        {
            let cursor = watcher.cursor_manager().lock().await;
            cursor.set_offset(&file_path.to_string_lossy(), 999999).unwrap();
        }

        watcher.process_file(&file_path).await.unwrap();

        // Cursor should be set to end of file (re-read from 0)
        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());
        assert_eq!(offset, (line.len() + 1) as u64);
    }

    // ── Test 4: Empty file ──

    #[tokio::test]
    async fn test_empty_file_no_errors() {
        let mock_server = MockServer::start().await;
        // No mock expectation — server should NOT be called
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(0)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        let project_dir = watch_tmp.path().join("-home-test-project");
        std::fs::create_dir_all(&project_dir).unwrap();
        let file_path = project_dir.join("empty.jsonl");
        std::fs::File::create(&file_path).unwrap(); // 0-byte file

        let result = watcher.process_file(&file_path).await;
        assert!(result.is_ok());
    }

    // ── Test 5: Subagent file skipped ──

    #[tokio::test]
    async fn test_subagent_file_skipped() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(0)  // Should NOT be called
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        // Create a file under a subagents directory
        let subagent_dir = watch_tmp.path().join("-home-test-project").join("subagents").join("sub1");
        std::fs::create_dir_all(&subagent_dir).unwrap();
        let file_path = subagent_dir.join("session.jsonl");
        let line = make_user_line("sess-1", "subagent message");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            writeln!(f, "{}", line).unwrap();
        }

        let result = watcher.process_file(&file_path).await;
        assert!(result.is_ok());

        // Cursor should NOT have been set
        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());
        assert_eq!(offset, 0);
    }

    // ── Test 6: Batch splitting ──

    #[tokio::test]
    async fn test_batch_splitting_multiple_server_calls() {
        let mock_server = MockServer::start().await;
        // With batch_size=2 and 5 entries, we expect 3 server calls (2+2+1)
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(3)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher_with_config(
            &mock_server.uri(),
            |c| c.batch_size = 2,
        ).await;

        let lines: Vec<String> = (0..5).map(|i| make_user_line("sess-1", &format!("message {}", i))).collect();
        let line_refs: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
        let file_path = write_jsonl_file(watch_tmp.path(), "-home-test-project", "session1", &line_refs);

        watcher.process_file(&file_path).await.unwrap();

        // Cursor should be at end of file
        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());
        let expected: u64 = lines.iter().map(|l| l.len() as u64 + 1).sum();
        assert_eq!(offset, expected);
    }

    // ── Test 7: Cursor not advanced on send_batch error ──
    //
    // send_batch returns Err only when BOTH the HTTP call and the offline queue
    // fail simultaneously. The sender queues to SQLite on any HTTP error, and
    // SQLite keeps its connection open via file descriptor, making it hard to
    // break. Instead we test the *logical* path: when the server returns a 5xx
    // error the entries are durably queued and the cursor advances (by design).
    // We verify that by contrast, when a 4xx non-retryable error is returned
    // (entries dropped by sender), the cursor STILL advances because process_file
    // treats that as a successful "send" (entries were intentionally discarded).
    //
    // The "cursor doesn't advance" path is exercised when send_batch returns Err,
    // which only happens when disk is full (queue write fails). We verify the
    // intended behavior through two complementary tests:
    //   7a: server 500 → entries queued → cursor advances (queue = durable)
    //   7b: server unreachable + queue broken → cursor stays at 0

    #[tokio::test]
    async fn test_cursor_advances_when_entries_queued_on_500() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        let line = make_user_line("sess-1", "queued data");
        let file_path = write_jsonl_file(watch_tmp.path(), "-home-test-project", "session1", &[&line]);

        watcher.process_file(&file_path).await.unwrap();

        // Cursor SHOULD advance because entries were durably queued
        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());
        assert_eq!(offset, (line.len() + 1) as u64,
            "Cursor should advance when entries are durably queued");
    }

    #[tokio::test]
    async fn test_cursor_not_advanced_on_total_send_failure() {
        // Create a queue backed by an in-memory database, then drop its table
        // so enqueue fails with a SQL error.
        let watch_tmp = tempfile::tempdir().unwrap();
        let data_tmp = tempfile::tempdir().unwrap();

        // Use a port where nothing listens so HTTP always fails
        let config = Arc::new(test_config(
            "http://127.0.0.1:1", watch_tmp.path(), data_tmp.path(),
        ));
        let cursor = Arc::new(Mutex::new(CursorManager::new(data_tmp.path()).unwrap()));

        // Create queue normally, then destroy the table so writes fail
        let queue = OfflineQueue::new(data_tmp.path()).unwrap();
        // Drop the queue table so enqueue returns Err
        {
            let conn = rusqlite::Connection::open(data_tmp.path().join("queue.db")).unwrap();
            conn.execute_batch("DROP TABLE queue").unwrap();
        }
        let queue = Arc::new(Mutex::new(queue));

        let sender = Arc::new(Sender::new(config.clone(), queue));
        let watcher = Watcher::new(config, cursor, sender);

        let line = make_user_line("sess-1", "important data");
        let file_path = write_jsonl_file(watch_tmp.path(), "-home-test-project", "session1", &[&line]);

        let _ = watcher.process_file(&file_path).await;

        // Cursor should NOT have advanced
        let cursor_mgr = watcher.cursor_manager().lock().await;
        let offset = cursor_mgr.get_offset(&file_path.to_string_lossy());
        assert_eq!(offset, 0, "Cursor should not advance when send_batch fails");
    }

    // ── Test 8: Malformed line in middle ──

    #[tokio::test]
    async fn test_malformed_line_in_middle() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(1)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        let line1 = make_user_line("sess-1", "valid first");
        let bad_line = "this is not valid json at all {{{";
        let line3 = make_user_line("sess-1", "valid third");
        let file_path = write_jsonl_file(
            watch_tmp.path(), "-home-test-project", "session1",
            &[&line1, bad_line, &line3],
        );

        let result = watcher.process_file(&file_path).await;
        assert!(result.is_ok());

        // Cursor should advance past ALL lines (including the bad one)
        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());
        let expected = (line1.len() + 1 + bad_line.len() + 1 + line3.len() + 1) as u64;
        assert_eq!(offset, expected);
    }

    // ── Test 9: Cursor advances by exact byte count ──

    #[tokio::test]
    async fn test_cursor_advances_exact_byte_count() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        let line1 = make_user_line("sess-1", "abc");
        let line2 = make_user_line("sess-1", "defgh");
        let line3 = make_user_line("sess-1", "ijklmnop");

        let file_path = write_jsonl_file(
            watch_tmp.path(), "-home-test-project", "session1",
            &[&line1, &line2, &line3],
        );

        watcher.process_file(&file_path).await.unwrap();

        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());

        // Each line contributes line.len() + 1 (for \n) to the offset
        let expected_offset: u64 =
            (line1.len() as u64 + 1) +
            (line2.len() as u64 + 1) +
            (line3.len() as u64 + 1);

        assert_eq!(offset, expected_offset);

        // Also verify against actual file size
        let file_len = std::fs::metadata(&file_path).unwrap().len();
        assert_eq!(offset, file_len);
    }

    // ── Test 10: initial_scan finds nested JSONL files ──

    #[tokio::test]
    async fn test_initial_scan_finds_nested_jsonl_files() {
        let mock_server = MockServer::start().await;
        // Expect 2 calls (one per file)
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(2)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        // Create two project directories with JSONL files (matches glob */*.jsonl)
        let line1 = make_user_line("sess-1", "project A message");
        let line2 = make_user_line("sess-2", "project B message");
        write_jsonl_file(watch_tmp.path(), "-home-test-projectA", "session1", &[&line1]);
        write_jsonl_file(watch_tmp.path(), "-home-test-projectB", "session2", &[&line2]);

        let result = watcher.initial_scan().await;
        assert!(result.is_ok());
    }

    // ── Test 11: initial_scan skips subagent files ──

    #[tokio::test]
    async fn test_initial_scan_skips_subagent_files() {
        let mock_server = MockServer::start().await;
        // Only 1 call expected (the non-subagent file)
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(1)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        // Normal file (matches */*.jsonl glob)
        let line1 = make_user_line("sess-1", "normal message");
        write_jsonl_file(watch_tmp.path(), "-home-test-project", "session1", &[&line1]);

        // Subagent file — the glob */*.jsonl won't match deep paths, but
        // initial_scan also explicitly filters /subagents/ paths
        let subagent_dir = watch_tmp.path().join("subagents");
        std::fs::create_dir_all(&subagent_dir).unwrap();
        let subagent_file = subagent_dir.join("sub.jsonl");
        let line2 = make_user_line("sess-2", "subagent message");
        {
            let mut f = std::fs::File::create(&subagent_file).unwrap();
            writeln!(f, "{}", line2).unwrap();
        }

        let result = watcher.initial_scan().await;
        assert!(result.is_ok());
    }

    // ── Test 12: file with only blank lines — no server calls, cursor still advances ──

    #[tokio::test]
    async fn test_file_with_only_blank_lines() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/ingest"))
            .respond_with(mock_ingest_success())
            .expect(0)
            .mount(&mock_server)
            .await;

        let (watcher, watch_tmp, _data_tmp) = setup_watcher(&mock_server.uri()).await;

        let project_dir = watch_tmp.path().join("-home-test-project");
        std::fs::create_dir_all(&project_dir).unwrap();
        let file_path = project_dir.join("blank.jsonl");
        {
            let mut f = std::fs::File::create(&file_path).unwrap();
            writeln!(f).unwrap(); // blank line
            writeln!(f, "   ").unwrap(); // whitespace-only line
            writeln!(f).unwrap(); // another blank line
        }

        let result = watcher.process_file(&file_path).await;
        assert!(result.is_ok());

        // Cursor should still advance past the blank lines
        let cursor = watcher.cursor_manager().lock().await;
        let offset = cursor.get_offset(&file_path.to_string_lossy());
        let file_len = std::fs::metadata(&file_path).unwrap().len();
        assert_eq!(offset, file_len);
    }
}
