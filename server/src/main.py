import asyncio
import hmac
import json as json_mod
import logging
import os
import time
from contextlib import asynccontextmanager

from pathlib import Path

from fastapi import FastAPI, Request, HTTPException
from fastapi.responses import JSONResponse
from fastapi.staticfiles import StaticFiles
from slowapi.errors import RateLimitExceeded

from .config import settings
from .rate_limit import limiter
from .db import init_pool, close_pool

try:
    from ._version import __version__ as SERVER_VERSION
except ImportError:
    SERVER_VERSION = "0.0.0-dev"
from .embeddings import init_embedder, embedding_worker
from .eviction import eviction_worker
from .extraction import init_extractor, extraction_worker
from .retention import retention_worker
from .routes.ingest import router as ingest_router
from .routes.search import router as search_router
from .routes.files import router as files_router
from .routes.embeddings import router as embeddings_router
from .routes.migration import router as migration_router
from .routes.stream import router as stream_router
from .routes.browse import router as browse_router
from .routes.stats import router as stats_router
from .routes.graph import router as graph_router
from .routes.version import router as version_router


import re

_SECRET_PATTERNS = re.compile(
    r"(postgres(?:ql)?://\S+|"
    r"Bearer\s+\S+|"
    r"sk-[A-Za-z0-9]{20,}|"
    r"(?:OPENAI_API_KEY|ANTHROPIC_API_KEY|MEMLAYER_AUTH_TOKEN|POSTGRES_PASSWORD)=[^\s]+)",
    re.IGNORECASE,
)


def _redact(text: str) -> str:
    return _SECRET_PATTERNS.sub("[REDACTED]", text)


class JsonFormatter(logging.Formatter):
    def format(self, record):
        log_data = {
            "timestamp": self.formatTime(record),
            "level": record.levelname,
            "logger": record.name,
            "message": _redact(record.getMessage()),
        }
        if record.exc_info and record.exc_info[0]:
            log_data["exception"] = _redact(self.formatException(record.exc_info))
        return json_mod.dumps(log_data)


class RedactingFilter(logging.Filter):
    """Redact secrets from all log records (text format)."""
    def filter(self, record):
        record.msg = _redact(str(record.msg))
        if record.exc_info and record.exc_info[1]:
            record.exc_text = _redact(str(record.exc_info[1]))
        return True


log_format = os.environ.get("LOG_FORMAT", "text")
log_level = os.environ.get("LOG_LEVEL", "INFO").upper()
if log_format == "json":
    handler = logging.StreamHandler()
    handler.setFormatter(JsonFormatter())
    logging.root.handlers = [handler]
    logging.root.setLevel(getattr(logging, log_level, logging.INFO))
else:
    logging.basicConfig(level=getattr(logging, log_level, logging.INFO), format="%(asctime)s %(name)s %(levelname)s %(message)s")
    for h in logging.root.handlers:
        h.addFilter(RedactingFilter())

logger = logging.getLogger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI):
    await init_pool()

    # Run migrations with full safety guarantees
    from .db import get_pool
    from .migrator import Migrator, MigrationError
    pool = get_pool()

    migration_dir = "/app/migrations"
    dry_run = os.environ.get("MEMLAYER_MIGRATION_DRY_RUN", "").lower() in ("1", "true", "yes")

    # Detect Supabase: if vector extension is in a non-public schema, likely Supabase
    is_supabase = False
    try:
        vector_schema = await pool.fetchval("""
            SELECT nspname FROM pg_extension e
            JOIN pg_namespace n ON n.oid = e.extnamespace
            WHERE e.extname = 'vector'
        """)
        if vector_schema and vector_schema not in ("public", "pg_catalog"):
            is_supabase = True
    except Exception:
        pass

    migrator = Migrator(
        pool=pool,
        migration_dir=migration_dir,
        backup_dir=os.environ.get("MEMLAYER_BACKUP_DIR", "/data/backups"),
        is_supabase=is_supabase,
        database_url=settings.database_url,
    )

    result = await migrator.run_pending(dry_run=dry_run)

    if result.read_only:
        app.state.read_only = True
        logger.warning(
            f"Server is in READ-ONLY mode (DB schema v{result.schema_version} "
            f"ahead of server's v{migrator.expected_schema_version()})"
        )
    else:
        app.state.read_only = False

    if result.failed:
        raise RuntimeError(
            f"Migration failed: {result.failed} — {result.error}. "
            "Server cannot start with an inconsistent schema. "
            "The failed migration was rolled back; the database is intact."
        )

    if dry_run:
        logger.info(
            f"Dry-run complete: validated {len(result.applied)} migration(s). "
            "No changes were applied."
        )
    elif result.applied:
        logger.info(
            f"Applied {len(result.applied)} migration(s): {', '.join(result.applied)}. "
            f"Schema version: {result.schema_version}"
        )
        if result.backup_path:
            logger.info(f"Pre-migration backup at: {result.backup_path}")
    if result.fingerprint_changed:
        logger.warning("Schema fingerprint changed — possible manual schema alteration detected")

    app.state.schema_version = result.schema_version
    app.state.min_client_version = settings.min_client_version or None

    os.makedirs(settings.file_storage_path, exist_ok=True)

    init_embedder()
    init_extractor()
    embed_task = asyncio.create_task(embedding_worker())
    evict_task = asyncio.create_task(eviction_worker())
    extract_task = asyncio.create_task(extraction_worker())
    retain_task = asyncio.create_task(retention_worker())
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
    extract_task.cancel()
    retain_task.cancel()
    try:
        await asyncio.gather(embed_task, evict_task, extract_task, retain_task, return_exceptions=True)
    except Exception:
        pass
    await close_pool()

    elapsed = time.monotonic() - shutdown_start
    logger.info(f"Memlayer server stopped (shutdown took {elapsed:.1f}s)")


