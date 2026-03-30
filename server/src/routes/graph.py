"""Knowledge graph endpoints: stats, entity browsing, relationships."""

import logging
import time

from fastapi import APIRouter, HTTPException, Query
from pydantic import BaseModel, Field

from ..db import get_pool

logger = logging.getLogger(__name__)
router = APIRouter()


# --- Request/Response models ---

class EntityInfo(BaseModel):
    id: int
    canonical_name: str
    entity_type: str
    description: str | None
    project_path: str | None
    status: str
    confidence: float
    mention_count: int
    first_seen_at: str
    last_seen_at: str


class AliasInfo(BaseModel):
    id: int
    alias: str


class MentionInfo(BaseModel):
    id: int
    entry_id: int
    session_id: str
    mention_text: str | None
    context_snippet: str | None
    confidence: float
    created_at: str


class RelationshipInfo(BaseModel):
    id: int
    direction: str  # "outgoing" or "incoming"
    related_entity: EntityInfo
    relationship_type: str
    description: str | None
    confidence: float
    valid_from: str
    valid_until: str | None


class EntityDetail(BaseModel):
    entity: EntityInfo
    aliases: list[AliasInfo]
    mentions: list[MentionInfo]
    relationships: list[RelationshipInfo]


class EntitiesPage(BaseModel):
    entities: list[EntityInfo]
    total: int
    limit: int
    offset: int


class EntityUpdate(BaseModel):
    canonical_name: str | None = None
    status: str | None = None
    merge_into: int | None = None


class RelationshipCreate(BaseModel):
    source_entity_id: int
    target_entity_id: int
    relationship_type: str
    description: str | None = None
    confidence: float = Field(default=1.0, ge=0.0, le=1.0)


class GraphNeighbors(BaseModel):
    center: EntityInfo
    nodes: list[EntityInfo]
    edges: list[dict]


class EntityCreate(BaseModel):
    canonical_name: str
    entity_type: str
    description: str | None = None
    project_path: str | None = None
    confidence: float = Field(default=1.0, ge=0.0, le=1.0)
    source: str = Field(default="agent", description="Origin: 'agent' or 'extraction'")

# Simple cache for graph stats
_cache: dict = {}
_cache_ts: float = 0.0
_CACHE_TTL = 30.0


@router.get("/graph/stats")
async def graph_stats():
    """Knowledge graph statistics with 30s TTL cache."""
    global _cache, _cache_ts

    now = time.monotonic()
    if _cache and (now - _cache_ts) < _CACHE_TTL:
        return _cache

    pool = get_pool()

    entity_count = await pool.fetchval(
        "SELECT COUNT(*) FROM entities WHERE status = 'active'"
    ) or 0
    total_entities = await pool.fetchval("SELECT COUNT(*) FROM entities") or 0
    relationship_count = await pool.fetchval(
        "SELECT COUNT(*) FROM entity_relationships WHERE valid_until IS NULL"
    ) or 0
    mention_count = await pool.fetchval("SELECT COUNT(*) FROM entity_mentions") or 0

    # Entities by type
    type_rows = await pool.fetch(
        """
        SELECT entity_type, COUNT(*) AS count
        FROM entities WHERE status = 'active'
        GROUP BY entity_type ORDER BY count DESC
        """
    )

    # Relationships by type
    rel_type_rows = await pool.fetch(
        """
        SELECT relationship_type, COUNT(*) AS count
        FROM entity_relationships WHERE valid_until IS NULL
        GROUP BY relationship_type ORDER BY count DESC
        """
    )

    # Extraction progress
    from ..extraction import get_extraction_status
    extraction = await get_extraction_status()

    # Top entities by mention count
    top_rows = await pool.fetch(
        """
        SELECT id, canonical_name, entity_type, mention_count
        FROM entities WHERE status = 'active'
        ORDER BY mention_count DESC
        LIMIT 10
        """
    )

    result = {
        "entities": {
            "active": entity_count,
            "total": total_entities,
            "by_type": {r["entity_type"]: r["count"] for r in type_rows},
        },
        "relationships": {
            "active": relationship_count,
            "by_type": {r["relationship_type"]: r["count"] for r in rel_type_rows},
        },
        "mentions": mention_count,
        "extraction": extraction,
        "top_entities": [
            {
                "id": r["id"],
                "name": r["canonical_name"],
                "type": r["entity_type"],
                "mentions": r["mention_count"],
            }
            for r in top_rows
        ],
    }

    _cache = result
    _cache_ts = now
    return result


