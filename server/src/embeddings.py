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

    # Catch up: process any entries that missed embedding
    pool = get_pool()
    rows = await pool.fetch(
        "SELECT id FROM memory_entries WHERE embedding IS NULL ORDER BY id LIMIT 1000"
    )
    if rows:
        logger.info(f"Catching up: {len(rows)} entries need embeddings")
        async with _queue_lock:
            _queue.extend([r["id"] for r in rows])

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

            texts = [r["raw_content"] for r in rows]
            ids = [r["id"] for r in rows]

            # Generate embeddings
            start = time.monotonic()
            embeddings = await _embedder.embed(texts)
            elapsed = time.monotonic() - start
            logger.info(f"Generated {len(embeddings)} embeddings in {elapsed:.2f}s")

            # Update DB
            for entry_id, emb in zip(ids, embeddings):
                await pool.execute(
                    "UPDATE memory_entries SET embedding = $1 WHERE id = $2",
                    np.array(emb, dtype=np.float32).tobytes(),
                    entry_id,
                )

        except Exception:
            logger.exception("Embedding worker error")
            await asyncio.sleep(10)

        await asyncio.sleep(settings.embedding_interval_secs)


async def embed_query(text: str) -> list[float] | None:
    """Generate embedding for a search query. Returns None if no provider."""
    if not _embedder:
        return None
    embeddings = await _embedder.embed([text])
    return embeddings[0]
