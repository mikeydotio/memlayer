import asyncio
import logging
import time
from datetime import datetime, timezone

from ..config import settings
from ..db import get_pool
from .llm_extract import ExtractionProvider, get_extractor
from .entity_resolver import resolve_entity

logger = logging.getLogger(__name__)

_extractor: ExtractionProvider | None = None
_queue: list[int] = []
_queue_lock = asyncio.Lock()


def init_extractor():
    global _extractor
    if settings.extraction_mode == "off":
        logger.info("Knowledge graph extraction: off")
        return
    _extractor = get_extractor()
    if _extractor:
        logger.info(
            f"Knowledge graph extraction: {settings.extraction_mode} "
            f"(provider={settings.extraction_llm_provider}, model={settings.extraction_llm_model})"
        )
    else:
        logger.warning(
            f"extraction_mode={settings.extraction_mode} but no valid LLM provider configured"
        )


async def enqueue_extraction_ids(ids: list[int]):
    if not _extractor:
        return
    async with _queue_lock:
        _queue.extend(ids)


async def get_extraction_status() -> dict:
    """Return extraction progress stats."""
    pool = get_pool()
    total_extractable = await pool.fetchval(
        """
        SELECT COUNT(*) FROM memory_entries
        WHERE message_type IN ('user', 'assistant')
          AND LENGTH(raw_content) > 50
        """
    )
    extracted = await pool.fetchval(
        "SELECT COUNT(*) FROM extraction_log WHERE status = 'completed'"
    )
    failed = await pool.fetchval(
        "SELECT COUNT(*) FROM extraction_log WHERE status = 'failed'"
    )
    entity_count = await pool.fetchval("SELECT COUNT(*) FROM entities WHERE status = 'active'")
    relationship_count = await pool.fetchval(
        "SELECT COUNT(*) FROM entity_relationships WHERE valid_until IS NULL"
    )
    return {
        "total_extractable": total_extractable,
        "extracted": extracted,
        "failed": failed,
        "pending": total_extractable - extracted - failed,
        "queue_depth": len(_queue),
        "entity_count": entity_count,
        "relationship_count": relationship_count,
        "enabled": _extractor is not None,
        "provider": settings.extraction_llm_provider if _extractor else None,
        "model": settings.extraction_llm_model if _extractor else None,
    }


async def extraction_worker():
    """Background task that extracts entities and relationships from entries."""
    if not _extractor:
        return

    pool = get_pool()

    # Backfill: find entries not yet in extraction_log
    rows = await pool.fetch(
        """
        SELECT me.id FROM memory_entries me
        LEFT JOIN extraction_log el ON el.entry_id = me.id
        WHERE el.id IS NULL
          AND me.message_type IN ('user', 'assistant')
          AND LENGTH(me.raw_content) > 50
        ORDER BY me.id
        """
    )
    if rows:
        backfill_ids = [r["id"] for r in rows]
        async with _queue_lock:
            _queue.extend(backfill_ids)
        logger.info(f"Extraction backfill: queued {len(backfill_ids)} entries")

    error_backoff = 10
    while True:
        try:
            # Drain a batch from queue
            async with _queue_lock:
                if not _queue:
                    await asyncio.sleep(settings.extraction_interval_secs)
                    continue
                batch_ids = _queue[:settings.extraction_batch_size]
                del _queue[:settings.extraction_batch_size]

            # Fetch entries for this batch
            rows = await pool.fetch(
                """
                SELECT me.id, me.session_id, me.message_type, me.raw_content, me.created_at,
                       cs.project_path
                FROM memory_entries me
                JOIN claude_sessions cs ON cs.session_id = me.session_id
                WHERE me.id = ANY($1)
                  AND me.message_type IN ('user', 'assistant')
                  AND LENGTH(me.raw_content) > 50
                ORDER BY me.created_at ASC
                """,
                batch_ids,
            )
            if not rows:
                continue

            # Group by session for context-aware extraction
            session_groups: dict[str, list[dict]] = {}
            for r in rows:
                sid = r["session_id"]
                if sid not in session_groups:
                    session_groups[sid] = []
                session_groups[sid].append(dict(r))

            for session_id, entries in session_groups.items():
                await _process_session_batch(pool, session_id, entries)

        except Exception as exc:
            logger.exception("Extraction worker error")
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

        error_backoff = 10
        await asyncio.sleep(settings.extraction_interval_secs)