# --- Helper to build EntityInfo from a DB row ---

def _entity_from_row(r) -> EntityInfo:
    return EntityInfo(
        id=r["id"],
        canonical_name=r["canonical_name"],
        entity_type=r["entity_type"],
        description=r["description"],
        project_path=r["project_path"],
        status=r["status"],
        confidence=r["confidence"],
        mention_count=r["mention_count"],
        first_seen_at=r["first_seen_at"].isoformat() if r["first_seen_at"] else "",
        last_seen_at=r["last_seen_at"].isoformat() if r["last_seen_at"] else "",
    )


# --- Entity endpoints ---

@router.get("/entities", response_model=EntitiesPage)
async def list_entities(
    q: str | None = Query(default=None, description="Fuzzy search on entity name"),
    type: str | None = Query(default=None, alias="type", description="Filter by entity_type"),
    project_path: str | None = Query(default=None),
    status: str = Query(default="active"),
    limit: int = Query(default=50, ge=1, le=200),
    offset: int = Query(default=0, ge=0),
):
    pool = get_pool()

    if q:
        # Fuzzy search using trigram similarity
        rows = await pool.fetch(
            """
            SELECT e.*, similarity(e.canonical_name, $1) AS sim
            FROM entities e
            WHERE ($2::varchar IS NULL OR e.entity_type = $2)
              AND ($3::varchar IS NULL OR e.project_path = $3)
              AND e.status = $4
              AND (similarity(e.canonical_name, $1) > 0.2
                   OR e.canonical_name ILIKE '%' || $1 || '%')
            ORDER BY sim DESC, e.mention_count DESC
            LIMIT $5 OFFSET $6
            """,
            q, type, project_path, status, limit, offset,
        )
        total = await pool.fetchval(
            """
            SELECT COUNT(*) FROM entities e
            WHERE ($2::varchar IS NULL OR e.entity_type = $2)
              AND ($3::varchar IS NULL OR e.project_path = $3)
              AND e.status = $4
              AND (similarity(e.canonical_name, $1) > 0.2
                   OR e.canonical_name ILIKE '%' || $1 || '%')
            """,
            q, type, project_path, status,
        ) or 0
    else:
        rows = await pool.fetch(
            """
            SELECT * FROM entities
            WHERE ($1::varchar IS NULL OR entity_type = $1)
              AND ($2::varchar IS NULL OR project_path = $2)
              AND status = $3
            ORDER BY mention_count DESC, last_seen_at DESC
            LIMIT $4 OFFSET $5
            """,
            type, project_path, status, limit, offset,
        )
        total = await pool.fetchval(
            """
            SELECT COUNT(*) FROM entities
            WHERE ($1::varchar IS NULL OR entity_type = $1)
              AND ($2::varchar IS NULL OR project_path = $2)
              AND status = $3
            """,
            type, project_path, status,
        ) or 0

    return EntitiesPage(
        entities=[_entity_from_row(r) for r in rows],
        total=total,
        limit=limit,
        offset=offset,
    )


@router.post("/entities")
async def create_entity(body: EntityCreate):
    """Create an entity directly (for agent-directed memory)."""
    pool = get_pool()

    # Check for existing entity with same name, type, and project
    existing = await pool.fetchrow(
        """
        SELECT id FROM entities
        WHERE lower(canonical_name) = lower($1) AND entity_type = $2
          AND (project_path = $3 OR (project_path IS NULL AND $3 IS NULL))
          AND status != 'archived'
        """,
        body.canonical_name, body.entity_type, body.project_path,
    )
    if existing:
        # Update existing entity
        await pool.execute(
            """
            UPDATE entities SET mention_count = mention_count + 1,
                last_seen_at = NOW(), updated_at = NOW(),
                confidence = GREATEST(confidence, $2)
            WHERE id = $1
            """,
            existing["id"], body.confidence,
        )
        return {"created": False, "entity_id": existing["id"], "action": "updated_existing"}

    row = await pool.fetchrow(
        """
        INSERT INTO entities (canonical_name, entity_type, description, project_path, confidence)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        """,
        body.canonical_name, body.entity_type, body.description,
        body.project_path, body.confidence,
    )
    return {"created": True, "entity_id": row["id"]}