app = FastAPI(title="memlayer-server", version=SERVER_VERSION, lifespan=lifespan)

# Rate limiting
app.state.limiter = limiter


@app.exception_handler(RateLimitExceeded)
async def rate_limit_handler(request: Request, exc: RateLimitExceeded):
    return JSONResponse(
        status_code=429,
        content={"detail": "Rate limit exceeded"},
    )


MAX_REQUEST_BODY_BYTES = 10 * 1024 * 1024  # 10 MB


@app.middleware("http")
async def security_middleware(request: Request, call_next):
    # Reject oversized request bodies
    content_length = request.headers.get("content-length")
    if content_length and int(content_length) > MAX_REQUEST_BODY_BYTES:
        return JSONResponse(
            status_code=413,
            content={"detail": f"Request body too large (max {MAX_REQUEST_BODY_BYTES} bytes)"},
        )

    response = await call_next(request)

    # Security headers
    response.headers["X-Content-Type-Options"] = "nosniff"
    response.headers["X-Frame-Options"] = "DENY"
    response.headers["Content-Security-Policy"] = "default-src 'none'"
    response.headers["Referrer-Policy"] = "no-referrer"

    return response


# Auth middleware
@app.middleware("http")
async def auth_middleware(request: Request, call_next):
    # No auth required for health, version, or static files
    if request.url.path in ("/health", "/api/version"):
        return await call_next(request)

    if not request.url.path.startswith("/api"):
        return await call_next(request)

    if settings.memlayer_auth_token:
        auth = request.headers.get("Authorization", "")
        expected = f"Bearer {settings.memlayer_auth_token}"
        if hmac.compare_digest(auth, expected):
            pass  # Admin auth — allow
        elif auth.startswith("Bearer migration:"):
            # Migration key passthrough for endpoints that self-validate
            path = request.url.path
            migration_key_allowed = (
                path in {
                    "/api/migration/status",
                    "/api/migration/verify-destination",
                    "/api/migration/client-provision",
                    "/api/ingest",
                }
                or path.startswith("/api/migration/stream/")
            )
            if not migration_key_allowed:
                return JSONResponse(status_code=401, content={"detail": "Invalid or missing auth token"})
        else:
            return JSONResponse(status_code=401, content={"detail": "Invalid or missing auth token"})

    # Version compatibility negotiation
    from .version import (
        SERVER_VERSION as SV, check_compatibility, CompatResult,
        features_for_version,
    )
    from .notifications import tracker

    client_version = request.headers.get("X-Memlayer-Version", "")
    client_component = request.headers.get("X-Memlayer-Component", "unknown")
    is_read_only = getattr(request.app.state, "read_only", False)
    schema_version = getattr(request.app.state, "schema_version", 0)
    min_client_ver = getattr(request.app.state, "min_client_version", None)
    strict_check = os.environ.get("MEMLAYER_STRICT_VERSION_CHECK", "").lower() in ("1", "true", "yes")

    if client_version:
        compat = check_compatibility(client_version, min_client_version=min_client_ver)

        if compat == CompatResult.UPGRADE_REQUIRED:
            logger.warning(
                f"Client {client_component} v{client_version} below minimum required "
                f"v{min_client_ver}"
            )
            tracker.record(client_version, client_component)
            return JSONResponse(
                status_code=426,
                content={
                    "error": "upgrade_required",
                    "detail": f"Critical update required: minimum version {min_client_ver}",
                    "server_version": SV,
                    "min_client_version": min_client_ver,
                    "update_url": "https://github.com/mikeydotio/memlayer/releases/latest",
                },
            )

        if compat == CompatResult.MAJOR_MISMATCH:
            logger.warning(
                f"Major version mismatch: client {client_component} "
                f"v{client_version}, server v{SV}"
            )
            tracker.record(client_version, client_component)
            if strict_check:
                from .version import parse_version
                return JSONResponse(
                    status_code=400,
                    content={
                        "error": "version_incompatible",
                        "detail": f"Major version mismatch: client={client_version}, server={SV}",
                        "server_version": SV,
                        "required_major": parse_version(SV)[0],
                    },
                )

        if compat == CompatResult.MINOR_MISMATCH:
            logger.info(
                f"Minor version mismatch: client {client_component} "
                f"v{client_version}, server v{SV}"
            )

    # Read-only mode enforcement: reject mutating requests
    if is_read_only and request.method in ("POST", "PUT", "PATCH", "DELETE"):
        from .migrator import Migrator
        expected = len([f for f in os.listdir("/app/migrations") if f.endswith(".sql")]) if os.path.isdir("/app/migrations") else 0
        return JSONResponse(
            status_code=503,
            content={
                "error": "read_only_mode",
                "detail": "Server is in read-only mode (database schema ahead of server)",
                "schema_version": schema_version,
                "server_expected_schema": expected,
            },
        )

    response = await call_next(request)

    # Add version headers (suppressible via EXPOSE_VERSION_HEADERS=false)
    if settings.expose_version_headers:
        response.headers["X-Memlayer-Server-Version"] = SV
        response.headers["X-Memlayer-Schema-Version"] = str(schema_version)
    response.headers["X-Memlayer-Read-Only"] = str(is_read_only).lower()
    if min_client_ver:
        response.headers["X-Memlayer-Min-Client-Version"] = min_client_ver
    if client_version:
        response.headers["X-Memlayer-Features"] = ",".join(
            features_for_version(client_version)
        )
    if not client_version:
        response.headers["X-Memlayer-Upgrade-Required"] = "true"

    return response


