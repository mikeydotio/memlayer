import json
import logging
import os
import uuid

from .config import settings
from .db import get_pool

logger = logging.getLogger(__name__)


async def store_response_file(
    content: str,
    source_endpoint: str,
    source_params: dict | None = None,
    summary: str | None = None,
    structural_index: str | None = None,
    content_type: str = "text",
) -> dict:
    """Write content to a file and record it in the DB. Returns the DB record as a dict."""
    file_id = str(uuid.uuid4())
    file_name = f"{file_id}.txt"
    file_path = os.path.join(settings.file_storage_path, file_name)

    os.makedirs(settings.file_storage_path, exist_ok=True)

    content_bytes = content.encode("utf-8")
    size_bytes = len(content_bytes)

    # Pre-check: evict before writing if hard limit would be exceeded
    if settings.file_storage_hard_limit > 0:
        total = await get_total_file_size()
        if total + size_bytes > settings.file_storage_hard_limit:
            target = int(settings.file_storage_hard_limit * 0.8)
            evicted = await evict_lru_files(target)
            if evicted:
                logger.info(f"Pre-write eviction: removed {evicted} files to stay within hard limit")

    with open(file_path, "w", encoding="utf-8") as f:
        f.write(content)

    pool = get_pool()
    row = await pool.fetchrow(
        """
        INSERT INTO response_files (id, file_path, size_bytes, content_type, summary,
                                     structural_index, source_endpoint, source_params)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, file_path, size_bytes, content_type, summary, structural_index,
                  source_endpoint, created_at, last_accessed_at
        """,
        uuid.UUID(file_id),
        file_path,
        size_bytes,
        content_type,
        summary,
        structural_index,
        source_endpoint,
        json.dumps(source_params) if source_params else None,
    )

    return dict(row)


async def get_file_path(file_id: str) -> str:
    """Return the disk path for a file, update last_accessed_at. Raises FileNotFoundError if missing."""
    pool = get_pool()
    row = await pool.fetchrow(
        """
        UPDATE response_files
        SET last_accessed_at = NOW()
        WHERE id = $1 AND deleted_at IS NULL
        RETURNING file_path
        """,
        uuid.UUID(file_id),
    )
    if not row:
        raise FileNotFoundError(f"Response file {file_id} not found or deleted")

    path = row["file_path"]
    if not os.path.exists(path):
        raise FileNotFoundError(f"Response file {file_id} missing from disk: {path}")

    return path


async def get_total_file_size() -> int:
    """Return total bytes of all non-deleted response files."""
    pool = get_pool()
    row = await pool.fetchrow(
        "SELECT COALESCE(SUM(size_bytes), 0) AS total FROM response_files WHERE deleted_at IS NULL"
    )
    return row["total"]


async def evict_lru_files(target_bytes: int) -> int:
    """Evict least-recently-accessed files until total is under target_bytes. Returns count evicted."""
    pool = get_pool()
    evicted = 0

    while True:
        total = await get_total_file_size()
        if total <= target_bytes:
            break

        row = await pool.fetchrow(
            """
            SELECT id, file_path FROM response_files
            WHERE deleted_at IS NULL
            ORDER BY last_accessed_at ASC
            LIMIT 1
            """
        )
        if not row:
            break

        # Tombstone the DB record
        await pool.execute(
            "UPDATE response_files SET deleted_at = NOW() WHERE id = $1",
            row["id"],
        )

        # Delete from disk
        try:
            os.remove(row["file_path"])
        except OSError:
            pass

        evicted += 1

    return evicted