@router.get("/entities/{entity_id}", response_model=EntityDetail)
async def get_entity(entity_id: int):
    pool = get_pool()

    row = await pool.fetchrow("SELECT * FROM entities WHERE id = $1", entity_id)
    if not row:
        raise HTTPException(404, f"Entity {entity_id} not found")

    aliases = await pool.fetch(
        "SELECT id, alias FROM entity_aliases WHERE entity_id = $1 ORDER BY id",
        entity_id,
    )

    mentions = await pool.fetch(
        """
        SELECT id, entry_id, session_id, mention_text, context_snippet, confidence, created_at
        FROM entity_mentions WHERE entity_id = $1
        ORDER BY created_at DESC LIMIT 50
        """,
        entity_id,
    )

    # Outgoing relationships
    outgoing = await pool.fetch(
        """
        SELECT er.id AS rel_id, er.relationship_type, er.description AS rel_description,
               er.confidence AS rel_confidence, er.valid_from, er.valid_until, e.*
        FROM entity_relationships er
        JOIN entities e ON e.id = er.target_entity_id
        WHERE er.source_entity_id = $1
        ORDER BY er.created_at DESC
        """,
        entity_id,
    )

    # Incoming relationships
    incoming = await pool.fetch(
        """
        SELECT er.id AS rel_id, er.relationship_type, er.description AS rel_description,
               er.confidence AS rel_confidence, er.valid_from, er.valid_until, e.*
        FROM entity_relationships er
        JOIN entities e ON e.id = er.source_entity_id
        WHERE er.target_entity_id = $1
        ORDER BY er.created_at DESC
        """,
        entity_id,
    )

    relationships = []
    for r in outgoing:
        relationships.append(RelationshipInfo(
            id=r["rel_id"],
            direction="outgoing",
            related_entity=_entity_from_row(r),
            relationship_type=r["relationship_type"],
            description=r["rel_description"],
            confidence=r["rel_confidence"],
            valid_from=r["valid_from"].isoformat() if r["valid_from"] else "",
            valid_until=r["valid_until"].isoformat() if r["valid_until"] else None,
        ))
    for r in incoming:
        relationships.append(RelationshipInfo(
            id=r["rel_id"],
            direction="incoming",
            related_entity=_entity_from_row(r),
            relationship_type=r["relationship_type"],
            description=r["rel_description"],
            confidence=r["rel_confidence"],
            valid_from=r["valid_from"].isoformat() if r["valid_from"] else "",
            valid_until=r["valid_until"].isoformat() if r["valid_until"] else None,
        ))

    return EntityDetail(
        entity=_entity_from_row(row),
        aliases=[AliasInfo(id=a["id"], alias=a["alias"]) for a in aliases],
        mentions=[
            MentionInfo(
                id=m["id"],
                entry_id=m["entry_id"],
                session_id=m["session_id"],
                mention_text=m["mention_text"],
                context_snippet=m["context_snippet"],
                confidence=m["confidence"],
                created_at=m["created_at"].isoformat() if m["created_at"] else "",
            )
            for m in mentions
        ],
        relationships=relationships,
    )


