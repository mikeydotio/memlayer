import logging

from fastapi import APIRouter

from ..embeddings import get_embedding_status

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/embeddings/status")
async def embedding_status():
    """Return embedding progress: total, embedded, pending, queue depth."""
    return await get_embedding_status()
