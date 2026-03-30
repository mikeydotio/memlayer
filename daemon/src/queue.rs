use rusqlite::{Connection, params};
use std::path::Path;

use crate::parser::ParsedEntry;

pub struct OfflineQueue {
    conn: Connection,
    max_size: usize,
}

impl OfflineQueue {
    pub fn new(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let max_size = std::env::var("MEMLAYER_QUEUE_MAX_SIZE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(50_000);
        Self::with_max_size(data_dir, max_size)
    }

    pub fn with_max_size(data_dir: &Path, max_size: usize) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("queue.db");
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS queue (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                retry_count INTEGER DEFAULT 0
            )"
        )?;
        Ok(OfflineQueue { conn, max_size })
    }

    pub fn enqueue(&self, entries: &[ParsedEntry]) -> Result<(), Box<dyn std::error::Error>> {
        let tx = self.conn.unchecked_transaction()?;
        for entry in entries {
            let json = serde_json::to_string(entry)?;
            tx.execute(
                "INSERT INTO queue (payload) VALUES (?1)",
                params![json],
            )?;
        }
        tx.commit()?;

        // Evict oldest entries if queue exceeds max size
        if self.max_size > 0 {
            let count = self.count();
            if count > self.max_size {
                let excess = count - self.max_size;
                self.conn.execute(
                    "DELETE FROM queue WHERE id IN (SELECT id FROM queue ORDER BY id ASC LIMIT ?1)",
                    params![excess as i64],
                )?;
                tracing::warn!(evicted = excess, max_size = self.max_size, "Queue exceeded max size, evicted oldest entries");
            }
        }
        Ok(())
    }

    pub fn dequeue_batch(&self, limit: usize) -> Result<Vec<(i64, ParsedEntry)>, Box<dyn std::error::Error>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, payload FROM queue ORDER BY id ASC LIMIT ?1"
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let id: i64 = row.get(0)?;
            let payload: String = row.get(1)?;
            Ok((id, payload))
        })?;

        let mut result = Vec::new();
        for row in rows {
            let (id, payload) = row?;
            let entry: ParsedEntry = serde_json::from_str(&payload)?;
            result.push((id, entry));
        }
        Ok(result)
    }

    pub fn remove(&self, ids: &[i64]) -> Result<(), Box<dyn std::error::Error>> {
        if ids.is_empty() {
            return Ok(());
        }
        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!("DELETE FROM queue WHERE id IN ({})", placeholders.join(","));
        let params: Vec<Box<dyn rusqlite::types::ToSql>> = ids.iter().map(|id| Box::new(*id) as Box<dyn rusqlite::types::ToSql>).collect();
        self.conn.execute(&sql, rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())))?;
        Ok(())
    }

    pub fn count(&self) -> usize {
        self.conn
            .query_row("SELECT COUNT(*) FROM queue", [], |row| row.get(0))
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_queue() -> (OfflineQueue, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let q = OfflineQueue::new(tmp.path()).expect("failed to create OfflineQueue");
        (q, tmp)
    }

    fn sample_entry(content: &str) -> ParsedEntry {
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

    #[test]
    fn test_empty_queue_count_is_zero() {
        let (q, _tmp) = make_queue();
        assert_eq!(q.count(), 0);
    }

    #[test]
    fn test_empty_queue_dequeue_returns_empty() {
        let (q, _tmp) = make_queue();
        let batch = q.dequeue_batch(10).unwrap();
        assert!(batch.is_empty());
    }

    #[test]
    fn test_enqueue_and_dequeue_roundtrip() {
        let (q, _tmp) = make_queue();
        let entries = vec![sample_entry("hello"), sample_entry("world")];
        q.enqueue(&entries).unwrap();
        assert_eq!(q.count(), 2);

        let batch = q.dequeue_batch(10).unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].1.raw_content, "hello");
        assert_eq!(batch[1].1.raw_content, "world");
    }

    #[test]
    fn test_dequeue_respects_limit() {
        let (q, _tmp) = make_queue();
        let entries = vec![
            sample_entry("a"),
            sample_entry("b"),
            sample_entry("c"),
            sample_entry("d"),
        ];
        q.enqueue(&entries).unwrap();
        assert_eq!(q.count(), 4);

        let batch = q.dequeue_batch(2).unwrap();
        assert_eq!(batch.len(), 2);
        assert_eq!(batch[0].1.raw_content, "a");
        assert_eq!(batch[1].1.raw_content, "b");
    }

    #[test]
    fn test_dequeue_preserves_order() {
        let (q, _tmp) = make_queue();
        let entries = vec![
            sample_entry("first"),
            sample_entry("second"),
            sample_entry("third"),
        ];
        q.enqueue(&entries).unwrap();

        let batch = q.dequeue_batch(3).unwrap();
        assert_eq!(batch[0].1.raw_content, "first");
        assert_eq!(batch[1].1.raw_content, "second");
        assert_eq!(batch[2].1.raw_content, "third");
        // IDs should be ascending
        assert!(batch[0].0 < batch[1].0);
        assert!(batch[1].0 < batch[2].0);
    }

    #[test]
    fn test_count_after_enqueue() {
        let (q, _tmp) = make_queue();
        assert_eq!(q.count(), 0);
        q.enqueue(&[sample_entry("a")]).unwrap();
        assert_eq!(q.count(), 1);
        q.enqueue(&[sample_entry("b"), sample_entry("c")]).unwrap();
        assert_eq!(q.count(), 3);
    }

    #[test]
    fn test_remove_deletes_specific_entries() {
        let (q, _tmp) = make_queue();
        q.enqueue(&[sample_entry("a"), sample_entry("b"), sample_entry("c")]).unwrap();
        assert_eq!(q.count(), 3);

        let batch = q.dequeue_batch(10).unwrap();
        // Remove only the first and third
        let ids_to_remove = vec![batch[0].0, batch[2].0];
        q.remove(&ids_to_remove).unwrap();
        assert_eq!(q.count(), 1);

        // The remaining entry should be "b"
        let remaining = q.dequeue_batch(10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].1.raw_content, "b");
    }

    #[test]
    fn test_remove_empty_ids_is_ok() {
        let (q, _tmp) = make_queue();
        q.enqueue(&[sample_entry("a")]).unwrap();
        q.remove(&[]).unwrap();
        assert_eq!(q.count(), 1);
    }

    #[test]
    fn test_enqueue_empty_slice() {
        let (q, _tmp) = make_queue();
        q.enqueue(&[]).unwrap();
        assert_eq!(q.count(), 0);
    }

    #[test]
    fn test_dequeue_does_not_remove_entries() {
        // Dequeue is peek-like; entries remain until explicitly removed
        let (q, _tmp) = make_queue();
        q.enqueue(&[sample_entry("a")]).unwrap();

        let batch1 = q.dequeue_batch(10).unwrap();
        assert_eq!(batch1.len(), 1);
        assert_eq!(q.count(), 1); // Still there

        let batch2 = q.dequeue_batch(10).unwrap();
        assert_eq!(batch2.len(), 1);
        assert_eq!(batch2[0].0, batch1[0].0); // Same ID
    }

    #[test]
    fn test_entry_roundtrip_preserves_all_fields() {
        let (q, _tmp) = make_queue();
        let entry = ParsedEntry {
            payload_hash: "hash-abc".to_string(),
            session_id: "sess-99".to_string(),
            message_type: "assistant".to_string(),
            content_type: "tool_use".to_string(),
            raw_content: "some content".to_string(),
            timestamp: "2024-06-15T12:00:00Z".to_string(),
            project_path: "/home/mikey/project".to_string(),
            client_machine_id: "myhost".to_string(),
            slug: Some("my-slug".to_string()),
            source_uuid: Some("uuid-1".to_string()),
            parent_uuid: Some("uuid-0".to_string()),
            tool_name: Some("Bash".to_string()),
            cwd: Some("/tmp".to_string()),
            git_branch: Some("feature".to_string()),
        };
        q.enqueue(&[entry]).unwrap();

        let batch = q.dequeue_batch(1).unwrap();
        let e = &batch[0].1;
        assert_eq!(e.payload_hash, "hash-abc");
        assert_eq!(e.session_id, "sess-99");
        assert_eq!(e.message_type, "assistant");
        assert_eq!(e.content_type, "tool_use");
        assert_eq!(e.raw_content, "some content");
        assert_eq!(e.timestamp, "2024-06-15T12:00:00Z");
        assert_eq!(e.project_path, "/home/mikey/project");
        assert_eq!(e.client_machine_id, "myhost");
        assert_eq!(e.slug.as_deref(), Some("my-slug"));
        assert_eq!(e.source_uuid.as_deref(), Some("uuid-1"));
        assert_eq!(e.parent_uuid.as_deref(), Some("uuid-0"));
        assert_eq!(e.tool_name.as_deref(), Some("Bash"));
        assert_eq!(e.cwd.as_deref(), Some("/tmp"));
        assert_eq!(e.git_branch.as_deref(), Some("feature"));
    }
}
