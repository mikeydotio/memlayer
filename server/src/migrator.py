"""
Safe database migration system for memlayer.

Provides:
- Transaction-wrapped migrations (atomic rollback on failure)
- Pre-flight schema validation before each migration
- Automatic pg_dump backup before applying pending migrations
- Schema version tracking via schema_meta table
- Schema fingerprint for tamper detection
- Dry-run mode (validate without applying)
- Read-only detection (DB ahead of server = read-only mode)
- Supabase awareness (skip CREATE EXTENSION, handle permissions)
"""

import asyncio
import hashlib
import logging
import os
import re
from dataclasses import dataclass, field
from datetime import datetime

import asyncpg

logger = logging.getLogger(__name__)


@dataclass
class MigrationResult:
    applied: list[str] = field(default_factory=list)
    skipped: list[str] = field(default_factory=list)
    failed: str | None = None
    error: str | None = None
    backup_path: str | None = None
    dry_run: bool = False
    schema_version: int = 0
    read_only: bool = False
    fingerprint_changed: bool = False


class MigrationError(Exception):
    pass


class Migrator:
    def __init__(
        self,
        pool: asyncpg.Pool,
        migration_dir: str,
        backup_dir: str = "/data/backups",
        is_supabase: bool = False,
        database_url: str = "",
    ):
        self.pool = pool
        self.migration_dir = migration_dir
        self.backup_dir = backup_dir
        self.is_supabase = is_supabase
        self.database_url = database_url

    def _migration_files(self) -> list[str]:
        """Return sorted list of .sql migration filenames."""
        if not os.path.isdir(self.migration_dir):
            return []
        return sorted(f for f in os.listdir(self.migration_dir) if f.endswith(".sql"))

    def expected_schema_version(self) -> int:
        """Number of migration files = expected schema version."""
        return len(self._migration_files())

    async def _ensure_schema_meta(self):
        """Create schema_meta table if it doesn't exist (bootstrap for pre-013 DBs)."""
        await self.pool.execute("""
            CREATE TABLE IF NOT EXISTS schema_meta (
                key   VARCHAR(64) PRIMARY KEY,
                value TEXT         NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        """)

    async def _ensure_tracking_table(self):
        """Create applied_migrations table if needed."""
        await self.pool.execute("""
            CREATE TABLE IF NOT EXISTS applied_migrations (
                filename   VARCHAR(256) PRIMARY KEY,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        """)

    async def get_schema_version(self) -> int:
        """Read schema_version from schema_meta. Returns 0 if table/key doesn't exist."""
        try:
            row = await self.pool.fetchval(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'"
            )
            return int(row) if row else 0
        except asyncpg.UndefinedTableError:
            return 0

    async def detect_read_only(self) -> bool:
        """True if DB schema version is ahead of what this server knows about."""
        db_version = await self.get_schema_version()
        expected = self.expected_schema_version()
        if db_version > expected:
            logger.warning(
                f"Database schema (v{db_version}) is ahead of server "
                f"(expects v{expected}). Starting in READ-ONLY mode."
            )
            return True
        return False

    async def _seed_tracking(self):
        """Seed applied_migrations for databases initialized by Docker entrypoint.

        If the tracking table is empty but core tables exist, mark pre-existing
        migrations as applied based on their created objects.
        """
        tracked_count = await self.pool.fetchval(
            "SELECT COUNT(*) FROM applied_migrations"
        )
        if tracked_count > 0:
            return

        tables_exist = await self.pool.fetchval("""
            SELECT EXISTS (
                SELECT 1 FROM information_schema.tables
                WHERE table_name = 'memory_entries'
            )
        """)
        if not tables_exist:
            return

        seed_checks = {
            "001_extensions.sql": "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')",
            "002_tables.sql": "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'memory_entries')",
            "003_indexes.sql": "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_entries_fts')",
            "004_functions.sql": "SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'hybrid_search')",
            "005_response_files.sql": "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'response_files')",
        }

        seeded = 0
        for filename in self._migration_files():
            if filename in seed_checks:
                exists = await self.pool.fetchval(seed_checks[filename])
                if exists:
                    await self.pool.execute(
                        "INSERT INTO applied_migrations (filename) VALUES ($1) ON CONFLICT DO NOTHING",
                        filename,
                    )
                    seeded += 1

        if seeded:
            logger.info(f"Seeded migration tracking with {seeded} pre-existing migrations")

    async def _preflight_check(self, filename: str, sql: str) -> list[str]:
        """Validate preconditions for a migration. Returns warnings.

        Raises MigrationError if a hard precondition fails.
        """
        warnings = []

        # Detect ALTER TABLE targets — the table should exist
        for match in re.finditer(r"ALTER\s+TABLE\s+(?:IF\s+EXISTS\s+)?(\w+)", sql, re.IGNORECASE):
            table = match.group(1)
            exists = await self.pool.fetchval(
                "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
                table,
            )
            if not exists:
                raise MigrationError(
                    f"Pre-flight failed for {filename}: ALTER TABLE references "
                    f"'{table}' which does not exist"
                )

        # Detect CREATE TABLE targets — warn if table already exists
        for match in re.finditer(
            r"CREATE\s+TABLE\s+(?!IF\s+NOT\s+EXISTS)(\w+)", sql, re.IGNORECASE
        ):
            table = match.group(1)
            exists = await self.pool.fetchval(
                "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
                table,
            )
            if exists:
                warnings.append(
                    f"{filename}: CREATE TABLE '{table}' but table already exists "
                    "(missing IF NOT EXISTS?)"
                )

        # For Supabase: warn about CREATE EXTENSION (they pre-install extensions)
        if self.is_supabase and re.search(r"CREATE\s+EXTENSION", sql, re.IGNORECASE):
            warnings.append(
                f"{filename}: contains CREATE EXTENSION — Supabase pre-installs extensions, "
                "this may fail or be a no-op"
            )

        return warnings

    async def create_backup(self) -> str | None:
        """Run pg_dump before applying migrations. Returns backup file path."""
        if not self.database_url:
            logger.warning("No database_url configured, skipping pre-migration backup")
            return None

        os.makedirs(self.backup_dir, exist_ok=True)
        timestamp = datetime.utcnow().strftime("%Y%m%d_%H%M%S")
        backup_path = os.path.join(self.backup_dir, f"memlayer_pre_migration_{timestamp}.sql")

        cmd = ["pg_dump", "--no-password", "-f", backup_path, self.database_url]
        if self.is_supabase:
            cmd.insert(1, "--no-owner")
            cmd.insert(1, "--no-privileges")

        try:
            proc = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
            _, stderr = await asyncio.wait_for(proc.communicate(), timeout=300)

            if proc.returncode != 0:
                err_msg = stderr.decode().strip() if stderr else "unknown error"
                logger.error(f"pg_dump failed (rc={proc.returncode}): {err_msg}")
                raise MigrationError(f"Pre-migration backup failed: {err_msg}")

            size = os.path.getsize(backup_path)
            logger.info(f"Pre-migration backup created: {backup_path} ({size} bytes)")
            return backup_path

        except asyncio.TimeoutError:
            logger.error("pg_dump timed out after 300s")
            raise MigrationError("Pre-migration backup timed out")

    async def _apply_one(self, filename: str, sql: str, dry_run: bool = False) -> bool:
        """Apply a single migration in a transaction.

        Returns True on success. Raises MigrationError on failure.
        """
        # Pre-flight
        warnings = await self._preflight_check(filename, sql)
        for w in warnings:
            logger.warning(f"Pre-flight: {w}")

        # Extract schema version from filename (e.g. "013_schema_meta.sql" -> 13)
        match = re.match(r"(\d+)", filename)
        schema_version = int(match.group(1)) if match else 0

        async with self.pool.acquire() as conn:
            tx = conn.transaction()
            await tx.start()
            try:
                await conn.execute(sql)

                # Update schema version if schema_meta exists
                try:
                    await conn.execute(
                        "UPDATE schema_meta SET value = $1, updated_at = NOW() "
                        "WHERE key = 'schema_version'",
                        str(schema_version),
                    )
                except asyncpg.UndefinedTableError:
                    pass  # schema_meta doesn't exist yet (will be created by migration 013)

                await conn.execute(
                    "INSERT INTO applied_migrations (filename) VALUES ($1)",
                    filename,
                )

                if dry_run:
                    await tx.rollback()
                    logger.info(f"Dry-run validated: {filename}")
                else:
                    await tx.commit()
                    logger.info(f"Applied migration: {filename}")

                return True

            except Exception as e:
                await tx.rollback()
                raise MigrationError(
                    f"Migration {filename} failed (transaction rolled back): {e}"
                ) from e

    async def _compute_fingerprint(self) -> str:
        """Compute SHA-256 fingerprint of the memlayer schema (tables, indexes, functions)."""
        parts = []

        # Tables and columns
        rows = await self.pool.fetch("""
            SELECT table_name, column_name, data_type, is_nullable, column_default
            FROM information_schema.columns
            WHERE table_schema = 'public'
              AND table_name IN (
                  'claude_sessions', 'memory_entries', 'response_files',
                  'applied_migrations', 'schema_meta', 'migration_state',
                  'server_identity', 'entities', 'entity_aliases',
                  'entity_mentions', 'entity_relationships', 'extraction_log'
              )
            ORDER BY table_name, column_name
        """)
        for r in rows:
            parts.append(f"col:{r['table_name']}.{r['column_name']}:{r['data_type']}:{r['is_nullable']}:{r['column_default']}")

        # Indexes
        rows = await self.pool.fetch("""
            SELECT indexname, indexdef
            FROM pg_indexes
            WHERE schemaname = 'public'
              AND tablename IN (
                  'claude_sessions', 'memory_entries', 'response_files',
                  'applied_migrations', 'schema_meta', 'migration_state',
                  'server_identity', 'entities', 'entity_aliases',
                  'entity_mentions', 'entity_relationships', 'extraction_log'
              )
            ORDER BY indexname
        """)
        for r in rows:
            parts.append(f"idx:{r['indexname']}:{r['indexdef']}")

        # Functions
        rows = await self.pool.fetch("""
            SELECT proname, pg_get_functiondef(oid) AS funcdef
            FROM pg_proc
            WHERE pronamespace = 'public'::regnamespace
              AND proname IN ('hybrid_search', 'get_session_entries', 'graph_expanded_search')
            ORDER BY proname
        """)
        for r in rows:
            parts.append(f"func:{r['proname']}:{r['funcdef']}")

        fingerprint = hashlib.sha256("\n".join(parts).encode()).hexdigest()
        return fingerprint

    async def _update_fingerprint(self):
        """Compute and store the schema fingerprint."""
        fingerprint = await self._compute_fingerprint()

        # Check for tamper
        stored = await self.pool.fetchval(
            "SELECT value FROM schema_meta WHERE key = 'schema_fingerprint'"
        )
        strict_fingerprint = os.environ.get(
            "MEMLAYER_STRICT_FINGERPRINT", ""
        ).lower() in ("1", "true", "yes")

        if stored and stored not in ("pending", fingerprint):
            msg = (
                "Schema fingerprint mismatch — the database schema may have been "
                "manually altered since the last migration. "
                f"Expected: {stored[:16]}..., Got: {fingerprint[:16]}..."
            )
            if strict_fingerprint:
                raise MigrationError(
                    f"STRICT FINGERPRINT: {msg}. "
                    "Disable MEMLAYER_STRICT_FINGERPRINT to allow startup."
                )
            logger.warning(msg)

        await self.pool.execute(
            "INSERT INTO schema_meta (key, value, updated_at) VALUES ('schema_fingerprint', $1, NOW()) "
            "ON CONFLICT (key) DO UPDATE SET value = $1, updated_at = NOW()",
            fingerprint,
        )
        return stored not in ("pending", fingerprint) if stored else False

    async def run_pending(
        self,
        dry_run: bool = False,
        backup: bool = True,
    ) -> MigrationResult:
        """Run all pending migrations with full safety guarantees.

        1. Ensure tracking tables exist
        2. Seed pre-existing migrations (Docker entrypoint compat)
        3. Check for read-only condition (DB ahead of server)
        4. Create pg_dump backup (if not dry-run)
        5. Apply each pending migration in a transaction
        6. Update schema fingerprint
        """
        result = MigrationResult(dry_run=dry_run)

        # Bootstrap tables
        await self._ensure_tracking_table()
        await self._ensure_schema_meta()
        await self._seed_tracking()

        # Read-only detection
        if await self.detect_read_only():
            result.read_only = True
            result.schema_version = await self.get_schema_version()
            return result

        # Find pending migrations
        migration_files = self._migration_files()
        pending = []
        for filename in migration_files:
            already = await self.pool.fetchval(
                "SELECT 1 FROM applied_migrations WHERE filename = $1", filename
            )
            if not already:
                pending.append(filename)

        if not pending:
            result.schema_version = await self.get_schema_version()
            logger.info(f"No pending migrations (schema version: {result.schema_version})")
            # Still update fingerprint to detect tampering
            try:
                result.fingerprint_changed = await self._update_fingerprint()
            except Exception as e:
                logger.warning(f"Failed to update schema fingerprint: {e}")
            return result

        logger.info(f"Found {len(pending)} pending migration(s): {', '.join(pending)}")

        # Backup before applying (not for dry-run)
        if backup and not dry_run:
            try:
                result.backup_path = await self.create_backup()
            except MigrationError as e:
                logger.error(f"Backup failed, aborting migrations: {e}")
                result.error = str(e)
                return result

        # Apply each migration
        for filename in pending:
            filepath = os.path.join(self.migration_dir, filename)
            with open(filepath) as f:
                sql = f.read()

            try:
                await self._apply_one(filename, sql, dry_run=dry_run)
                result.applied.append(filename)
            except MigrationError as e:
                result.failed = filename
                result.error = str(e)
                logger.error(str(e))
                break

        result.schema_version = await self.get_schema_version()

        # Update fingerprint (only if not dry-run and we applied something)
        if not dry_run and result.applied and not result.failed:
            try:
                result.fingerprint_changed = await self._update_fingerprint()
            except Exception as e:
                logger.warning(f"Failed to update schema fingerprint: {e}")

        return result
