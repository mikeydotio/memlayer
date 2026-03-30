"""Optional data retention worker.

When RETENTION_DAYS > 0, periodically deletes memory entries older than the
configured threshold and cleans up orphaned extraction_log and entity_mention
records.
"""

import asyncio
import logging

from .config import settings
from .db import get_pool

logger = logging.getLogger(__name__)


async def retention_worker():
    """Background task that deletes old entries if retention_days > 0."""
    if settings.retention_days <= 0:
        logger.info("Data retention disabled (RETENTION_DAYS=0)")
        return

    logger.info(
        f"Retention worker started (retention_days={settings.retention_days}, "
        f"interval={settings.retention_check_interval_secs}s)"
    )

    while True:
        try:
            pool = get_pool()

            # Delete old entries
            result = await pool.execute(
                """
                DELETE FROM memory_entries
                WHERE created_at < NOW() - ($1 || ' days')::interval
                """,
                str(settings.retention_days),
            )
            deleted_entries = int(result.split()[-1]) if result else 0

            # Clean up orphaned extraction_log entries
            result = await pool.execute(
                """
                DELETE FROM extraction_log
                WHERE entry_id NOT IN (SELECT id FROM memory_entries)
                """
            )
            deleted_logs = int(result.split()[-1]) if result else 0

            # Clean up orphaned entity_mentions
            result = await pool.execute(
                """
                DELETE FROM entity_mentions
                WHERE entry_id NOT IN (SELECT id FROM memory_entries)
                """
            )
            deleted_mentions = int(result.split()[-1]) if result else 0

            if deleted_entries or deleted_logs or deleted_mentions:
                logger.info(
                    f"Retention: deleted {deleted_entries} entries, "
                    f"{deleted_logs} extraction logs, "
                    f"{deleted_mentions} entity mentions "
                    f"(older than {settings.retention_days} days)"
                )

        except Exception:
            logger.exception("Retention worker error")

        await asyncio.sleep(settings.retention_check_interval_secs)
