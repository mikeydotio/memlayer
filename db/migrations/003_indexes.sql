-- B-Tree indexes for exact lookups and range scans
CREATE INDEX IF NOT EXISTS idx_entries_session_id ON memory_entries(session_id);
CREATE INDEX IF NOT EXISTS idx_entries_created_at ON memory_entries(created_at);
CREATE INDEX IF NOT EXISTS idx_entries_content_type ON memory_entries(content_type);
CREATE INDEX IF NOT EXISTS idx_entries_tool_name ON memory_entries(tool_name) WHERE tool_name IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_sessions_project_path ON claude_sessions(project_path);
CREATE INDEX IF NOT EXISTS idx_sessions_slug ON claude_sessions(slug);

-- GIN index for full-text search
CREATE INDEX IF NOT EXISTS idx_entries_fts ON memory_entries USING GIN(fts);

-- HNSW index for vector similarity search (cosine distance)
CREATE INDEX IF NOT EXISTS idx_entries_embedding ON memory_entries
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 128);
