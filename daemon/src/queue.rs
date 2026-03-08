use rusqlite::{Connection, params};
use std::path::Path;

use crate::parser::ParsedEntry;

pub struct OfflineQueue {
    conn: Connection,
}

impl OfflineQueue {
    pub fn new(data_dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
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
        Ok(OfflineQueue { conn })
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
