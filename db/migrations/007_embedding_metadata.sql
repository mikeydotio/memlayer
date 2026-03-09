ALTER TABLE memory_entries ADD COLUMN IF NOT EXISTS embedding_provider VARCHAR(32);
ALTER TABLE memory_entries ADD COLUMN IF NOT EXISTS embedding_model VARCHAR(128);
