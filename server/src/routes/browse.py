"""Browse endpoints for TUI dashboard: projects, sessions, entries."""

import logging

from fastapi import APIRouter, Query

from ..db import get_pool

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/projects")
async def list_projects():
    """List distinct projects with session/entry counts."""
    pool = get_pool()
    rows = await pool.fetch(
        """
        SELECT
            cs.project_path,
            COUNT(DISTINCT cs.session_id) AS session_count,
            COALESCE(SUM(ec.cnt), 0) AS entry_count,
            MAX(cs.last_seen_at) AS last_activity
        FROM claude_sessions cs
        LEFT JOIN (
            SELECT session_id, COUNT(*) AS cnt
            FROM memory_entries
            GROUP BY session_id
        ) ec ON cs.session_id = ec.session_id
        WHERE cs.project_path IS NOT NULL
        GROUP BY cs.project_path
        ORDER BY MAX(cs.last_seen_at) DESC
        """
    )
    return [
        {
            "project_path": r["project_path"],
            "session_count": r["session_count"],
            "entry_count": r["entry_count"],
            "last_activity": r["last_activity"].isoformat() if r["last_activity"] else "",
        }
        for r in rows
    ]


@router.get("/sessions")
async def list_sessions(
    project_path: str | None = Query(None),
    offset: int = Query(0, ge=0),
    limit: int = Query(50, ge=1, le=200),
):
    """List sessions, optionally filtered by project path."""
    pool = get_pool()

    if project_path:
        rows = await pool.fetch(
            """
            SELECT
                cs.session_id, cs.slug, cs.created_at, cs.last_seen_at,
                COUNT(me.id) AS entry_count
            FROM claude_sessions cs
            LEFT JOIN memory_entries me ON cs.session_id = me.session_id
            WHERE cs.project_path = $1
            GROUP BY cs.session_id
            ORDER BY cs.last_seen_at DESC
            LIMIT $2 OFFSET $3
            """,
            project_path,
            limit,
            offset,
        )
        total_row = await pool.fetchval(
            "SELECT COUNT(*) FROM claude_sessions WHERE project_path = $1",
            project_path,
        )
    else:
        rows = await pool.fetch(
            """
            SELECT
                cs.session_id, cs.slug, cs.created_at, cs.last_seen_at,
                COUNT(me.id) AS entry_count
            FROM claude_sessions cs
            LEFT JOIN memory_entries me ON cs.session_id = me.session_id
            GROUP BY cs.session_id
            ORDER BY cs.last_seen_at DESC
            LIMIT $1 OFFSET $2
            """,
            limit,
            offset,
        )
        total_row = await pool.fetchval("SELECT COUNT(*) FROM claude_sessions")

    return {
        "sessions": [
            {
                "session_id": r["session_id"],
                "slug": r["slug"],
                "created_at": r["created_at"].isoformat() if r["created_at"] else "",
                "last_seen_at": r["last_seen_at"].isoformat()
                if r["last_seen_at"]
                else "",
                "entry_count": r["entry_count"],
            }
            for r in rows
        ],
        "total": total_row or 0,
        "limit": limit,
        "offset": offset,
    }


@router.get("/entries/recent")
async def recent_entries(
    machine_id: str | None = Query(None),
    limit: int = Query(10, ge=1, le=200),
):
    """Recent entries across all sessions, optionally filtered by host machine."""
    pool = get_pool()

    rows = await pool.fetch(
        """
        SELECT me.id, me.session_id, me.message_type, me.content_type,
               LEFT(me.raw_content, 200) AS content_preview,
               me.tool_name, me.created_at,
               cs.project_path, cs.slug
        FROM memory_entries me
        JOIN claude_sessions cs ON me.session_id = cs.session_id
        WHERE ($1::varchar IS NULL OR cs.client_machine_id = $1)
        ORDER BY me.created_at DESC
        LIMIT $2
        """,
        machine_id,
        limit,
    )

    # Total count (with same filter)
    total = await pool.fetchval(
        """
        SELECT COUNT(*)
        FROM memory_entries me
        JOIN claude_sessions cs ON me.session_id = cs.session_id
        WHERE ($1::varchar IS NULL OR cs.client_machine_id = $1)
        """,
        machine_id,
    ) or 0

    return {
        "entries": [
            {
                "id": r["id"],
                "session_id": r["session_id"],
                "message_type": r["message_type"],
                "content_type": r["content_type"],
                "content_preview": r["content_preview"] or "",
                "tool_name": r["tool_name"],
                "created_at": r["created_at"].isoformat() if r["created_at"] else "",
                "project_path": r["project_path"],
                "slug": r["slug"],
            }
            for r in rows
        ],
        "total": total,
        "limit": limit,
        "machine_id": machine_id,
    }


@router.get("/sessions/{session_id}/entries")
async def list_session_entries(
    session_id: str,
    cursor: int | None = Query(None),
    limit: int = Query(50, ge=1, le=200),
):
    """Paginated entries for a session using cursor-based pagination."""
    pool = get_pool()

    if cursor:
        rows = await pool.fetch(
            """
            SELECT id, message_type, content_type,
                   LEFT(raw_content, 200) AS content_preview,
                   tool_name, created_at
            FROM memory_entries
            WHERE session_id = $1 AND id > $2
            ORDER BY id ASC
            LIMIT $3
            """,
            session_id,
            cursor,
            limit + 1,  # fetch one extra to check has_more
        )
    else:
        rows = await pool.fetch(
            """
            SELECT id, message_type, content_type,
                   LEFT(raw_content, 200) AS content_preview,
                   tool_name, created_at
            FROM memory_entries
            WHERE session_id = $1
            ORDER BY id ASC
            LIMIT $2
            """,
            session_id,
            limit + 1,
        )

    has_more = len(rows) > limit
    entries = rows[:limit]

    return {
        "entries": [
            {
                "id": r["id"],
                "message_type": r["message_type"],
                "content_type": r["content_type"],
                "content_preview": r["content_preview"] or "",
                "tool_name": r["tool_name"],
                "created_at": r["created_at"].isoformat() if r["created_at"] else "",
            }
            for r in entries
        ],
        "cursor": str(entries[-1]["id"]) if entries else None,
        "has_more": has_more,
    }
