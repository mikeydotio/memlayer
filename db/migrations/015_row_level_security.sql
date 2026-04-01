-- Enable Row-Level Security on all tables.
-- Table owners (memlayer in Docker Compose, postgres in Supabase) bypass
-- RLS by default. On Supabase, explicit service_role policies are created
-- so the Supabase dashboard shows clean policy state.

ALTER TABLE claude_sessions ENABLE ROW LEVEL SECURITY;
ALTER TABLE memory_entries ENABLE ROW LEVEL SECURITY;
ALTER TABLE response_files ENABLE ROW LEVEL SECURITY;
ALTER TABLE entities ENABLE ROW LEVEL SECURITY;
ALTER TABLE entity_aliases ENABLE ROW LEVEL SECURITY;
ALTER TABLE entity_mentions ENABLE ROW LEVEL SECURITY;
ALTER TABLE entity_relationships ENABLE ROW LEVEL SECURITY;
ALTER TABLE extraction_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE migration_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE server_identity ENABLE ROW LEVEL SECURITY;
ALTER TABLE applied_migrations ENABLE ROW LEVEL SECURITY;
ALTER TABLE schema_meta ENABLE ROW LEVEL SECURITY;

-- On Supabase: grant service_role full access via explicit policies.
-- On self-hosted (Docker Compose): service_role doesn't exist, skip.
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'service_role') THEN
    DROP POLICY IF EXISTS "service_full_access" ON claude_sessions;
    CREATE POLICY "service_full_access" ON claude_sessions FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON memory_entries;
    CREATE POLICY "service_full_access" ON memory_entries FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON response_files;
    CREATE POLICY "service_full_access" ON response_files FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON entities;
    CREATE POLICY "service_full_access" ON entities FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON entity_aliases;
    CREATE POLICY "service_full_access" ON entity_aliases FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON entity_mentions;
    CREATE POLICY "service_full_access" ON entity_mentions FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON entity_relationships;
    CREATE POLICY "service_full_access" ON entity_relationships FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON extraction_log;
    CREATE POLICY "service_full_access" ON extraction_log FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON migration_state;
    CREATE POLICY "service_full_access" ON migration_state FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON server_identity;
    CREATE POLICY "service_full_access" ON server_identity FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON applied_migrations;
    CREATE POLICY "service_full_access" ON applied_migrations FOR ALL TO service_role USING (true) WITH CHECK (true);

    DROP POLICY IF EXISTS "service_full_access" ON schema_meta;
    CREATE POLICY "service_full_access" ON schema_meta FOR ALL TO service_role USING (true) WITH CHECK (true);
  END IF;
END
$$;

-- Bump schema version
INSERT INTO schema_meta (key, value, updated_at)
VALUES ('schema_version', '14', NOW())
ON CONFLICT (key) DO UPDATE SET value = '14', updated_at = NOW();