async def _process_session_batch(pool, session_id: str, new_entries: list[dict]):
    """Process a batch of entries from one session, with prior context."""
    # Fetch prior context: recent entries from this session that were already extracted
    earliest_id = min(e["id"] for e in new_entries)
    context_rows = await pool.fetch(
        """
        SELECT me.id, me.session_id, me.message_type, me.raw_content
        FROM memory_entries me
        JOIN extraction_log el ON el.entry_id = me.id AND el.status = 'completed'
        WHERE me.session_id = $1 AND me.id < $2
        ORDER BY me.id DESC
        LIMIT $3
        """,
        session_id, earliest_id, settings.extraction_context_window,
    )
    context_entries = [dict(r) for r in reversed(context_rows)]

    batch_key = f"{session_id}:{earliest_id}"
    project_path = new_entries[0].get("project_path")

    start = time.monotonic()
    try:
        result = await _extractor.extract(context_entries, new_entries)
    except Exception:
        logger.exception(f"LLM extraction failed for batch {batch_key}")
        for entry in new_entries:
            await _log_extraction(pool, entry["id"], session_id, batch_key, "failed", error="LLM call failed")
        return

    elapsed = time.monotonic() - start
    logger.info(
        f"Extraction: {len(result.entities)} entities, {len(result.relationships)} relationships "
        f"from {len(new_entries)} entries in {elapsed:.2f}s (tokens={result.tokens_used})"
    )

    # Filter by confidence threshold
    threshold = settings.extraction_confidence_threshold
    entities = [e for e in result.entities if e["confidence"] >= threshold]
    relationships = [r for r in result.relationships if r["confidence"] >= threshold]

    # Resolve entities and store mentions
    entity_name_to_id: dict[str, int] = {}
    entities_created = 0
    for entity_data in entities:
        entity_id, is_new = await resolve_entity(
            pool,
            name=entity_data["name"],
            entity_type=entity_data["type"],
            description=entity_data["description"],
            project_path=project_path,
            confidence=entity_data["confidence"],
        )
        entity_name_to_id[entity_data["name"].lower()] = entity_id
        if is_new:
            entities_created += 1

        # Create mentions linking entity to each new entry
        for entry in new_entries:
            # Only create mention if this entity is plausibly from this entry
            content_lower = entry["raw_content"][:2000].lower()
            name_lower = entity_data["name"].lower()
            # Check if the entity name (or significant words from it) appears in the entry
            name_words = [w for w in name_lower.split() if len(w) > 3]
            if name_lower in content_lower or any(w in content_lower for w in name_words):
                snippet = entry["raw_content"][:200]
                await pool.execute(
                    """
                    INSERT INTO entity_mentions (entity_id, entry_id, session_id, mention_text, context_snippet, confidence)
                    VALUES ($1, $2, $3, $4, $5, $6)
                    ON CONFLICT (entity_id, entry_id) DO NOTHING
                    """,
                    entity_id, entry["id"], session_id,
                    entity_data["name"], snippet, entity_data["confidence"],
                )

    # Store relationships
    rels_created = 0
    for rel in relationships:
        source_id = entity_name_to_id.get(rel["source"].lower())
        target_id = entity_name_to_id.get(rel["target"].lower())
        if source_id and target_id and source_id != target_id:
            # Find the entry that established this relationship (use the last entry in the batch)
            source_entry_id = new_entries[-1]["id"]
            await pool.execute(
                """
                INSERT INTO entity_relationships (
                    source_entity_id, target_entity_id, relationship_type,
                    description, confidence, source_entry_id
                ) VALUES ($1, $2, $3, $4, $5, $6)
                """,
                source_id, target_id, rel["type"],
                rel.get("reason", ""), rel["confidence"], source_entry_id,
            )
            rels_created += 1

    # Log extraction for each entry
    for entry in new_entries:
        await _log_extraction(
            pool, entry["id"], session_id, batch_key, "completed",
            entities_extracted=entities_created,
            relationships_extracted=rels_created,
            tokens_used=result.tokens_used,
        )


async def _log_extraction(
    pool,
    entry_id: int,
    session_id: str,
    batch_key: str,
    status: str,
    entities_extracted: int = 0,
    relationships_extracted: int = 0,
    tokens_used: int = 0,
    error: str | None = None,
):
    await pool.execute(
        """
        INSERT INTO extraction_log (entry_id, session_id, batch_key, status, entities_extracted,
                                     relationships_extracted, llm_provider, llm_model, tokens_used,
                                     error_message, completed_at)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT DO NOTHING
        """,
        entry_id, session_id, batch_key, status, entities_extracted,
        relationships_extracted, settings.extraction_llm_provider, settings.extraction_llm_model,
        tokens_used, error,
        datetime.now(timezone.utc) if status in ("completed", "failed") else None,
    )