@app.middleware("http")
async def migration_pubkey_middleware(request: Request, call_next):
    response = await call_next(request)
    if request.url.path == "/api/ingest":
        try:
            from .migration_state import get_migration_manager, MigrationRole
            mgr = get_migration_manager()
            state = await mgr.get_active_state(MigrationRole.SOURCE)
            if state and state.get("ed25519_public_key"):
                import base64
                response.headers["X-Memlayer-Migration-Pubkey"] = (
                    base64.urlsafe_b64encode(state["ed25519_public_key"]).decode()
                )
        except Exception:
            pass  # Never break ingest for migration bookkeeping
    return response


app.include_router(ingest_router, prefix="/api")
app.include_router(search_router, prefix="/api")
app.include_router(files_router, prefix="/api")
app.include_router(embeddings_router, prefix="/api")
app.include_router(migration_router, prefix="/api")
app.include_router(stream_router, prefix="/api")
app.include_router(browse_router, prefix="/api")
app.include_router(stats_router, prefix="/api")
app.include_router(graph_router, prefix="/api")
app.include_router(version_router, prefix="/api")


@app.get("/api/admin/incompatible-clients")
async def incompatible_clients():
    """Admin endpoint: list incompatible client connection records."""
    from .notifications import tracker
    return {"records": tracker.get_records()}


@app.get("/health")
async def health(request: Request):
    status = {
        "status": "ok",
        "version": SERVER_VERSION,
        "schema_version": getattr(request.app.state, "schema_version", 0),
        "read_only": getattr(request.app.state, "read_only", False),
        "components": {},
    }

    # Check database
    try:
        from .db import get_pool
        pool = get_pool()
        await pool.fetchval("SELECT 1")
        status["components"]["database"] = "ok"
    except Exception as e:
        if settings.memlayer_env == "production":
            status["components"]["database"] = "error"
        else:
            status["components"]["database"] = f"error: {e}"
        status["status"] = "degraded"

    # Check embeddings
    from .embeddings import _embedder, get_embedding_status
    if _embedder:
        status["components"]["embeddings"] = f"ok ({settings.embedding_provider})"
        try:
            status["embedding_progress"] = await get_embedding_status()
        except Exception:
            status["embedding_progress"] = {"error": "failed to fetch stats"}
    else:
        status["components"]["embeddings"] = "disabled (FTS-only)"

    # Response analytics
    from .analytics import response_analytics
    analytics = response_analytics.get_stats()
    if analytics:
        status["response_analytics"] = analytics

    return status


# Mount web UI (static files) — must be after all API routes
_static_dir = Path(__file__).parent.parent / "static"
if _static_dir.is_dir():
    app.mount("/", StaticFiles(directory=str(_static_dir), html=True), name="ui")
