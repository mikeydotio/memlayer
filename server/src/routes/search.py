import json
import logging
import time

import numpy as np
from fastapi import APIRouter, HTTPException, Query

from ..config import settings
from ..db import get_pool
from ..models import (
    LargeResponseRef,
    SearchRequest,
    SearchResponse,
    SearchResult,
    SessionSummary,
    SessionMessage,
)
from ..embeddings import embed_query
from ..file_storage import store_response_file
from ..indexing import generate_index

logger = logging.getLogger(__name__)
router = APIRouter()


async def _maybe_offload(
    response_json: str,
    threshold: int,
    source_endpoint: str,
    source_params: dict | None = None,
) -> LargeResponseRef | None:
    """If response exceeds threshold, store to file and return a LargeResponseRef."""
    if len(response_json) <= threshold:
        return None

    summary, structural_index, content_type = await generate_index(response_json)

    record = await store_response_file(
        content=response_json,
        source_endpoint=source_endpoint,
        source_params=source_params,
        summary=summary,
        structural_index=structural_index,
        content_type=content_type,
    )

    file_id = str(record["id"])
    return LargeResponseRef(
        file_id=file_id,
        file_url=f"/api/files/{file_id}",
        size_bytes=record["size_bytes"],
        summary=summary,
        index=structural_index,
        content_type=content_type,
    )


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

    response = SearchResponse(
        results=results,
        total=len(results),
        query_embedding_ms=embedding_ms,
        search_ms=search_ms,
    )

    # Check for large response offloading
    response_json = response.model_dump_json()
    large_ref = await _maybe_offload(
        response_json,
        settings.large_response_threshold_search,
        source_endpoint="/api/search",
        source_params={"query": req.query, "session_id": req.session_id, "project_path": req.project_path},
    )
    if large_ref:
        response.large_response = large_ref

    return response


@router.get("/sessions/{session_id}/summary", response_model=SessionSummary)
async def session_summary(session_id: str, limit: int = Query(default=200, ge=1, le=1000)):
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

    response = SessionSummary(
        session_id=session["session_id"],
        project_path=session["project_path"],
        slug=session["slug"],
        created_at=session["created_at"],
        message_count=len(messages),
        messages=messages,
    )

    # Check for large response offloading
    response_json = response.model_dump_json()
    large_ref = await _maybe_offload(
        response_json,
        settings.large_response_threshold_session,
        source_endpoint=f"/api/sessions/{session_id}/summary",
        source_params={"session_id": session_id, "limit": limit},
    )
    if large_ref:
        response.large_response = large_ref

    return response
