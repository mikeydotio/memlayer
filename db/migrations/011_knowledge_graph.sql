-- Knowledge graph: entities, aliases, mentions, relationships, extraction log

-- Entities: canonical concepts extracted from conversations
CREATE TABLE IF NOT EXISTS entities (
    id BIGSERIAL PRIMARY KEY,
    canonical_name VARCHAR(512) NOT NULL,
    entity_type VARCHAR(64) NOT NULL,       -- concept, decision, bug, pattern, tool, library, architecture, file, person, project
    description TEXT,
    project_path VARCHAR(512),              -- NULL = cross-project entity
    status VARCHAR(32) NOT NULL DEFAULT 'active',  -- active, superseded, resolved, archived
    confidence FLOAT NOT NULL DEFAULT 1.0,
    mention_count INT NOT NULL DEFAULT 1,
    first_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    embedding vector(1536),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Trigram index for fuzzy name matching during entity resolution
CREATE INDEX IF NOT EXISTS idx_entities_canonical_name_trgm
    ON entities USING GIN (canonical_name gin_trgm_ops);

CREATE INDEX IF NOT EXISTS idx_entities_type ON entities(entity_type);
CREATE INDEX IF NOT EXISTS idx_entities_project_path ON entities(project_path);
CREATE INDEX IF NOT EXISTS idx_entities_status ON entities(status) WHERE status = 'active';

-- Vector index for embedding-based entity resolution
CREATE INDEX IF NOT EXISTS idx_entities_embedding ON entities
    USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 128);

-- Entity aliases: alternative names that map to a canonical entity
CREATE TABLE IF NOT EXISTS entity_aliases (
    id BIGSERIAL PRIMARY KEY,
    entity_id BIGINT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    alias VARCHAR(512) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entity_aliases_entity_id ON entity_aliases(entity_id);
CREATE INDEX IF NOT EXISTS idx_entity_aliases_alias_trgm
    ON entity_aliases USING GIN (alias gin_trgm_ops);

-- Entity mentions: links entities back to the memory entries they were extracted from
CREATE TABLE IF NOT EXISTS entity_mentions (
    id BIGSERIAL PRIMARY KEY,
    entity_id BIGINT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    entry_id BIGINT NOT NULL REFERENCES memory_entries(id) ON DELETE CASCADE,
    session_id VARCHAR(64) NOT NULL REFERENCES claude_sessions(session_id),
    mention_text TEXT,
    context_snippet TEXT,
    confidence FLOAT NOT NULL DEFAULT 1.0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_entity_mentions_entity_id ON entity_mentions(entity_id);
CREATE INDEX IF NOT EXISTS idx_entity_mentions_entry_id ON entity_mentions(entry_id);
CREATE INDEX IF NOT EXISTS idx_entity_mentions_session_id ON entity_mentions(session_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_entity_mentions_unique ON entity_mentions(entity_id, entry_id);

-- Entity relationships: typed, directional, temporally-valid connections
CREATE TABLE IF NOT EXISTS entity_relationships (
    id BIGSERIAL PRIMARY KEY,
    source_entity_id BIGINT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    target_entity_id BIGINT NOT NULL REFERENCES entities(id) ON DELETE CASCADE,
    relationship_type VARCHAR(64) NOT NULL,  -- supports, contradicts, supersedes, depends_on, refines, implements, related_to, part_of, caused_by, resolved_by
    description TEXT,
    confidence FLOAT NOT NULL DEFAULT 1.0,
    source_entry_id BIGINT REFERENCES memory_entries(id),
    valid_from TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    valid_until TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT no_self_loop CHECK (source_entity_id != target_entity_id)
);

CREATE INDEX IF NOT EXISTS idx_relationships_source ON entity_relationships(source_entity_id);
CREATE INDEX IF NOT EXISTS idx_relationships_target ON entity_relationships(target_entity_id);
CREATE INDEX IF NOT EXISTS idx_relationships_type ON entity_relationships(relationship_type);
CREATE INDEX IF NOT EXISTS idx_relationships_valid ON entity_relationships(valid_until) WHERE valid_until IS NULL;

-- Extraction log: tracks which entries have been processed
CREATE TABLE IF NOT EXISTS extraction_log (
    id BIGSERIAL PRIMARY KEY,
    entry_id BIGINT REFERENCES memory_entries(id),
    session_id VARCHAR(64),
    batch_key VARCHAR(256),
    status VARCHAR(32) NOT NULL,            -- pending, processing, completed, failed, skipped
    entities_extracted INT DEFAULT 0,
    relationships_extracted INT DEFAULT 0,
    llm_provider VARCHAR(32),
    llm_model VARCHAR(128),
    tokens_used INT DEFAULT 0,
    error_message TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_extraction_log_entry_id ON extraction_log(entry_id);
CREATE INDEX IF NOT EXISTS idx_extraction_log_status ON extraction_log(status) WHERE status = 'pending';
