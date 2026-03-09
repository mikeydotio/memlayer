import asyncio
import logging

from .config import settings
from .file_storage import get_total_file_size, evict_lru_files

logger = logging.getLogger(__name__)


async def eviction_worker():
    """Background task that periodically evicts LRU response files when over soft limit."""
    if settings.file_storage_soft_limit <= 0:
        logger.info("File storage soft limit is 0 (unlimited), eviction worker disabled")
        return

    logger.info(
        f"Eviction worker started (soft_limit={settings.file_storage_soft_limit}, "
        f"interval={settings.eviction_interval_secs}s)"
    )

    while True:
        try:
            total = await get_total_file_size()
            if total > settings.file_storage_soft_limit:
                target = int(settings.file_storage_soft_limit * 0.8)
                evicted = await evict_lru_files(target)
                if evicted:
                    new_total = await get_total_file_size()
                    logger.info(
                        f"Eviction: removed {evicted} files, "
                        f"{total} -> {new_total} bytes"
                    )
        except Exception:
            logger.exception("Eviction worker error")

        await asyncio.sleep(settings.eviction_interval_secs)
