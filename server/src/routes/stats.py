"""Aggregate statistics endpoint for TUI dashboard."""

import logging
import time

from fastapi import APIRouter

from ..db import get_pool

logger = logging.getLogger(__name__)
router = APIRouter()

# Simple in-memory cache with TTL
_cache: dict = {}
_cache_ts: float = 0.0
_CACHE_TTL = 30.0  # seconds


@router.get("/stats")
async def aggregate_stats():
    """Aggregate statistics with 30s TTL cache."""
    global _cache, _cache_ts

    now = time.monotonic()
    if _cache and (now - _cache_ts) < _CACHE_TTL:
        return _cache

    pool = get_pool()

    # Run aggregate queries
    total_entries = await pool.fetchval("SELECT COUNT(*) FROM memory_entries") or 0
    total_sessions = await pool.fetchval("SELECT COUNT(*) FROM claude_sessions") or 0
    total_projects = (
        await pool.fetchval(
            "SELECT COUNT(DISTINCT project_path) FROM claude_sessions WHERE project_path IS NOT NULL"
        )
        or 0
    )

    # Embedding stats
    embedded = (
        await pool.fetchval(
            "SELECT COUNT(*) FROM memory_entries WHERE embedding IS NOT NULL"
        )
        or 0
    )
    pending = total_entries - embedded

    # Embedding provider info
    from ..config import settings

    provider = settings.embedding_provider if settings.embedding_provider != "off" else None
    model = settings.embedding_model if provider else None

    # Activity: entries per day, last 30 days
    activity_rows = await pool.fetch(
        """
        SELECT DATE(created_at) AS day, COUNT(*) AS entries
        FROM memory_entries
        WHERE created_at >= NOW() - INTERVAL '30 days'
        GROUP BY DATE(created_at)
        ORDER BY day DESC
        LIMIT 30
        """
    )

    # Contributors by source machine
    contributor_rows = await pool.fetch(
        """
        SELECT
            cs.client_machine_id AS machine_id,
            COUNT(DISTINCT cs.session_id) AS session_count,
            COUNT(me.id) AS entry_count,
            MAX(cs.last_seen_at) AS last_active
        FROM claude_sessions cs
        LEFT JOIN memory_entries me ON me.session_id = cs.session_id
        WHERE cs.client_machine_id IS NOT NULL
          AND cs.client_machine_id != ''
        GROUP BY cs.client_machine_id
        ORDER BY entry_count DESC
        LIMIT 20
        """
    )

    # Database size
    db_size = await pool.fetchval(
        "SELECT pg_database_size(current_database())"
    ) or 0

    result = {
        "totals": {
            "entries": total_entries,
            "sessions": total_sessions,
            "projects": total_projects,
        },
        "embeddings": {
            "total": total_entries,
            "embedded": embedded,
            "pending": pending,
            "provider": provider,
            "model": model,
        },
        "activity": [
            {"day": str(r["day"]), "entries": r["entries"]} for r in activity_rows
        ],
        "contributors": [
            {
                "machine_id": r["machine_id"],
                "session_count": r["session_count"],
                "entry_count": r["entry_count"],
                "last_active": str(r["last_active"]) if r["last_active"] else "",
            }
            for r in contributor_rows
        ],
        "database_size_bytes": db_size,
    }

    _cache = result
    _cache_ts = now
    return result
