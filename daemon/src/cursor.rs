use rusqlite::{Connection, params};
use std::path::Path;

pub struct CursorManager {
    conn: Connection,
}

impl CursorManager {
    pub fn new(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("cursors.db");
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS cursors (
                file_path TEXT PRIMARY KEY,
                byte_offset INTEGER NOT NULL,
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"
        )?;
        Ok(CursorManager { conn })
    }

    pub fn get_offset(&self, file_path: &str) -> u64 {
        self.conn
            .query_row(
                "SELECT byte_offset FROM cursors WHERE file_path = ?1",
                params![file_path],
                |row| row.get(0),
            )
            .unwrap_or(0)
    }

    pub fn set_offset(&self, file_path: &str, offset: u64) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute(
            "INSERT INTO cursors (file_path, byte_offset, updated_at) VALUES (?1, ?2, datetime('now'))
             ON CONFLICT(file_path) DO UPDATE SET byte_offset = ?2, updated_at = datetime('now')",
            params![file_path, offset as i64],
        )?;
        Ok(())
    }

    pub fn remove(&self, file_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.conn.execute("DELETE FROM cursors WHERE file_path = ?1", params![file_path])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cursor_mgr() -> (CursorManager, tempfile::TempDir) {
        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let mgr = CursorManager::new(tmp.path()).expect("failed to create CursorManager");
        (mgr, tmp)
    }

    #[test]
    fn test_get_offset_returns_zero_for_unknown_file() {
        let (mgr, _tmp) = make_cursor_mgr();
        assert_eq!(mgr.get_offset("/no/such/file.jsonl"), 0);
    }

    #[test]
    fn test_set_and_get_offset_roundtrip() {
        let (mgr, _tmp) = make_cursor_mgr();
        mgr.set_offset("/a/b.jsonl", 12345).unwrap();
        assert_eq!(mgr.get_offset("/a/b.jsonl"), 12345);
    }

    #[test]
    fn test_set_offset_overwrites_previous() {
        let (mgr, _tmp) = make_cursor_mgr();
        mgr.set_offset("/file.jsonl", 100).unwrap();
        assert_eq!(mgr.get_offset("/file.jsonl"), 100);
        mgr.set_offset("/file.jsonl", 999).unwrap();
        assert_eq!(mgr.get_offset("/file.jsonl"), 999);
    }

    #[test]
    fn test_remove_deletes_entry() {
        let (mgr, _tmp) = make_cursor_mgr();
        mgr.set_offset("/file.jsonl", 42).unwrap();
        assert_eq!(mgr.get_offset("/file.jsonl"), 42);
        mgr.remove("/file.jsonl").unwrap();
        assert_eq!(mgr.get_offset("/file.jsonl"), 0);
    }

    #[test]
    fn test_remove_nonexistent_is_ok() {
        let (mgr, _tmp) = make_cursor_mgr();
        // Removing a key that was never set should not error
        mgr.remove("/never/set.jsonl").unwrap();
    }

    #[test]
    fn test_multiple_files_independent() {
        let (mgr, _tmp) = make_cursor_mgr();
        mgr.set_offset("/a.jsonl", 10).unwrap();
        mgr.set_offset("/b.jsonl", 20).unwrap();
        assert_eq!(mgr.get_offset("/a.jsonl"), 10);
        assert_eq!(mgr.get_offset("/b.jsonl"), 20);
    }

    #[test]
    fn test_large_offset_value() {
        let (mgr, _tmp) = make_cursor_mgr();
        let large: u64 = u64::MAX / 2; // large but fits in i64
        mgr.set_offset("/big.jsonl", large).unwrap();
        assert_eq!(mgr.get_offset("/big.jsonl"), large);
    }

    #[test]
    fn test_zero_offset() {
        let (mgr, _tmp) = make_cursor_mgr();
        mgr.set_offset("/z.jsonl", 0).unwrap();
        assert_eq!(mgr.get_offset("/z.jsonl"), 0);
    }
}
