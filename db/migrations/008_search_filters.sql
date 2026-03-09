-- Add date range and content type filters to hybrid_search
CREATE OR REPLACE FUNCTION hybrid_search(
    query_text TEXT,
    query_embedding vector(1536),
    filter_session_id VARCHAR DEFAULT NULL,
    filter_project_path VARCHAR DEFAULT NULL,
    match_limit INT DEFAULT 20,
    fts_weight FLOAT DEFAULT 1.0,
    vector_weight FLOAT DEFAULT 1.0,
    filter_after TIMESTAMPTZ DEFAULT NULL,
    filter_before TIMESTAMPTZ DEFAULT NULL,
    filter_types VARCHAR[] DEFAULT NULL
)
RETURNS TABLE (
    id BIGINT,
    session_id VARCHAR,
    message_type VARCHAR,
    content_type VARCHAR,
    raw_content TEXT,
    tool_name VARCHAR,
    created_at TIMESTAMPTZ,
    project_path VARCHAR,
    fts_rank INT,
    vector_rank INT,
    rrf_score FLOAT
)
LANGUAGE sql STABLE AS $$
    WITH fts_results AS (
        SELECT
            me.id,
            ROW_NUMBER() OVER (
                ORDER BY ts_rank_cd(me.fts, websearch_to_tsquery('english', query_text)) DESC
            ) AS rank
        FROM memory_entries me
        WHERE me.fts @@ websearch_to_tsquery('english', query_text)
            AND (filter_session_id IS NULL OR me.session_id = filter_session_id)
            AND (filter_project_path IS NULL OR me.session_id IN (
                SELECT cs.session_id FROM claude_sessions cs
                WHERE cs.project_path = filter_project_path
            ))
            AND (filter_after IS NULL OR me.created_at >= filter_after)
            AND (filter_before IS NULL OR me.created_at <= filter_before)
            AND (filter_types IS NULL OR me.message_type = ANY(filter_types))
        ORDER BY ts_rank_cd(me.fts, websearch_to_tsquery('english', query_text)) DESC
        LIMIT 50
    ),
    vector_results AS (
        SELECT
            me.id,
            ROW_NUMBER() OVER (ORDER BY me.embedding <=> query_embedding) AS rank
        FROM memory_entries me
        WHERE me.embedding IS NOT NULL
            AND (filter_session_id IS NULL OR me.session_id = filter_session_id)
            AND (filter_project_path IS NULL OR me.session_id IN (
                SELECT cs.session_id FROM claude_sessions cs
                WHERE cs.project_path = filter_project_path
            ))
            AND (filter_after IS NULL OR me.created_at >= filter_after)
            AND (filter_before IS NULL OR me.created_at <= filter_before)
            AND (filter_types IS NULL OR me.message_type = ANY(filter_types))
        ORDER BY me.embedding <=> query_embedding
        LIMIT 50
    ),
    combined AS (
        SELECT
            COALESCE(f.id, v.id) AS id,
            COALESCE(f.rank, 51) AS fts_rank,
            COALESCE(v.rank, 51) AS vector_rank,
            (fts_weight / (60.0 + COALESCE(f.rank, 51))) +
            (vector_weight / (60.0 + COALESCE(v.rank, 51))) AS rrf_score
        FROM fts_results f
        FULL OUTER JOIN vector_results v ON f.id = v.id
    )
    SELECT
        me.id,
        me.session_id,
        me.message_type,
        me.content_type,
        me.raw_content,
        me.tool_name,
        me.created_at,
        cs.project_path,
        c.fts_rank::INT,
        c.vector_rank::INT,
        c.rrf_score::FLOAT
    FROM combined c
    JOIN memory_entries me ON me.id = c.id
    JOIN claude_sessions cs ON me.session_id = cs.session_id
    ORDER BY c.rrf_score DESC
    LIMIT match_limit;
$$;
