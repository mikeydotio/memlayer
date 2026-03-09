CREATE TABLE IF NOT EXISTS applied_migrations (
    filename VARCHAR(256) PRIMARY KEY,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
