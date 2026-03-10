-- Migration state tracking for server-to-server migration
-- Supports the full migration lifecycle: IDLE → INITIATED → KEY_EXCHANGED →
-- REDIRECTING → DRAINING → TRANSFERRING → VERIFYING → COMPLETE

CREATE TABLE IF NOT EXISTS migration_state (
    id SERIAL PRIMARY KEY,
    role VARCHAR(16) NOT NULL CHECK (role IN ('source', 'destination')),
    state VARCHAR(20) NOT NULL DEFAULT 'IDLE' CHECK (state IN (
        'IDLE', 'INITIATED', 'KEY_EXCHANGED', 'REDIRECTING',
        'DRAINING', 'TRANSFERRING', 'VERIFYING', 'COMPLETE', 'FAILED'
    )),
    migration_id UUID NOT NULL DEFAULT gen_random_uuid(),

    -- Key management (source only)
    migration_key_hash VARCHAR(64),  -- SHA-256 of base64-encoded key
    migration_key_expires_at TIMESTAMPTZ,
    ed25519_private_key BYTEA,       -- 32-byte seed
    ed25519_public_key BYTEA,        -- 32-byte public key

    -- Peer info
    peer_url TEXT,                     -- URL of the other server

    -- Embedding negotiation
    embedding_provider VARCHAR(50),
    embedding_model VARCHAR(100),
    embedding_dimensions INTEGER,
    embeddings_compatible BOOLEAN DEFAULT FALSE,

    -- Transfer progress
    total_entries INTEGER DEFAULT 0,
    transferred_entries INTEGER DEFAULT 0,
    last_transferred_entry_id BIGINT DEFAULT 0,
    total_files INTEGER DEFAULT 0,
    transferred_files INTEGER DEFAULT 0,
    last_transferred_file_id UUID,
    total_bytes BIGINT DEFAULT 0,
    transferred_bytes BIGINT DEFAULT 0,

    -- Error tracking
    error_message TEXT,
    error_at TIMESTAMPTZ,

    -- Timestamps
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at TIMESTAMPTZ
);

-- Only one active migration at a time
CREATE UNIQUE INDEX IF NOT EXISTS idx_migration_state_active
    ON migration_state (role)
    WHERE state NOT IN ('IDLE', 'COMPLETE', 'FAILED');

-- Server identity for migration handshake
CREATE TABLE IF NOT EXISTS server_identity (
    id SERIAL PRIMARY KEY,
    server_id UUID NOT NULL DEFAULT gen_random_uuid(),
    server_name VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed server identity if not exists
INSERT INTO server_identity (server_name)
    SELECT 'memlayer-server'
    WHERE NOT EXISTS (SELECT 1 FROM server_identity);