@router.patch("/entities/{entity_id}")
async def update_entity(entity_id: int, body: EntityUpdate):
    pool = get_pool()

    row = await pool.fetchrow("SELECT id FROM entities WHERE id = $1", entity_id)
    if not row:
        raise HTTPException(404, f"Entity {entity_id} not found")

    # Merge operation
    if body.merge_into is not None:
        target = await pool.fetchrow("SELECT id, canonical_name FROM entities WHERE id = $1", body.merge_into)
        if not target:
            raise HTTPException(404, f"Merge target entity {body.merge_into} not found")
        if body.merge_into == entity_id:
            raise HTTPException(400, "Cannot merge entity into itself")

        # Get source name before merge
        source = await pool.fetchrow("SELECT canonical_name FROM entities WHERE id = $1", entity_id)

        # Transfer mentions
        await pool.execute(
            """
            UPDATE entity_mentions SET entity_id = $1
            WHERE entity_id = $2
              AND entry_id NOT IN (SELECT entry_id FROM entity_mentions WHERE entity_id = $1)
            """,
            body.merge_into, entity_id,
        )
        # Delete remaining duplicate mentions
        await pool.execute("DELETE FROM entity_mentions WHERE entity_id = $1", entity_id)

        # Transfer relationships (update source side)
        await pool.execute(
            """
            UPDATE entity_relationships SET source_entity_id = $1
            WHERE source_entity_id = $2 AND target_entity_id != $1
            """,
            body.merge_into, entity_id,
        )
        # Transfer relationships (update target side)
        await pool.execute(
            """
            UPDATE entity_relationships SET target_entity_id = $1
            WHERE target_entity_id = $2 AND source_entity_id != $1
            """,
            body.merge_into, entity_id,
        )
        # Clean up any self-loops created by merge
        await pool.execute(
            "DELETE FROM entity_relationships WHERE source_entity_id = target_entity_id"
        )
        # Delete orphaned relationships
        await pool.execute(
            "DELETE FROM entity_relationships WHERE source_entity_id = $1 OR target_entity_id = $1",
            entity_id,
        )

        # Add source name as alias on target
        await pool.execute(
            """
            INSERT INTO entity_aliases (entity_id, alias)
            SELECT $1, $2
            WHERE NOT EXISTS (
                SELECT 1 FROM entity_aliases WHERE entity_id = $1 AND lower(alias) = lower($2)
            )
            """,
            body.merge_into, source["canonical_name"],
        )

        # Transfer aliases
        await pool.execute(
            "UPDATE entity_aliases SET entity_id = $1 WHERE entity_id = $2",
            body.merge_into, entity_id,
        )

        # Update mention count on target
        new_count = await pool.fetchval(
            "SELECT COUNT(*) FROM entity_mentions WHERE entity_id = $1",
            body.merge_into,
        )
        await pool.execute(
            "UPDATE entities SET mention_count = $1, updated_at = NOW() WHERE id = $2",
            new_count, body.merge_into,
        )

        # Archive source entity
        await pool.execute(
            "UPDATE entities SET status = 'archived', updated_at = NOW() WHERE id = $1",
            entity_id,
        )

        return {"merged": True, "source": entity_id, "target": body.merge_into}

    # Simple update
    updates = []
    params = []
    idx = 1
    if body.canonical_name is not None:
        updates.append(f"canonical_name = ${idx}")
        params.append(body.canonical_name)
        idx += 1
    if body.status is not None:
        updates.append(f"status = ${idx}")
        params.append(body.status)
        idx += 1

    if not updates:
        raise HTTPException(400, "No fields to update")

    updates.append(f"updated_at = NOW()")
    params.append(entity_id)
    query = f"UPDATE entities SET {', '.join(updates)} WHERE id = ${idx}"
    await pool.execute(query, *params)

    return {"updated": True, "entity_id": entity_id}


# --- Relationship endpoints ---

@router.post("/relationships")
async def create_relationship(body: RelationshipCreate):
    pool = get_pool()

    if body.source_entity_id == body.target_entity_id:
        raise HTTPException(400, "Cannot create self-referencing relationship")

    # Verify both entities exist
    source = await pool.fetchrow("SELECT id FROM entities WHERE id = $1", body.source_entity_id)
    if not source:
        raise HTTPException(404, f"Source entity {body.source_entity_id} not found")
    target = await pool.fetchrow("SELECT id FROM entities WHERE id = $1", body.target_entity_id)
    if not target:
        raise HTTPException(404, f"Target entity {body.target_entity_id} not found")

    row = await pool.fetchrow(
        """
        INSERT INTO entity_relationships (
            source_entity_id, target_entity_id, relationship_type,
            description, confidence
        ) VALUES ($1, $2, $3, $4, $5)
        RETURNING id
        """,
        body.source_entity_id, body.target_entity_id, body.relationship_type,
        body.description, body.confidence,
    )

    return {"created": True, "relationship_id": row["id"]}


