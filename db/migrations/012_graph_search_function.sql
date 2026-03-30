-- Graph-augmented search: expand hybrid search results with 1-hop graph neighbors
CREATE OR REPLACE FUNCTION graph_expanded_search(
    query_text TEXT,
    query_embedding vector(1536),
    filter_session_id VARCHAR DEFAULT NULL,
    filter_project_path VARCHAR DEFAULT NULL,
    match_limit INT DEFAULT 20,
    fts_weight FLOAT DEFAULT 1.0,
    vector_weight FLOAT DEFAULT 1.0,
    filter_after TIMESTAMPTZ DEFAULT NULL,
    filter_before TIMESTAMPTZ DEFAULT NULL,
    filter_types VARCHAR[] DEFAULT NULL,
    graph_weight FLOAT DEFAULT 0.5
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
    rrf_score FLOAT,
    graph_boost FLOAT,
    related_entities JSONB
)
LANGUAGE sql STABLE AS $$
    -- Step 1: Run standard hybrid search (fetch extra for re-ranking headroom)
    WITH base_results AS (
        SELECT hs.*
        FROM hybrid_search(
            query_text, query_embedding,
            filter_session_id, filter_project_path,
            match_limit * 2,
            fts_weight, vector_weight,
            filter_after, filter_before, filter_types
        ) hs
    ),
    -- Step 2: Find active entities mentioned in base results
    result_entities AS (
        SELECT DISTINCT em.entity_id
        FROM base_results br
        JOIN entity_mentions em ON em.entry_id = br.id
        JOIN entities e ON e.id = em.entity_id
        WHERE e.status = 'active'
    ),
    -- Step 3: Find 1-hop graph neighbors via active relationships
    neighbor_entities AS (
        SELECT DISTINCT
            CASE
                WHEN er.source_entity_id = re.entity_id THEN er.target_entity_id
                ELSE er.source_entity_id
            END AS neighbor_id,
            er.relationship_type,
            er.confidence AS rel_confidence
        FROM result_entities re
        JOIN entity_relationships er ON (
            er.source_entity_id = re.entity_id OR er.target_entity_id = re.entity_id
        )
        WHERE er.valid_until IS NULL
    ),
    -- Step 4: Find entries mentioning neighbor entities (not already in base results)
    neighbor_entries AS (
        SELECT
            em.entry_id,
            SUM(graph_weight * ne.rel_confidence) AS boost,
            COUNT(*) AS connection_count
        FROM neighbor_entities ne
        JOIN entity_mentions em ON em.entity_id = ne.neighbor_id
        WHERE em.entry_id NOT IN (SELECT br.id FROM base_results br)
        GROUP BY em.entry_id
    ),
    -- Step 5: Combine base results + graph-expanded results
    all_results AS (
        -- Base results keep their original scores
        SELECT
            br.id, br.session_id, br.message_type, br.content_type,
            br.raw_content, br.tool_name, br.created_at, br.project_path,
            br.fts_rank, br.vector_rank, br.rrf_score,
            0.0::FLOAT AS graph_boost
        FROM base_results br
        UNION ALL
        -- Graph-expanded results have no RRF score, only graph boost
        SELECT
            me.id, me.session_id, me.message_type, me.content_type,
            me.raw_content, me.tool_name, me.created_at, cs.project_path,
            99::INT AS fts_rank, 99::INT AS vector_rank,
            0.0::FLOAT AS rrf_score,
            ne.boost::FLOAT AS graph_boost
        FROM neighbor_entries ne
        JOIN memory_entries me ON me.id = ne.entry_id
        JOIN claude_sessions cs ON cs.session_id = me.session_id
        WHERE (filter_session_id IS NULL OR me.session_id = filter_session_id)
          AND (filter_project_path IS NULL OR cs.project_path = filter_project_path)
          AND (filter_after IS NULL OR me.created_at >= filter_after)
          AND (filter_before IS NULL OR me.created_at <= filter_before)
          AND (filter_types IS NULL OR me.message_type = ANY(filter_types))
    )
    -- Step 6: Final ranking with graph boost, attach entity annotations
    SELECT
        ar.id, ar.session_id, ar.message_type, ar.content_type,
        ar.raw_content, ar.tool_name, ar.created_at, ar.project_path,
        ar.fts_rank, ar.vector_rank,
        (ar.rrf_score + ar.graph_boost)::FLOAT AS rrf_score,
        ar.graph_boost,
        COALESCE(
            (SELECT jsonb_agg(jsonb_build_object(
                'id', e.id,
                'name', e.canonical_name,
                'type', e.entity_type
            ))
            FROM entity_mentions em
            JOIN entities e ON e.id = em.entity_id
            WHERE em.entry_id = ar.id AND e.status = 'active'
            ), '[]'::jsonb
        ) AS related_entities
    FROM all_results ar
    ORDER BY (ar.rrf_score + ar.graph_boost) DESC
    LIMIT match_limit;
$$;
