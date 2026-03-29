-- Composite index for cursor-based entry pagination in the browse endpoint
CREATE INDEX IF NOT EXISTS idx_entries_session_id_id
    ON memory_entries(session_id, id);
