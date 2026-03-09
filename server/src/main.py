import asyncio
import hmac
import json as json_mod
import logging
import os
import time
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request, HTTPException

from .config import settings
from .db import init_pool, close_pool
from .embeddings import init_embedder, embedding_worker
from .eviction import eviction_worker
from .routes.ingest import router as ingest_router
from .routes.search import router as search_router
from .routes.files import router as files_router


class JsonFormatter(logging.Formatter):
    def format(self, record):
        log_data = {
            "timestamp": self.formatTime(record),
            "level": record.levelname,
            "logger": record.name,
            "message": record.getMessage(),
        }
        if record.exc_info and record.exc_info[0]:
            log_data["exception"] = self.formatException(record.exc_info)
        return json_mod.dumps(log_data)


log_format = os.environ.get("LOG_FORMAT", "text")
log_level = os.environ.get("LOG_LEVEL", "INFO").upper()
if log_format == "json":
    handler = logging.StreamHandler()
    handler.setFormatter(JsonFormatter())
    logging.root.handlers = [handler]
    logging.root.setLevel(getattr(logging, log_level, logging.INFO))
else:
    logging.basicConfig(level=getattr(logging, log_level, logging.INFO), format="%(asctime)s %(name)s %(levelname)s %(message)s")

logger = logging.getLogger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI):
    await init_pool()

    # Run migrations
    from .db import get_pool
    pool = get_pool()

    # Ensure migration tracking table exists
    await pool.execute("""
        CREATE TABLE IF NOT EXISTS applied_migrations (
            filename VARCHAR(256) PRIMARY KEY,
            applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        )
    """)

    # Apply all pending migrations
    migration_dir = "/app/migrations"
    if os.path.isdir(migration_dir):
        migration_files = sorted(
            f for f in os.listdir(migration_dir) if f.endswith(".sql")
        )

        # Seed tracking table: if it's empty but DB has tables from Docker init,
        # record migrations that were already applied by Docker entrypoint (001-005)
        tracked_count = await pool.fetchval(
            "SELECT COUNT(*) FROM applied_migrations"
        )
        if tracked_count == 0:
            # Check if DB was already initialized (memory_entries exists = migrations ran)
            tables_exist = await pool.fetchval("""
                SELECT EXISTS (
                    SELECT 1 FROM information_schema.tables
                    WHERE table_name = 'memory_entries'
                )
            """)
            if tables_exist:
                # Only seed migrations that Docker init would have run (those that
                # existed when the DB was first created). We detect this by checking
                # if the objects they create already exist.
                seed_checks = {
                    "001_extensions.sql": "SELECT EXISTS (SELECT 1 FROM pg_extension WHERE extname = 'vector')",
                    "002_tables.sql": "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'memory_entries')",
                    "003_indexes.sql": "SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_entries_fts')",
                    "004_functions.sql": "SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'hybrid_search')",
                    "005_response_files.sql": "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'response_files')",
                }
                seeded = 0
                for filename in migration_files:
                    if filename in seed_checks:
                        exists = await pool.fetchval(seed_checks[filename])
                        if exists:
                            await pool.execute(
                                "INSERT INTO applied_migrations (filename) VALUES ($1) ON CONFLICT DO NOTHING",
                                filename,
                            )
                            seeded += 1
                if seeded:
                    logger.info(f"Seeded migration tracking with {seeded} pre-existing migrations")

        for filename in migration_files:
            already = await pool.fetchval(
                "SELECT 1 FROM applied_migrations WHERE filename = $1", filename
            )
            if already:
                continue
            filepath = os.path.join(migration_dir, filename)
            with open(filepath) as f:
                sql = f.read()
            await pool.execute(sql)
            await pool.execute(
                "INSERT INTO applied_migrations (filename) VALUES ($1)", filename
            )
            logger.info(f"Applied migration {filename}")

    os.makedirs(settings.file_storage_path, exist_ok=True)

    init_embedder()
    embed_task = asyncio.create_task(embedding_worker())
    evict_task = asyncio.create_task(eviction_worker())
    logger.info("Memlayer server started")
    yield

    shutdown_start = time.monotonic()
    logger.info("Shutting down...")

    # Give workers a moment to finish current operations
    from .embeddings import _queue
    if _queue:
        logger.info(f"Embedding queue has {len(_queue)} items, waiting up to 10s...")
        await asyncio.sleep(min(10, len(_queue) * 0.5))  # Rough estimate

    embed_task.cancel()
    evict_task.cancel()
    try:
        await asyncio.gather(embed_task, evict_task, return_exceptions=True)
    except Exception:
        pass
    await close_pool()

    elapsed = time.monotonic() - shutdown_start
    logger.info(f"Memlayer server stopped (shutdown took {elapsed:.1f}s)")


app = FastAPI(title="claude-mem-server", version="1.0.0", lifespan=lifespan)


# Auth middleware
@app.middleware("http")
async def auth_middleware(request: Request, call_next):
    if request.url.path == "/health":
        return await call_next(request)

    if not request.url.path.startswith("/api"):
        return await call_next(request)

    if settings.memlayer_auth_token:
        auth = request.headers.get("Authorization", "")
        expected = f"Bearer {settings.memlayer_auth_token}"
        if not hmac.compare_digest(auth, expected):
            raise HTTPException(401, "Invalid or missing auth token")

    # Version compatibility check
    client_version = request.headers.get("X-Memlayer-Version", "")
    if client_version:
        # Compare major.minor — warn if different
        server_parts = "1.0.0".split(".")[:2]
        client_parts = client_version.split(".")[:2]
        if server_parts != client_parts:
            logger.warning(
                f"Version mismatch: server=1.0.0, client={client_version}. "
                "Some features may not work correctly."
            )

    return await call_next(request)


app.include_router(ingest_router, prefix="/api")
app.include_router(search_router, prefix="/api")
app.include_router(files_router, prefix="/api")


@app.get("/health")
async def health():
    status = {"status": "ok", "components": {}}

    # Check database
    try:
        from .db import get_pool
        pool = get_pool()
        await pool.fetchval("SELECT 1")
        status["components"]["database"] = "ok"
    except Exception as e:
        status["components"]["database"] = f"error: {e}"
        status["status"] = "degraded"

    # Check embeddings
    from .embeddings import _embedder
    if _embedder:
        status["components"]["embeddings"] = f"ok ({settings.embedding_provider})"
    else:
        status["components"]["embeddings"] = "disabled (FTS-only)"

    # Response analytics
    from .analytics import response_analytics
    analytics = response_analytics.get_stats()
    if analytics:
        status["response_analytics"] = analytics

    return status
