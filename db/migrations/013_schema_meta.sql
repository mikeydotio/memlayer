-- Schema metadata table for version tracking and tamper detection.
-- Stores schema_version (incremented per migration) and schema_fingerprint
-- (SHA-256 of DDL, computed by the server at startup).

CREATE TABLE IF NOT EXISTS schema_meta (
    key   VARCHAR(64)  PRIMARY KEY,
    value TEXT         NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Seed: this is migration 13, so schema_version = 13
INSERT INTO schema_meta (key, value)
VALUES ('schema_version', '13')
ON CONFLICT (key) DO UPDATE SET value = '13', updated_at = NOW();

-- Fingerprint placeholder — server computes and stores the real value on startup
INSERT INTO schema_meta (key, value)
VALUES ('schema_fingerprint', 'pending')
ON CONFLICT (key) DO NOTHING;
