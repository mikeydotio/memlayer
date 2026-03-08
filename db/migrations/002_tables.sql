CREATE TABLE IF NOT EXISTS claude_sessions (
    session_id VARCHAR(64) PRIMARY KEY,
    project_path VARCHAR(512),
    client_machine_id VARCHAR(128),
    slug VARCHAR(128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS memory_entries (
    id BIGSERIAL PRIMARY KEY,
    session_id VARCHAR(64) NOT NULL REFERENCES claude_sessions(session_id),
    message_type VARCHAR(32) NOT NULL,
    content_type VARCHAR(32) NOT NULL,
    raw_content TEXT NOT NULL,
    payload_hash VARCHAR(64) NOT NULL UNIQUE,
    source_uuid VARCHAR(64),
    parent_uuid VARCHAR(64),
    tool_name VARCHAR(128),
    cwd VARCHAR(512),
    created_at TIMESTAMPTZ NOT NULL,
    ingested_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    fts tsvector GENERATED ALWAYS AS (to_tsvector('english', raw_content)) STORED,
    embedding vector(1536)
);