@router.delete("/relationships/{relationship_id}")
async def delete_relationship(relationship_id: int):
    pool = get_pool()

    row = await pool.fetchrow(
        "SELECT id FROM entity_relationships WHERE id = $1", relationship_id
    )
    if not row:
        raise HTTPException(404, f"Relationship {relationship_id} not found")

    await pool.execute(
        "UPDATE entity_relationships SET valid_until = NOW() WHERE id = $1",
        relationship_id,
    )
    return {"deleted": True, "relationship_id": relationship_id}


# --- Graph traversal ---

@router.get("/entities/{entity_id}/neighbors", response_model=GraphNeighbors)
async def get_neighbors(
    entity_id: int,
    hops: int = Query(default=1, ge=1, le=3),
    relationship_types: str | None = Query(default=None, description="Comma-separated filter"),
    direction: str = Query(default="both", description="outgoing, incoming, or both"),
):
    pool = get_pool()

    center_row = await pool.fetchrow("SELECT * FROM entities WHERE id = $1", entity_id)
    if not center_row:
        raise HTTPException(404, f"Entity {entity_id} not found")

    type_filter = [t.strip() for t in relationship_types.split(",") if t.strip()] if relationship_types else None

    # Collect neighbors hop by hop
    visited_ids = {entity_id}
    frontier = {entity_id}
    all_edges = []

    for _ in range(hops):
        if not frontier:
            break

        frontier_list = list(frontier)
        next_frontier = set()

        # Outgoing edges
        if direction in ("both", "outgoing"):
            rows = await pool.fetch(
                """
                SELECT er.id AS rel_id, er.source_entity_id, er.target_entity_id,
                       er.relationship_type, er.confidence
                FROM entity_relationships er
                WHERE er.source_entity_id = ANY($1)
                  AND er.valid_until IS NULL
                  AND ($2::varchar[] IS NULL OR er.relationship_type = ANY($2))
                """,
                frontier_list, type_filter,
            )
            for r in rows:
                all_edges.append({
                    "id": r["rel_id"],
                    "source_id": r["source_entity_id"],
                    "target_id": r["target_entity_id"],
                    "relationship_type": r["relationship_type"],
                    "confidence": r["confidence"],
                })
                if r["target_entity_id"] not in visited_ids:
                    next_frontier.add(r["target_entity_id"])
                    visited_ids.add(r["target_entity_id"])

        # Incoming edges
        if direction in ("both", "incoming"):
            rows = await pool.fetch(
                """
                SELECT er.id AS rel_id, er.source_entity_id, er.target_entity_id,
                       er.relationship_type, er.confidence
                FROM entity_relationships er
                WHERE er.target_entity_id = ANY($1)
                  AND er.valid_until IS NULL
                  AND ($2::varchar[] IS NULL OR er.relationship_type = ANY($2))
                """,
                frontier_list, type_filter,
            )
            for r in rows:
                all_edges.append({
                    "id": r["rel_id"],
                    "source_id": r["source_entity_id"],
                    "target_id": r["target_entity_id"],
                    "relationship_type": r["relationship_type"],
                    "confidence": r["confidence"],
                })
                if r["source_entity_id"] not in visited_ids:
                    next_frontier.add(r["source_entity_id"])
                    visited_ids.add(r["source_entity_id"])

        frontier = next_frontier

    # Fetch all neighbor entity details
    neighbor_ids = list(visited_ids - {entity_id})
    nodes = []
    if neighbor_ids:
        rows = await pool.fetch(
            "SELECT * FROM entities WHERE id = ANY($1) ORDER BY mention_count DESC",
            neighbor_ids,
        )
        nodes = [_entity_from_row(r) for r in rows]

    # Deduplicate edges
    seen_edges = set()
    unique_edges = []
    for e in all_edges:
        key = e["id"]
        if key not in seen_edges:
            seen_edges.add(key)
            unique_edges.append(e)

    return GraphNeighbors(
        center=_entity_from_row(center_row),
        nodes=nodes,
        edges=unique_edges,
    )
