import logging

import numpy as np

from ..config import settings
from ..embeddings import embed_query

logger = logging.getLogger(__name__)


async def resolve_entity(
    pool,
    name: str,
    entity_type: str,
    description: str,
    project_path: str | None,
    confidence: float,
) -> tuple[int, bool]:
    """Resolve an extracted entity to an existing or new entity.

    Returns (entity_id, is_new).

    Resolution stages:
    1. Exact name match (same type and project)
    2. Trigram fuzzy match on canonical_name + aliases
    3. Embedding similarity match
    4. Create new entity if no match found
    """
    threshold = settings.entity_resolution_threshold

    # Stage 1: Exact match on canonical_name
    row = await pool.fetchrow(
        """
        SELECT id FROM entities
        WHERE lower(canonical_name) = lower($1)
          AND entity_type = $2
          AND (project_path = $3 OR project_path IS NULL OR $3 IS NULL)
          AND status != 'archived'
        LIMIT 1
        """,
        name, entity_type, project_path,
    )
    if row:
        await _update_existing(pool, row["id"])
        return row["id"], False

    # Stage 1b: Exact match on aliases
    row = await pool.fetchrow(
        """
        SELECT ea.entity_id FROM entity_aliases ea
        JOIN entities e ON e.id = ea.entity_id
        WHERE lower(ea.alias) = lower($1)
          AND e.entity_type = $2
          AND (e.project_path = $3 OR e.project_path IS NULL OR $3 IS NULL)
          AND e.status != 'archived'
        LIMIT 1
        """,
        name, entity_type, project_path,
    )
    if row:
        await _update_existing(pool, row["entity_id"])
        return row["entity_id"], False

    # Stage 2: Trigram fuzzy match
    row = await pool.fetchrow(
        """
        SELECT id, canonical_name, similarity(canonical_name, $1) AS sim
        FROM entities
        WHERE entity_type = $2
          AND (project_path = $3 OR project_path IS NULL OR $3 IS NULL)
          AND status != 'archived'
          AND similarity(canonical_name, $1) > $4
        ORDER BY sim DESC
        LIMIT 1
        """,
        name, entity_type, project_path, threshold,
    )
    if row:
        # High confidence match — add as alias
        if row["sim"] >= 0.8:
            await _add_alias(pool, row["id"], name)
            await _update_existing(pool, row["id"])
            return row["id"], False
        # Medium confidence — also check aliases table for better match
        alias_row = await pool.fetchrow(
            """
            SELECT ea.entity_id, similarity(ea.alias, $1) AS sim
            FROM entity_aliases ea
            JOIN entities e ON e.id = ea.entity_id
            WHERE e.entity_type = $2
              AND (e.project_path = $3 OR e.project_path IS NULL OR $3 IS NULL)
              AND e.status != 'archived'
              AND similarity(ea.alias, $1) > $4
            ORDER BY sim DESC
            LIMIT 1
            """,
            name, entity_type, project_path, threshold,
        )
        best_row = alias_row if alias_row and alias_row["sim"] > row["sim"] else row
        best_id = best_row.get("entity_id", best_row.get("id"))
        if best_row["sim"] >= threshold:
            await _add_alias(pool, best_id, name)
            await _update_existing(pool, best_id)
            return best_id, False

    # Stage 3: Embedding similarity
    embedding = await embed_query(name)
    if embedding:
        emb_array = np.array(embedding, dtype=np.float32)
        row = await pool.fetchrow(
            """
            SELECT id, canonical_name, 1 - (embedding <=> $1) AS sim
            FROM entities
            WHERE entity_type = $2
              AND (project_path = $3 OR project_path IS NULL OR $3 IS NULL)
              AND status != 'archived'
              AND embedding IS NOT NULL
            ORDER BY embedding <=> $1
            LIMIT 1
            """,
            emb_array, entity_type, project_path,
        )
        if row and row["sim"] >= threshold:
            await _add_alias(pool, row["id"], name)
            await _update_existing(pool, row["id"])
            return row["id"], False

    # Stage 4: Create new entity
    entity_id = await _create_entity(pool, name, entity_type, description, project_path, confidence, embedding)
    return entity_id, True


async def _update_existing(pool, entity_id: int):
    """Bump mention_count and last_seen_at for an existing entity."""
    await pool.execute(
        """
        UPDATE entities
        SET mention_count = mention_count + 1,
            last_seen_at = NOW(),
            updated_at = NOW()
        WHERE id = $1
        """,
        entity_id,
    )


async def _add_alias(pool, entity_id: int, alias: str):
    """Add an alias if it doesn't already exist for this entity."""
    await pool.execute(
        """
        INSERT INTO entity_aliases (entity_id, alias)
        SELECT $1, $2
        WHERE NOT EXISTS (
            SELECT 1 FROM entity_aliases
            WHERE entity_id = $1 AND lower(alias) = lower($2)
        )
        """,
        entity_id, alias,
    )


async def _create_entity(
    pool,
    name: str,
    entity_type: str,
    description: str,
    project_path: str | None,
    confidence: float,
    embedding: list[float] | None,
) -> int:
    """Create a new entity and return its ID."""
    emb_value = np.array(embedding, dtype=np.float32) if embedding else None
    row = await pool.fetchrow(
        """
        INSERT INTO entities (canonical_name, entity_type, description, project_path, confidence, embedding)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        """,
        name, entity_type, description, project_path, confidence, emb_value,
    )
    return row["id"]
