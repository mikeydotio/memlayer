CREATE TABLE IF NOT EXISTS response_files (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    file_path VARCHAR(512) NOT NULL,
    size_bytes BIGINT NOT NULL,
    content_type VARCHAR(32) NOT NULL DEFAULT 'text',
    summary TEXT,
    structural_index TEXT,
    source_endpoint VARCHAR(128) NOT NULL,
    source_params JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_accessed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_response_files_lru
    ON response_files(last_accessed_at) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_response_files_tombstones
    ON response_files(deleted_at) WHERE deleted_at IS NOT NULL;
