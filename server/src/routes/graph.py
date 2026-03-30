"""Knowledge graph endpoints: stats, entity browsing, relationships."""

import logging
import time

from fastapi import APIRouter

from ..db import get_pool

logger = logging.getLogger(__name__)
router = APIRouter()

# Simple cache for graph stats
_cache: dict = {}
_cache_ts: float = 0.0
_CACHE_TTL = 30.0


@router.get("/graph/stats")
async def graph_stats():
    """Knowledge graph statistics with 30s TTL cache."""
    global _cache, _cache_ts

    now = time.monotonic()
    if _cache and (now - _cache_ts) < _CACHE_TTL:
        return _cache

    pool = get_pool()

    entity_count = await pool.fetchval(
        "SELECT COUNT(*) FROM entities WHERE status = 'active'"
    ) or 0
    total_entities = await pool.fetchval("SELECT COUNT(*) FROM entities") or 0
    relationship_count = await pool.fetchval(
        "SELECT COUNT(*) FROM entity_relationships WHERE valid_until IS NULL"
    ) or 0
    mention_count = await pool.fetchval("SELECT COUNT(*) FROM entity_mentions") or 0

    # Entities by type
    type_rows = await pool.fetch(
        """
        SELECT entity_type, COUNT(*) AS count
        FROM entities WHERE status = 'active'
        GROUP BY entity_type ORDER BY count DESC
        """
    )

    # Relationships by type
    rel_type_rows = await pool.fetch(
        """
        SELECT relationship_type, COUNT(*) AS count
        FROM entity_relationships WHERE valid_until IS NULL
        GROUP BY relationship_type ORDER BY count DESC
        """
    )

    # Extraction progress
    from ..extraction import get_extraction_status
    extraction = await get_extraction_status()

    # Top entities by mention count
    top_rows = await pool.fetch(
        """
        SELECT id, canonical_name, entity_type, mention_count
        FROM entities WHERE status = 'active'
        ORDER BY mention_count DESC
        LIMIT 10
        """
    )

    result = {
        "entities": {
            "active": entity_count,
            "total": total_entities,
            "by_type": {r["entity_type"]: r["count"] for r in type_rows},
        },
        "relationships": {
            "active": relationship_count,
            "by_type": {r["relationship_type"]: r["count"] for r in rel_type_rows},
        },
        "mentions": mention_count,
        "extraction": extraction,
        "top_entities": [
            {
                "id": r["id"],
                "name": r["canonical_name"],
                "type": r["entity_type"],
                "mentions": r["mention_count"],
            }
            for r in top_rows
        ],
    }

    _cache = result
    _cache_ts = now
    return result
