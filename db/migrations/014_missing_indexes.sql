-- 014: Add missing indexes identified during DBA review
--
-- idx_entries_embedding_null: Speeds up embedding backfill queries that scan
--   for entries without embeddings (previously required full table scan).
-- idx_extraction_log_status: Speeds up extraction worker's pending-entry detection.

CREATE INDEX IF NOT EXISTS idx_entries_embedding_null
    ON memory_entries(id) WHERE embedding IS NULL;

CREATE INDEX IF NOT EXISTS idx_extraction_log_status
    ON extraction_log(status) WHERE status IN ('pending', 'processing');
