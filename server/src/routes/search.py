import logging
import time

import numpy as np
from fastapi import APIRouter, HTTPException

from ..db import get_pool
from ..models import (
    SearchRequest,
    SearchResponse,
    SearchResult,
    SessionSummary,
    SessionMessage,
)
from ..embeddings import embed_query

logger = logging.getLogger(__name__)
router = APIRouter()


@router.post("/search", response_model=SearchResponse)
async def search(req: SearchRequest):
    pool = get_pool()

    # Generate query embedding
    t0 = time.monotonic()
    query_embedding = await embed_query(req.query)
    embedding_ms = (time.monotonic() - t0) * 1000

    t1 = time.monotonic()

    if query_embedding is not None:
        # Full hybrid search
        embedding_bytes = np.array(query_embedding, dtype=np.float32).tobytes()
        rows = await pool.fetch(
            "SELECT * FROM hybrid_search($1, $2, $3, $4, $5)",
            req.query,
            embedding_bytes,
            req.session_id,
            req.project_path,
            req.limit,
        )
    else:
        # FTS-only fallback
        rows = await pool.fetch(
            """
            SELECT me.id, me.session_id, me.message_type, me.content_type,
                   me.raw_content, me.tool_name, me.created_at,
                   cs.project_path,
                   ROW_NUMBER() OVER (
                       ORDER BY ts_rank_cd(me.fts, websearch_to_tsquery('english', $1)) DESC
                   )::INT AS fts_rank,
                   0::INT AS vector_rank,
                   ts_rank_cd(me.fts, websearch_to_tsquery('english', $1))::FLOAT AS rrf_score
            FROM memory_entries me
            JOIN claude_sessions cs ON me.session_id = cs.session_id
            WHERE me.fts @@ websearch_to_tsquery('english', $1)
                AND ($2::varchar IS NULL OR me.session_id = $2)
                AND ($3::varchar IS NULL OR cs.project_path = $3)
            ORDER BY ts_rank_cd(me.fts, websearch_to_tsquery('english', $1)) DESC
            LIMIT $4
            """,
            req.query,
            req.session_id,
            req.project_path,
            req.limit,
        )

    search_ms = (time.monotonic() - t1) * 1000

    results = [
        SearchResult(
            id=r["id"],
            session_id=r["session_id"],
            message_type=r["message_type"],
            content_type=r["content_type"],
            raw_content=r["raw_content"],
            tool_name=r["tool_name"],
            created_at=r["created_at"],
            project_path=r["project_path"],
            fts_rank=r["fts_rank"],
            vector_rank=r["vector_rank"],
            rrf_score=r["rrf_score"],
        )
        for r in rows
    ]

    return SearchResponse(
        results=results,
        total=len(results),
        query_embedding_ms=embedding_ms,
        search_ms=search_ms,
    )


@router.get("/sessions/{session_id}/summary", response_model=SessionSummary)
async def session_summary(session_id: str, limit: int = 200):
    pool = get_pool()

    session = await pool.fetchrow(
        "SELECT session_id, project_path, slug, created_at FROM claude_sessions WHERE session_id = $1",
        session_id,
    )
    if not session:
        raise HTTPException(404, f"Session {session_id} not found")

    rows = await pool.fetch(
        "SELECT * FROM get_session_entries($1, $2)",
        session_id,
        limit,
    )

    messages = [
        SessionMessage(
            id=r["id"],
            message_type=r["message_type"],
            content_type=r["content_type"],
            raw_content=r["raw_content"],
            tool_name=r["tool_name"],
            created_at=r["created_at"],
        )
        for r in rows
    ]

    return SessionSummary(
        session_id=session["session_id"],
        project_path=session["project_path"],
        slug=session["slug"],
        created_at=session["created_at"],
        message_count=len(messages),
        messages=messages,
    )
