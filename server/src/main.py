import asyncio
import logging
from contextlib import asynccontextmanager

from fastapi import FastAPI, Request, HTTPException

from .config import settings
from .db import init_pool, close_pool
from .embeddings import init_embedder, embedding_worker
from .routes.ingest import router as ingest_router
from .routes.search import router as search_router

logging.basicConfig(level=logging.INFO, format="%(asctime)s %(name)s %(levelname)s %(message)s")
logger = logging.getLogger(__name__)


@asynccontextmanager
async def lifespan(app: FastAPI):
    await init_pool()
    init_embedder()
    task = asyncio.create_task(embedding_worker())
    logger.info("Memlayer server started")
    yield
    task.cancel()
    await close_pool()
    logger.info("Memlayer server stopped")


app = FastAPI(title="claude-mem-server", version="0.1.0", lifespan=lifespan)


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


@app.get("/health")
async def health():
    return {"status": "ok"}
