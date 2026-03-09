import asyncio
import logging
import os
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request, HTTPException

from .config import settings
from .db import init_pool, close_pool
from .embeddings import init_embedder, embedding_worker
from .eviction import eviction_worker
from .routes.ingest import router as ingest_router
from .routes.search import router as search_router
from .routes.files import router as files_router

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(name)s %(levelname)s %(message)s")
logger = logging.getLogger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI):
    await init_pool()

    # Run new migration for response_files table
    from .db import get_pool
    pool = get_pool()
    migration_path = "/app/migrations/005_response_files.sql"
    if os.path.exists(migration_path):
        with open(migration_path) as f:
            await pool.execute(f.read())
        logger.info("Applied migration 005_response_files.sql")

    os.makedirs(settings.file_storage_path, exist_ok=True)

    init_embedder()
    embed_task = asyncio.create_task(embedding_worker())
    evict_task = asyncio.create_task(eviction_worker())
    logger.info("Memlayer server started")
    yield
    embed_task.cancel()
    evict_task.cancel()
    await close_pool()
    logger.info("Memlayer server stopped")


app = FastAPI(title="claude-mem-server", version="0.3.0", lifespan=lifespan)


# Auth middleware
@app.middleware("http")
async def auth_middleware(request: Request, call_next):
    if request.url.path == "/health":
        return await call_next(request)

    if not request.url.path.startswith("/api"):
        return await call_next(request)

    if settings.memlayer_auth_token:
        auth = request.headers.get("Authorization", "")
        if auth != f"Bearer {settings.memlayer_auth_token}":
            raise HTTPException(401, "Invalid or missing auth token")

    return await call_next(request)


app.include_router(ingest_router, prefix="/api")
app.include_router(search_router, prefix="/api")
app.include_router(files_router, prefix="/api")


@app.get("/health")
async def health():
    return {"status": "ok"}
