import asyncio
import logging
import time

import numpy as np
from openai import AsyncOpenAI
import httpx

from .config import settings
from .db import get_pool

logger = logging.getLogger(__name__)


class EmbeddingProvider:
    async def embed(self, texts: list[str]) -> list[list[float]]:
        raise NotImplementedError

    @property
    def dimensions(self) -> int:
        return settings.embedding_dimensions


class OpenAIEmbedder(EmbeddingProvider):
    def __init__(self):
        self.client = AsyncOpenAI(api_key=settings.openai_api_key)
        self.model = settings.embedding_model

    async def embed(self, texts: list[str]) -> list[list[float]]:
        resp = await self.client.embeddings.create(
            model=self.model,
            input=texts,
            dimensions=settings.embedding_dimensions,
        )
        return [item.embedding for item in resp.data]


class OllamaEmbedder(EmbeddingProvider):
    def __init__(self):
        self.base_url = settings.ollama_base_url
        self.model = settings.embedding_model

    async def embed(self, texts: list[str]) -> list[list[float]]:
        results = []
        async with httpx.AsyncClient(timeout=60.0) as client:
            for text in texts:
                resp = await client.post(
                    f"{self.base_url}/api/embed",
                    json={"model": self.model, "input": text},
                )
                resp.raise_for_status()
                data = resp.json()
                results.append(data["embeddings"][0])
        return results


def get_embedder() -> EmbeddingProvider | None:
    if settings.embedding_provider == "openai":
        if not settings.openai_api_key:
            logger.warning("No OPENAI_API_KEY set, running in FTS-only mode")
            return None
        return OpenAIEmbedder()
    elif settings.embedding_provider == "ollama":
        return OllamaEmbedder()
    else:
        logger.warning(f"Unknown embedding provider: {settings.embedding_provider}")
        return None


_embedder: EmbeddingProvider | None = None
_queue: list[int] = []
_queue_lock = asyncio.Lock()


def init_embedder():
    global _embedder
    _embedder = get_embedder()
    if _embedder:
        logger.info(f"Embedding provider: {settings.embedding_provider} ({settings.embedding_model})")
    else:
        logger.info("No embedding provider configured, FTS-only mode")


async def enqueue_ids(ids: list[int]):
    async with _queue_lock:
        _queue.extend(ids)


async def embedding_worker():
    """Background task that processes entries with NULL embeddings."""
    if not _embedder:
        return

    pool = get_pool()

    # Backfill: load all entry IDs with NULL embeddings
    # (just integer IDs — trivial memory even for 100K entries)
    rows = await pool.fetch(
        "SELECT id FROM memory_entries WHERE embedding IS NULL ORDER BY id"
    )
    if rows:
        backfill_ids = [r["id"] for r in rows]
        async with _queue_lock:
            _queue.extend(backfill_ids)
        logger.info(f"Backfill: queued {len(backfill_ids)} entries for embedding")

    error_backoff = 10
    while True:
        try:
            # Drain batch from queue
            async with _queue_lock:
                if not _queue:
                    await asyncio.sleep(settings.embedding_interval_secs)
                    continue
                batch_ids = _queue[:settings.embedding_batch_size]
                del _queue[:settings.embedding_batch_size]

            # Fetch texts
            rows = await pool.fetch(
                "SELECT id, raw_content FROM memory_entries WHERE id = ANY($1) AND embedding IS NULL",
                batch_ids,
            )
            if not rows:
                continue

            # Truncate long texts to avoid exceeding API token limits
            # text-embedding-3-small has an 8191 token limit (~32K chars)
            max_chars = 28000
            texts = [r["raw_content"][:max_chars] for r in rows]
            ids = [r["id"] for r in rows]

            # Generate embeddings
            start = time.monotonic()
            embeddings = await _embedder.embed(texts)
            elapsed = time.monotonic() - start
            logger.info(f"Generated {len(embeddings)} embeddings in {elapsed:.2f}s")

            # Update DB with provider/model metadata
            for entry_id, emb in zip(ids, embeddings):
                await pool.execute(
                    "UPDATE memory_entries SET embedding = $1, embedding_provider = $2, embedding_model = $3 WHERE id = $4",
                    np.array(emb, dtype=np.float32).tobytes(),
                    settings.embedding_provider,
                    settings.embedding_model,
                    entry_id,
                )

        except Exception as exc:
            logger.exception("Embedding worker error")
            # Exponential backoff for rate limits (up to 2 minutes)
            exc_str = str(exc).lower()
            if "rate" in exc_str or "429" in exc_str:
                backoff = min(error_backoff * 2, 120)
                error_backoff = backoff
                logger.warning(f"Rate limited, backing off {backoff}s")
                await asyncio.sleep(backoff)
            else:
                error_backoff = 10
                await asyncio.sleep(10)
            continue

        error_backoff = 10  # Reset on success
        await asyncio.sleep(settings.embedding_interval_secs)


async def embed_query(text: str) -> list[float] | None:
    """Generate embedding for a search query. Returns None if no provider or on error."""
    if not _embedder:
        return None
    try:
        embeddings = await _embedder.embed([text])
        return embeddings[0]
    except Exception:
        logger.warning("Failed to generate query embedding, falling back to FTS-only")
        return None
