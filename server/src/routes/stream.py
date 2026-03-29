"""SSE endpoint for live entry streaming."""

import asyncio
import json
import logging

from fastapi import APIRouter, Request
from fastapi.responses import StreamingResponse

from ..event_bus import event_bus

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/stream/entries")
async def stream_entries(request: Request):
    """Server-Sent Events stream of newly ingested entries."""
    last_event_id = request.headers.get("Last-Event-ID")

    async def event_generator():
        sub_id, queue = event_bus.subscribe()
        logger.info("SSE client connected (id=%d)", sub_id)
        try:
            # Replay missed entries if Last-Event-ID provided
            if last_event_id:
                try:
                    last_id = int(last_event_id)
                    from ..db import get_pool
                    pool = get_pool()
                    rows = await pool.fetch(
                        """
                        SELECT id, session_id, message_type, content_type,
                               LEFT(raw_content, 200) AS content_preview,
                               tool_name, created_at,
                               (SELECT project_path FROM claude_sessions cs
                                WHERE cs.session_id = me.session_id) AS project_path
                        FROM memory_entries me
                        WHERE id > $1
                        ORDER BY id ASC
                        LIMIT 100
                        """,
                        last_id,
                    )
                    for row in rows:
                        data = {
                            "id": row["id"],
                            "session_id": row["session_id"],
                            "message_type": row["message_type"],
                            "content_type": row["content_type"],
                            "content_preview": row["content_preview"] or "",
                            "tool_name": row["tool_name"],
                            "created_at": row["created_at"].isoformat()
                            if row["created_at"]
                            else "",
                            "project_path": row["project_path"],
                        }
                        yield f"event: entry\nid: {row['id']}\ndata: {json.dumps(data)}\n\n"
                except (ValueError, Exception):
                    logger.warning(
                        "Failed to replay from Last-Event-ID: %s", last_event_id
                    )

            # Stream live entries
            while True:
                if await request.is_disconnected():
                    break
                try:
                    event = await asyncio.wait_for(queue.get(), timeout=15.0)
                    data_str = json.dumps(event)
                    entry_id = event.get("id", 0)
                    yield f"event: entry\nid: {entry_id}\ndata: {data_str}\n\n"
                except asyncio.TimeoutError:
                    # Send keepalive comment
                    yield ": keepalive\n\n"
        except asyncio.CancelledError:
            pass
        finally:
            event_bus.unsubscribe(sub_id)
            logger.info("SSE client disconnected (id=%d)", sub_id)

    return StreamingResponse(
        event_generator(),
        media_type="text/event-stream",
        headers={
            "Cache-Control": "no-cache",
            "X-Accel-Buffering": "no",
            "Connection": "keep-alive",
        },
    )
