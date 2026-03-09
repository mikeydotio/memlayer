import logging
from datetime import datetime, timezone

from fastapi import APIRouter, HTTPException

from ..db import get_pool
from ..models import IngestEntry, IngestRequest, IngestResponse
from ..embeddings import enqueue_ids

logger = logging.getLogger(__name__)
router = APIRouter()


def _parse_ts(entry: IngestEntry) -> datetime:
    """Parse ISO 8601 timestamp string to datetime for asyncpg."""
    return datetime.fromisoformat(entry.timestamp.replace("Z", "+00:00"))


async def _batch_insert(pool, entries: list[IngestEntry]) -> tuple[list[int], int, int]:
    """Batch INSERT using unnest arrays. Returns (new_ids, duplicates, errors)."""
    # Build parallel arrays for unnest
    session_ids = [e.session_id for e in entries]
    message_types = [e.message_type for e in entries]
    content_types = [e.content_type for e in entries]
    raw_contents = [e.raw_content for e in entries]
    payload_hashes = [e.payload_hash for e in entries]
    source_uuids = [e.source_uuid for e in entries]
    parent_uuids = [e.parent_uuid for e in entries]
    tool_names = [e.tool_name for e in entries]
    cwds = [e.cwd for e in entries]
    timestamps = [_parse_ts(e) for e in entries]

    rows = await pool.fetch(
        """
        INSERT INTO memory_entries (
            session_id, message_type, content_type, raw_content,
            payload_hash, source_uuid, parent_uuid, tool_name, cwd, created_at
        )
        SELECT * FROM unnest(
            $1::text[], $2::text[], $3::text[], $4::text[],
            $5::text[], $6::text[], $7::text[], $8::text[], $9::text[], $10::timestamptz[]
        )
        ON CONFLICT (payload_hash) DO NOTHING
        RETURNING id
        """,
        session_ids,
        message_types,
        content_types,
        raw_contents,
        payload_hashes,
        source_uuids,
        parent_uuids,
        tool_names,
        cwds,
        timestamps,
    )

    new_ids = [r["id"] for r in rows]
    accepted = len(new_ids)
    duplicates = len(entries) - accepted
    return new_ids, duplicates, 0


async def _one_at_a_time_insert(pool, entries: list[IngestEntry]) -> tuple[list[int], int, int]:
    """Fallback: insert entries one at a time to isolate failures."""
    new_ids: list[int] = []
    duplicates = 0
    errors = 0

    for entry in entries:
        try:
            ts = _parse_ts(entry)
            result = await pool.fetchrow(
                """
                INSERT INTO memory_entries (
                    session_id, message_type, content_type, raw_content,
                    payload_hash, source_uuid, parent_uuid, tool_name, cwd, created_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                ON CONFLICT (payload_hash) DO NOTHING
                RETURNING id
                """,
                entry.session_id,
                entry.message_type,
                entry.content_type,
                entry.raw_content,
                entry.payload_hash,
                entry.source_uuid,
                entry.parent_uuid,
                entry.tool_name,
                entry.cwd,
                ts,
            )
            if result:
                new_ids.append(result["id"])
            else:
                duplicates += 1
        except Exception:
            logger.exception(f"Error inserting entry {entry.payload_hash}")
            errors += 1

    return new_ids, duplicates, errors


@router.post("/ingest", response_model=IngestResponse)
async def ingest(req: IngestRequest):
    if len(req.entries) > 200:
        raise HTTPException(400, "Max 200 entries per batch")

    pool = get_pool()

    # Upsert sessions (batch via executemany)
    session_map: dict[str, dict] = {}
    for entry in req.entries:
        if entry.session_id not in session_map:
            session_map[entry.session_id] = {
                "project_path": entry.project_path,
                "client_machine_id": entry.client_machine_id,
                "slug": entry.slug,
            }

    if session_map:
        await pool.executemany(
            """
            INSERT INTO claude_sessions (session_id, project_path, client_machine_id, slug)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (session_id) DO UPDATE SET last_seen_at = NOW()
            """,
            [
                (sid, meta["project_path"], meta["client_machine_id"], meta["slug"])
                for sid, meta in session_map.items()
            ],
        )

    # Batch INSERT with one-at-a-time fallback on error
    try:
        new_ids, duplicates, errors = await _batch_insert(pool, req.entries)
    except Exception:
        logger.warning("Batch INSERT failed, falling back to one-at-a-time", exc_info=True)
        new_ids, duplicates, errors = await _one_at_a_time_insert(pool, req.entries)

    accepted = len(new_ids)

    # Enqueue for embedding
    if new_ids:
        await enqueue_ids(new_ids)

    logger.info(
        f"Ingest: accepted={accepted} duplicates={duplicates} errors={errors} "
        f"sessions={len(session_map)} entries={len(req.entries)}"
    )

    return IngestResponse(accepted=accepted, duplicates=duplicates, errors=errors)
