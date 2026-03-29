use std::fs;
use std::path::{Path, PathBuf};

const SOFT_LIMIT: u64 = 50 * 1024 * 1024; // 50MB
const HARD_LIMIT: u64 = 100 * 1024 * 1024; // 100MB

struct CacheEntry {
    file_path: PathBuf,
    size: u64,
    mtime_ms: u128,
}

pub struct FileCache {
    cache_dir: PathBuf,
}

impl FileCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        FileCache { cache_dir }
    }

    fn ensure_dir(&self) {
        if !self.cache_dir.exists() {
            let _ = fs::create_dir_all(&self.cache_dir);
        }
    }

    fn file_path(&self, file_id: &str) -> PathBuf {
        self.cache_dir.join(format!("{file_id}.txt"))
    }

    /// Scan cache directory, return entries sorted oldest-first (FIFO).
    fn scan_entries(&self) -> Vec<CacheEntry> {
        self.ensure_dir();
        let mut entries = Vec::new();
        if let Ok(read_dir) = fs::read_dir(&self.cache_dir) {
            for entry in read_dir.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        let mtime_ms = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis())
                            .unwrap_or(0);
                        entries.push(CacheEntry {
                            file_path: entry.path(),
                            size: meta.len(),
                            mtime_ms,
                        });
                    }
                }
            }
        }
        entries.sort_by_key(|e| e.mtime_ms);
        entries
    }

    /// FIFO eviction: remove oldest files until at or below target_bytes.
    fn evict_to(&self, target_bytes: u64) {
        let entries = self.scan_entries();
        let mut total: u64 = entries.iter().map(|e| e.size).sum();

        for entry in &entries {
            if total <= target_bytes {
                break;
            }
            if fs::remove_file(&entry.file_path).is_ok() {
                total = total.saturating_sub(entry.size);
            }
        }
    }

    fn cache_size(&self) -> u64 {
        self.scan_entries().iter().map(|e| e.size).sum()
    }

    /// Ensure file is cached locally. Returns the local path.
    /// If not cached, calls `download_fn` to fetch content and writes it.
    pub async fn ensure_cached<F, Fut>(&self, file_id: &str, download_fn: F) -> Result<PathBuf, String>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<String, String>>,
    {
        self.ensure_dir();
        let local_path = self.file_path(file_id);

        if local_path.exists() {
            return Ok(local_path);
        }

        let content = download_fn().await?;
        let content_bytes = content.len() as u64;

        // Hard limit: sync eviction
        let current = self.cache_size();
        if current + content_bytes > HARD_LIMIT {
            self.evict_to(HARD_LIMIT.saturating_sub(content_bytes));
        }

        fs::write(&local_path, &content)
            .map_err(|e| format!("Failed to write cache file: {e}"))?;

        // Soft limit: background eviction
        if self.cache_size() > SOFT_LIMIT {
            self.evict_to(SOFT_LIMIT);
        }

        Ok(local_path)
    }

    /// Read a line range from a cached file. 1-indexed, inclusive.
    pub fn read_lines(path: &Path, start_line: usize, end_line: usize) -> Result<String, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read cached file: {e}"))?;
        let lines: Vec<&str> = content.split('\n').collect();
        let start = start_line.max(1) - 1;
        let end = end_line.min(lines.len());
        Ok(lines[start..end].join("\n"))
    }
}
