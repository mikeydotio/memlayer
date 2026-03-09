import logging
from datetime import datetime, timezone

from fastapi import APIRouter, HTTPException

from ..db import get_pool
from ..models import IngestRequest, IngestResponse
from ..embeddings import enqueue_ids

logger = logging.getLogger(__name__)
router = APIRouter()


@router.post("/ingest", response_model=IngestResponse)
async def ingest(req: IngestRequest):
    if len(req.entries) > 200:
        raise HTTPException(400, "Max 200 entries per batch")

    pool = get_pool()
    accepted = 0
    duplicates = 0
    errors = 0
    new_ids: list[int] = []

    # Upsert sessions
    session_map: dict[str, dict] = {}
    for entry in req.entries:
        if entry.session_id not in session_map:
            session_map[entry.session_id] = {
                "project_path": entry.project_path,
                "client_machine_id": entry.client_machine_id,
                "slug": entry.slug,
            }

    for sid, meta in session_map.items():
        await pool.execute(
            """
            INSERT INTO claude_sessions (session_id, project_path, client_machine_id, slug)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (session_id) DO UPDATE SET last_seen_at = NOW()
            """,
            sid,
            meta["project_path"],
            meta["client_machine_id"],
            meta["slug"],
        )

    # Insert entries
    for entry in req.entries:
        try:
            # Parse timestamp string to datetime for asyncpg
            ts = datetime.fromisoformat(entry.timestamp.replace("Z", "+00:00"))

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
                accepted += 1
                new_ids.append(result["id"])
            else:
                duplicates += 1
        except Exception:
            logger.exception(f"Error inserting entry {entry.payload_hash}")
            errors += 1

    # Enqueue for embedding
    if new_ids:
        await enqueue_ids(new_ids)

    logger.info(
        f"Ingest: accepted={accepted} duplicates={duplicates} errors={errors} "
        f"sessions={len(session_map)} entries={len(req.entries)}"
    )

    return IngestResponse(accepted=accepted, duplicates=duplicates, errors=errors)
