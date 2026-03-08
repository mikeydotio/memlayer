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
