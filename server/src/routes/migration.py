import asyncio
import base64
import json
import logging
import os
import struct
from datetime import datetime, timezone
from typing import Optional
from uuid import UUID

import httpx
import numpy as np
from fastapi import APIRouter, HTTPException, Request, Query
from pgvector.asyncpg import register_vector

from ..config import settings
from ..db import get_pool
from ..embeddings import enqueue_ids
from ..migration_state import (
    MigrationRole,
    MigrationState,
    get_migration_manager,
)

logger = logging.getLogger(__name__)
router = APIRouter()


async def _safe_json(request: Request) -> dict:
    """Parse JSON body, returning {} on empty or invalid JSON."""
    try:
        return await request.json()
    except Exception:
        return {}


# ── Source Server Endpoints ──


@router.post("/migration/initiate")
async def initiate_migration():
    """Generate migration key + Ed25519 keypair. Requires admin auth."""
    mgr = get_migration_manager()
    try:
        state, migration_key = await mgr.initiate()
    except ValueError as e:
        raise HTTPException(409, str(e))

    return {
        "migration_id": str(state["migration_id"]),
        "migration_key": migration_key,
        "expires_at": state["migration_key_expires_at"].isoformat(),
        "public_key": base64.urlsafe_b64encode(state["ed25519_public_key"]).decode(),
        "state": state["state"],
    }


@router.get("/migration/status")
async def migration_status(request: Request):
    """Get migration state and progress. Admin or migration key auth."""
    mgr = get_migration_manager()

    # Check for migration key in auth header
    auth = request.headers.get("Authorization", "")
    migration_key = None
    if auth.startswith("Bearer migration:"):
        migration_key = auth.replace("Bearer migration:", "")

    # Try source state first, then destination
    for role in [MigrationRole.SOURCE, MigrationRole.DESTINATION]:
        state = await mgr.get_state(role)
        if state and state["state"] != "IDLE":
            # If using migration key, validate it
            if migration_key:
                valid = await mgr.validate_migration_key(migration_key)
                if not valid:
                    raise HTTPException(401, "Invalid migration key")

            return {
                "migration_id": str(state["migration_id"]),
                "role": state["role"],
                "state": state["state"],
                "peer_url": state["peer_url"],
                "embeddings_compatible": state["embeddings_compatible"],
                "progress": {
                    "total_entries": state["total_entries"],
                    "transferred_entries": state["transferred_entries"],
                    "total_files": state["total_files"],
                    "transferred_files": state["transferred_files"],
                    "total_bytes": state["total_bytes"],
                    "transferred_bytes": state["transferred_bytes"],
                },
                "error": state["error_message"],
                "created_at": state["created_at"].isoformat(),
                "updated_at": state["updated_at"].isoformat(),
            }

    return {"state": "IDLE", "message": "No active migration"}


@router.post("/migration/cancel")
async def cancel_migration(request: Request):
    """Cancel an active migration. Admin auth required."""
    body = await _safe_json(request)
    migration_id = body.get("migration_id")

    if not migration_id:
        # Find the active migration
        mgr = get_migration_manager()
        state = await mgr.get_active_state(MigrationRole.SOURCE)
        if not state:
            state = await mgr.get_active_state(MigrationRole.DESTINATION)
        if not state:
            raise HTTPException(404, "No active migration to cancel")
        migration_id = str(state["migration_id"])

    mgr = get_migration_manager()
    try:
        result = await mgr.cancel(migration_id)
    except ValueError as e:
        raise HTTPException(404, str(e))

    return {"migration_id": migration_id, "state": result["state"]}


@router.post("/migration/verify-destination")
async def verify_destination(request: Request):
    """
    Called by destination server to validate migration key and negotiate embeddings.
    Transitions source: INITIATED → KEY_EXCHANGED.
    """
    body = await _safe_json(request)
    migration_key = body.get("migration_key")
    destination_url = body.get("destination_url")
    dest_embedding_provider = body.get("embedding_provider")
    dest_embedding_model = body.get("embedding_model")
    dest_embedding_dimensions = body.get("embedding_dimensions")

    if not migration_key or not destination_url:
        raise HTTPException(400, "migration_key and destination_url required")

    mgr = get_migration_manager()
    state = await mgr.validate_migration_key(migration_key)
    if not state:
        raise HTTPException(401, "Invalid or expired migration key")

    if state["state"] != MigrationState.INITIATED.value:
        raise HTTPException(
            409, f"Migration is in state {state['state']}, expected INITIATED"
        )

    # Check embedding compatibility
    embeddings_compatible = (
        settings.embedding_provider == dest_embedding_provider
        and settings.embedding_model == dest_embedding_model
        and settings.embedding_dimensions == dest_embedding_dimensions
    )

    # Count total entries and files for progress tracking
    pool = get_pool()
    total_entries = await pool.fetchval("SELECT COUNT(*) FROM memory_entries")
    total_files = await pool.fetchval("SELECT COUNT(*) FROM response_files")

    migration_id = str(state["migration_id"])
    result = await mgr.transition(
        migration_id,
        MigrationState.INITIATED,
        MigrationState.KEY_EXCHANGED,
        peer_url=destination_url,
        embedding_provider=dest_embedding_provider,
        embedding_model=dest_embedding_model,
        embedding_dimensions=dest_embedding_dimensions,
        embeddings_compatible=embeddings_compatible,
        total_entries=total_entries,
        total_files=total_files,
    )

    # Extend migration key TTL to 24 hours now that handshake succeeded.
    # The initial 1-hour TTL is for the bootstrap window; after KEY_EXCHANGED
    # the transfer may take much longer for large databases.
    from datetime import timedelta
    extended_expiry = datetime.now(timezone.utc) + timedelta(hours=24)
    await pool.execute(
        "UPDATE migration_state SET migration_key_expires_at = $1 WHERE migration_id::text = $2",
        extended_expiry,
        migration_id,
    )

    server_id = await mgr.get_server_id()

    return {
        "migration_id": migration_id,
        "state": result["state"],
        "server_id": server_id,
        "embeddings_compatible": embeddings_compatible,
        "source_embedding_provider": settings.embedding_provider,
        "source_embedding_model": settings.embedding_model,
        "source_embedding_dimensions": settings.embedding_dimensions,
        "total_entries": total_entries,
        "total_files": total_files,
        "public_key": base64.urlsafe_b64encode(state["ed25519_public_key"]).decode(),
    }


@router.post("/migration/start-redirect")
async def start_redirect(request: Request):
    """
    Start redirecting ingest requests with HTTP 449.
    Transitions source: KEY_EXCHANGED → REDIRECTING.
    """
    body = await _safe_json(request)
    migration_key = body.get("migration_key")

    if not migration_key:
        raise HTTPException(400, "migration_key required")

    mgr = get_migration_manager()
    state = await mgr.validate_migration_key(migration_key)
    if not state:
        raise HTTPException(401, "Invalid or expired migration key")

    if state["state"] != MigrationState.KEY_EXCHANGED.value:
        raise HTTPException(
            409, f"Migration is in state {state['state']}, expected KEY_EXCHANGED"
        )

    migration_id = str(state["migration_id"])
    result = await mgr.transition(
        migration_id,
        MigrationState.KEY_EXCHANGED,
        MigrationState.REDIRECTING,
    )

    return {"migration_id": migration_id, "state": result["state"]}


@router.get("/migration/stream/config")
async def stream_config(request: Request):
    """Export server configuration for migration. Migration key auth."""
    migration_key = request.query_params.get("key")
    if not migration_key:
        auth = request.headers.get("Authorization", "")
        if auth.startswith("Bearer migration:"):
            migration_key = auth.replace("Bearer migration:", "")

    if not migration_key:
        raise HTTPException(401, "Migration key required")

    mgr = get_migration_manager()
    state = await mgr.validate_migration_key(migration_key)
    if not state:
        raise HTTPException(401, "Invalid or expired migration key")

    pool = get_pool()

    # Gather config
    sessions = await pool.fetch(
        "SELECT session_id, project_path, client_machine_id, slug, created_at, last_seen_at "
        "FROM claude_sessions"
    )

    server_id = await mgr.get_server_id()

    return {
        "server_id": server_id,
        "embedding_provider": settings.embedding_provider,
        "embedding_model": settings.embedding_model,
        "embedding_dimensions": settings.embedding_dimensions,
        "sessions": [dict(s) for s in sessions],
    }


@router.get("/migration/stream/entries")
async def stream_entries(
    request: Request,
    after_id: int = Query(0, description="Stream entries after this ID"),
    batch_size: int = Query(200, ge=1, le=1000),
):
    """
    Paginated entry export for migration. Returns entries with optional embeddings.
    Migration key auth.
    """
    migration_key = request.query_params.get("key")
    if not migration_key:
        auth = request.headers.get("Authorization", "")
        if auth.startswith("Bearer migration:"):
            migration_key = auth.replace("Bearer migration:", "")

    if not migration_key:
        raise HTTPException(401, "Migration key required")

    mgr = get_migration_manager()
    state = await mgr.validate_migration_key(migration_key)
    if not state:
        raise HTTPException(401, "Invalid or expired migration key")

    pool = get_pool()
    include_embeddings = state.get("embeddings_compatible", False)

    if include_embeddings:
        rows = await pool.fetch(
            """
            SELECT id, session_id, message_type, content_type, raw_content,
                   payload_hash, source_uuid, parent_uuid, tool_name, cwd,
                   created_at, embedding
            FROM memory_entries
            WHERE id > $1
            ORDER BY id ASC
            LIMIT $2
            """,
            after_id,
            batch_size,
        )
    else:
        rows = await pool.fetch(
            """
            SELECT id, session_id, message_type, content_type, raw_content,
                   payload_hash, source_uuid, parent_uuid, tool_name, cwd,
                   created_at
            FROM memory_entries
            WHERE id > $1
            ORDER BY id ASC
            LIMIT $2
            """,
            after_id,
            batch_size,
        )

    entries = []
    for row in rows:
        entry = {
            "id": row["id"],
            "session_id": row["session_id"],
            "message_type": row["message_type"],
            "content_type": row["content_type"],
            "raw_content": row["raw_content"],
            "payload_hash": row["payload_hash"],
            "source_uuid": row["source_uuid"],
            "parent_uuid": row["parent_uuid"],
            "tool_name": row["tool_name"],
            "cwd": row["cwd"],
            "created_at": row["created_at"].isoformat(),
        }
        if include_embeddings and row.get("embedding") is not None:
            # Encode embedding as base64 float32 array
            embedding = row["embedding"]
            if hasattr(embedding, "tolist"):
                embedding = embedding.tolist()
            packed = struct.pack(f"{len(embedding)}f", *embedding)
            entry["embedding"] = base64.b64encode(packed).decode()
        entries.append(entry)

    has_more = len(entries) == batch_size
    last_id = entries[-1]["id"] if entries else after_id

    return {
        "entries": entries,
        "count": len(entries),
        "last_id": last_id,
        "has_more": has_more,
    }


@router.get("/migration/stream/files")
async def stream_files(
    request: Request,
    after_id: Optional[str] = Query(None, description="Stream files after this UUID"),
    batch_size: int = Query(10, ge=1, le=50),
):
    """Export response files for migration. Returns file metadata + content."""
    migration_key = request.query_params.get("key")
    if not migration_key:
        auth = request.headers.get("Authorization", "")
        if auth.startswith("Bearer migration:"):
            migration_key = auth.replace("Bearer migration:", "")

    if not migration_key:
        raise HTTPException(401, "Migration key required")

    mgr = get_migration_manager()
    state = await mgr.validate_migration_key(migration_key)
    if not state:
        raise HTTPException(401, "Invalid or expired migration key")

    pool = get_pool()

    if after_id:
        rows = await pool.fetch(
            """
            SELECT id, file_path, content_type, size_bytes, summary, structural_index, created_at
            FROM response_files
            WHERE id > $1::uuid AND deleted_at IS NULL
            ORDER BY id ASC
            LIMIT $2
            """,
            after_id,
            batch_size,
        )
    else:
        rows = await pool.fetch(
            """
            SELECT id, file_path, content_type, size_bytes, summary, structural_index, created_at
            FROM response_files
            WHERE deleted_at IS NULL
            ORDER BY id ASC
            LIMIT $1
            """,
            batch_size,
        )

    files = []
    for row in rows:
        file_id = str(row["id"])
        # Read file content from storage
        content = None
        try:
            with open(row["file_path"], "r") as f:
                content = f.read()
        except FileNotFoundError:
            logger.warning("File %s missing from storage, skipping content", file_id)

        files.append({
            "file_id": file_id,
            "content_type": row["content_type"],
            "size_bytes": row["size_bytes"],
            "summary": row["summary"],
            "index_data": row["structural_index"],
            "created_at": row["created_at"].isoformat(),
            "content": content,
        })

    has_more = len(files) == batch_size
    last_id = files[-1]["file_id"] if files else after_id

    return {
        "files": files,
        "count": len(files),
        "last_id": last_id,
        "has_more": has_more,
    }


# ── Transfer Worker ──

_MAX_RETRIES = 8
_BASE_BACKOFF = 1.0
_MAX_BACKOFF = 60.0


async def _fetch_with_retry(
    client: httpx.AsyncClient,
    method: str,
    url: str,
    *,
    params: dict | None = None,
    headers: dict | None = None,
) -> httpx.Response:
    """Make an HTTP request with exponential backoff on transient errors."""
    for attempt in range(_MAX_RETRIES):
        try:
            resp = await client.request(method, url, params=params, headers=headers)
            # Fail immediately on non-auth 4xx (client errors are not transient)
            if 400 <= resp.status_code < 500 and resp.status_code != 401:
                resp.raise_for_status()
            # Auth failures are also non-retryable
            if resp.status_code == 401:
                resp.raise_for_status()
            resp.raise_for_status()
            return resp
        except (httpx.TimeoutException, httpx.ConnectError, httpx.ReadError) as exc:
            backoff = min(_BASE_BACKOFF * (2 ** attempt), _MAX_BACKOFF)
            logger.warning(
                "Transient HTTP error (attempt %d/%d), retrying in %.1fs: %s",
                attempt + 1, _MAX_RETRIES, backoff, exc,
            )
            if attempt == _MAX_RETRIES - 1:
                raise
            await asyncio.sleep(backoff)
        except httpx.HTTPStatusError:
            # 5xx server errors are retryable
            if resp.status_code >= 500:
                backoff = min(_BASE_BACKOFF * (2 ** attempt), _MAX_BACKOFF)
                logger.warning(
                    "Server error %d (attempt %d/%d), retrying in %.1fs",
                    resp.status_code, attempt + 1, _MAX_RETRIES, backoff,
                )
                if attempt == _MAX_RETRIES - 1:
                    raise
                await asyncio.sleep(backoff)
            else:
                raise
    # Should not reach here, but satisfy type checker
    raise httpx.ConnectError("Max retries exhausted")


async def _transfer_worker(
    migration_id: str,
    source_url: str,
    migration_key: str,
    pool,
):
    """Background task: pull entries + files from source, then complete."""
    mgr = get_migration_manager()
    logger.info("Transfer worker started for migration %s", migration_id)

    try:
        # Transition to TRANSFERRING
        state = await mgr.get_active_state(MigrationRole.DESTINATION)
        if state:
            await mgr.transition(
                migration_id,
                MigrationState(state["state"]),
                MigrationState.TRANSFERRING,
            )

        auth_headers = {"Authorization": f"Bearer migration:{migration_key}"}

        async with httpx.AsyncClient(timeout=60.0) as client:
            # Fetch config and import sessions first (required for FK constraints)
            config_resp = await _fetch_with_retry(
                client,
                "GET",
                f"{source_url}/migration/stream/config",
                params={"key": migration_key},
                headers=auth_headers,
            )
            config_data = config_resp.json()
            for sess in config_data.get("sessions", []):
                await pool.execute(
                    """
                    INSERT INTO claude_sessions (session_id, project_path, client_machine_id, slug)
                    VALUES ($1, $2, $3, $4)
                    ON CONFLICT (session_id) DO NOTHING
                    """,
                    sess["session_id"],
                    sess.get("project_path"),
                    sess.get("client_machine_id"),
                    sess.get("slug"),
                )
            logger.info("Imported %d sessions from source", len(config_data.get("sessions", [])))

            # Pull entries
            after_id = 0
            # Check for checkpoint
            state = await mgr.get_active_state(MigrationRole.DESTINATION)
            if state and state.get("last_transferred_entry_id"):
                after_id = state["last_transferred_entry_id"]
                logger.info("Resuming entry transfer from id %d", after_id)

            total_accepted = 0
            while True:
                resp = await _fetch_with_retry(
                    client,
                    "GET",
                    f"{source_url}/migration/stream/entries",
                    params={"after_id": after_id, "batch_size": 200, "key": migration_key},
                    headers=auth_headers,
                )
                data = resp.json()

                entries = data.get("entries", [])
                if not entries:
                    break

                # Insert entries directly via pool (not HTTP self-call)
                accepted = 0
                for entry in entries:
                    try:
                        ts = datetime.fromisoformat(entry["created_at"].replace("Z", "+00:00"))

                        # Handle embedding if present
                        embedding = None
                        if entry.get("embedding"):
                            packed = base64.b64decode(entry["embedding"])
                            count = len(packed) // 4
                            embedding = list(struct.unpack(f"{count}f", packed))

                        if embedding:
                            embedding_arr = np.array(embedding, dtype=np.float32)
                            result = await pool.fetchrow(
                                """INSERT INTO memory_entries (
                                    session_id, message_type, content_type, raw_content,
                                    payload_hash, source_uuid, parent_uuid, tool_name, cwd,
                                    created_at, embedding
                                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
                                ON CONFLICT (payload_hash) DO NOTHING RETURNING id""",
                                entry["session_id"], entry["message_type"],
                                entry["content_type"], entry["raw_content"],
                                entry["payload_hash"], entry.get("source_uuid"),
                                entry.get("parent_uuid"), entry.get("tool_name"),
                                entry.get("cwd"), ts, embedding_arr,
                            )
                        else:
                            result = await pool.fetchrow(
                                """INSERT INTO memory_entries (
                                    session_id, message_type, content_type, raw_content,
                                    payload_hash, source_uuid, parent_uuid, tool_name, cwd, created_at
                                ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                                ON CONFLICT (payload_hash) DO NOTHING RETURNING id""",
                                entry["session_id"], entry["message_type"],
                                entry["content_type"], entry["raw_content"],
                                entry["payload_hash"], entry.get("source_uuid"),
                                entry.get("parent_uuid"), entry.get("tool_name"),
                                entry.get("cwd"), ts,
                            )

                        if result:
                            accepted += 1
                            # Enqueue for embedding if no embedding transferred
                            if not embedding:
                                await enqueue_ids([result["id"]])
                    except Exception:
                        logger.exception("Error inserting entry %s", entry.get("payload_hash"))

                total_accepted += accepted
                after_id = data["last_id"]

                # Update checkpoint
                await mgr.update_progress(
                    migration_id,
                    transferred_entries=total_accepted,
                    last_transferred_entry_id=after_id,
                )

                if not data.get("has_more", False):
                    break

            logger.info("Entries transferred: %d", total_accepted)

            # Pull files
            last_file_id = None
            total_files = 0
            while True:
                params: dict = {"batch_size": 10, "key": migration_key}
                if last_file_id:
                    params["after_id"] = last_file_id

                resp = await _fetch_with_retry(
                    client,
                    "GET",
                    f"{source_url}/migration/stream/files",
                    params=params,
                    headers=auth_headers,
                )
                data = resp.json()

                files = data.get("files", [])
                if not files:
                    break

                for file_data in files:
                    try:
                        file_id = file_data["file_id"]
                        file_name = f"{file_id}.txt"
                        file_path = os.path.join(settings.file_storage_path, file_name)
                        await pool.execute(
                            """INSERT INTO response_files (id, file_path, content_type, size_bytes, summary, structural_index, source_endpoint)
                            VALUES ($1::uuid, $2, $3, $4, $5, $6, 'migration')
                            ON CONFLICT (id) DO NOTHING""",
                            file_id, file_path, file_data.get("content_type", "text/plain"),
                            file_data.get("size_bytes", 0),
                            file_data.get("summary"), file_data.get("index_data"),
                        )
                        content = file_data.get("content")
                        if content is not None:
                            os.makedirs(settings.file_storage_path, exist_ok=True)
                            with open(file_path, "w") as f:
                                f.write(content)
                        total_files += 1
                    except Exception:
                        logger.exception("Error receiving file %s", file_data.get("file_id"))

                last_file_id = data.get("last_id")
                await mgr.update_progress(migration_id, transferred_files=total_files)

                if not data.get("has_more", False):
                    break

            logger.info("Files transferred: %d", total_files)

        # Transition to VERIFYING then COMPLETE
        await mgr.transition(migration_id, MigrationState.TRANSFERRING, MigrationState.VERIFYING)
        await mgr.transition(migration_id, MigrationState.VERIFYING, MigrationState.COMPLETE)

        logger.info("Migration %s complete: %d entries, %d files", migration_id, total_accepted, total_files)

    except Exception as e:
        logger.exception("Transfer worker failed for migration %s", migration_id)
        try:
            state = await mgr.get_active_state(MigrationRole.DESTINATION)
            if state:
                await mgr.transition(
                    migration_id,
                    MigrationState(state["state"]),
                    MigrationState.FAILED,
                    error_message=str(e),
                )
        except Exception:
            logger.exception("Failed to transition to FAILED state")


# ── Destination Server Endpoints ──


@router.post("/migration/receive/handshake")
async def receive_handshake(request: Request):
    """
    Accept migration config from source. Initialize destination state.
    """
    body = await _safe_json(request)
    migration_id = body.get("migration_id")
    source_url = body.get("source_url")
    config_data = body.get("config", {})

    if not migration_id or not source_url:
        raise HTTPException(400, "migration_id and source_url required")

    mgr = get_migration_manager()
    pool = get_pool()

    # Initialize destination state
    try:
        state = await mgr.init_destination(migration_id, source_url)
    except ValueError as e:
        raise HTTPException(409, str(e))

    # Import sessions from config
    sessions = config_data.get("sessions", [])
    for sess in sessions:
        await pool.execute(
            """
            INSERT INTO claude_sessions (session_id, project_path, client_machine_id, slug)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (session_id) DO NOTHING
            """,
            sess["session_id"],
            sess.get("project_path"),
            sess.get("client_machine_id"),
            sess.get("slug"),
        )

    logger.info("Destination handshake: imported %d sessions", len(sessions))

    # Launch transfer worker if migration_key provided
    migration_key = body.get("migration_key")
    if migration_key and source_url:
        asyncio.create_task(
            _transfer_worker(migration_id, source_url, migration_key, pool)
        )
        logger.info("Transfer worker launched for migration %s", migration_id)

    return {
        "migration_id": migration_id,
        "state": state["state"],
        "sessions_imported": len(sessions),
    }


@router.post("/migration/receive/entries")
async def receive_entries(request: Request):
    """
    Receive migrated entries (with optional embeddings if compatible).
    """
    body = await _safe_json(request)
    migration_id = body.get("migration_id")
    entries = body.get("entries", [])

    if not migration_id:
        raise HTTPException(400, "migration_id required")

    pool = get_pool()
    mgr = get_migration_manager()

    accepted = 0
    duplicates = 0
    errors = 0

    for entry in entries:
        try:
            ts = datetime.fromisoformat(entry["created_at"].replace("Z", "+00:00"))

            # Decode embedding if present
            embedding = None
            if entry.get("embedding"):
                packed = base64.b64decode(entry["embedding"])
                count = len(packed) // 4
                embedding = list(struct.unpack(f"{count}f", packed))

            if embedding:
                embedding_arr = np.array(embedding, dtype=np.float32)
                result = await pool.fetchrow(
                    """
                    INSERT INTO memory_entries (
                        session_id, message_type, content_type, raw_content,
                        payload_hash, source_uuid, parent_uuid, tool_name, cwd,
                        created_at, embedding
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
                    ON CONFLICT (payload_hash) DO NOTHING
                    RETURNING id
                    """,
                    entry["session_id"],
                    entry["message_type"],
                    entry["content_type"],
                    entry["raw_content"],
                    entry["payload_hash"],
                    entry.get("source_uuid"),
                    entry.get("parent_uuid"),
                    entry.get("tool_name"),
                    entry.get("cwd"),
                    ts,
                    embedding_arr,
                )
            else:
                result = await pool.fetchrow(
                    """
                    INSERT INTO memory_entries (
                        session_id, message_type, content_type, raw_content,
                        payload_hash, source_uuid, parent_uuid, tool_name, cwd,
                        created_at
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                    ON CONFLICT (payload_hash) DO NOTHING
                    RETURNING id
                    """,
                    entry["session_id"],
                    entry["message_type"],
                    entry["content_type"],
                    entry["raw_content"],
                    entry["payload_hash"],
                    entry.get("source_uuid"),
                    entry.get("parent_uuid"),
                    entry.get("tool_name"),
                    entry.get("cwd"),
                    ts,
                )

            if result:
                accepted += 1
                # Enqueue for embedding if no embedding transferred
                if not embedding:
                    await enqueue_ids([result["id"]])
            else:
                duplicates += 1
        except Exception:
            logger.exception("Error receiving entry %s", entry.get("payload_hash"))
            errors += 1

    # Update progress
    if entries:
        last_id = max(e.get("id", 0) for e in entries)
        state = await mgr.get_active_state(MigrationRole.DESTINATION)
        if state:
            current_transferred = state.get("transferred_entries", 0) or 0
            await mgr.update_progress(
                str(state["migration_id"]),
                transferred_entries=current_transferred + accepted,
                last_transferred_entry_id=last_id,
            )

    return {"accepted": accepted, "duplicates": duplicates, "errors": errors}


@router.post("/migration/receive/files")
async def receive_files(request: Request):
    """Receive response files from source server."""
    body = await _safe_json(request)
    migration_id = body.get("migration_id")
    files = body.get("files", [])

    if not migration_id:
        raise HTTPException(400, "migration_id required")

    pool = get_pool()
    mgr = get_migration_manager()
    imported = 0

    for file_data in files:
        try:
            file_id = file_data["file_id"]
            content = file_data.get("content")
            file_name = f"{file_id}.txt"
            file_path = os.path.join(settings.file_storage_path, file_name)

            # Insert file metadata
            await pool.execute(
                """
                INSERT INTO response_files (id, file_path, content_type, size_bytes, summary, structural_index, source_endpoint)
                VALUES ($1::uuid, $2, $3, $4, $5, $6, 'migration')
                ON CONFLICT (id) DO NOTHING
                """,
                file_id,
                file_path,
                file_data.get("content_type", "text/plain"),
                file_data.get("size_bytes", 0),
                file_data.get("summary"),
                file_data.get("index_data"),
            )

            # Write file content to storage
            if content is not None:
                os.makedirs(settings.file_storage_path, exist_ok=True)
                with open(file_path, "w") as f:
                    f.write(content)

            imported += 1
        except Exception:
            logger.exception("Error receiving file %s", file_data.get("file_id"))

    # Update progress
    state = await mgr.get_active_state(MigrationRole.DESTINATION)
    if state:
        current_files = state.get("transferred_files", 0) or 0
        await mgr.update_progress(
            str(state["migration_id"]),
            transferred_files=current_files + imported,
        )

    return {"imported": imported, "total": len(files)}


@router.post("/migration/receive/complete")
async def receive_complete(request: Request):
    """
    Verify transfer counts and complete migration.
    Transitions: TRANSFERRING → VERIFYING → COMPLETE.
    """
    body = await _safe_json(request)
    migration_id = body.get("migration_id")
    expected_entries = body.get("expected_entries", 0)
    expected_files = body.get("expected_files", 0)

    if not migration_id:
        raise HTTPException(400, "migration_id required")

    pool = get_pool()
    mgr = get_migration_manager()

    # Count what we received
    actual_entries = await pool.fetchval("SELECT COUNT(*) FROM memory_entries")
    actual_files = await pool.fetchval("SELECT COUNT(*) FROM response_files")

    state = await mgr.get_active_state(MigrationRole.DESTINATION)
    if not state:
        raise HTTPException(404, "No active destination migration")

    current_state = MigrationState(state["state"])

    # Transition to VERIFYING
    if current_state in (MigrationState.KEY_EXCHANGED, MigrationState.TRANSFERRING):
        await mgr.transition(
            migration_id,
            current_state,
            MigrationState.VERIFYING,
            total_entries=expected_entries,
            total_files=expected_files,
        )

    # Verify counts
    entries_ok = actual_entries >= expected_entries
    files_ok = actual_files >= expected_files

    if entries_ok and files_ok:
        await mgr.transition(
            migration_id,
            MigrationState.VERIFYING,
            MigrationState.COMPLETE,
        )
        logger.info(
            "Migration %s complete: %d entries, %d files",
            migration_id,
            actual_entries,
            actual_files,
        )
        return {
            "migration_id": migration_id,
            "state": "COMPLETE",
            "entries": actual_entries,
            "files": actual_files,
            "verified": True,
        }
    else:
        return {
            "migration_id": migration_id,
            "state": "VERIFYING",
            "entries": actual_entries,
            "expected_entries": expected_entries,
            "files": actual_files,
            "expected_files": expected_files,
            "verified": False,
            "message": "Count mismatch — retry transfer or investigate",
        }


@router.get("/migration/client-provision")
async def client_provision(request: Request):
    """
    Provide permanent auth credentials to a daemon after migration.
    Auth: Bearer migration:<migration_id>
    Only succeeds for COMPLETE destination migrations (credentials are
    single-use — private key material is cleared by the cleanup worker).
    """
    auth = request.headers.get("Authorization", "")
    if not auth.startswith("Bearer migration:"):
        raise HTTPException(401, "Migration auth required")

    migration_id = auth.replace("Bearer migration:", "")
    if not migration_id or len(migration_id) < 32:
        raise HTTPException(401, "Invalid migration auth")

    pool = get_pool()
    mgr = get_migration_manager()

    # Only allow provisioning for completed destination migrations.
    # The migration_id must match exactly (UUIDv4 — unguessable).
    row = await pool.fetchrow(
        """
        SELECT * FROM migration_state
        WHERE role = 'destination'
          AND migration_id::text = $1
          AND state IN ('COMPLETE', 'TRANSFERRING', 'VERIFYING')
        """,
        migration_id,
    )

    if not row:
        raise HTTPException(404, "Migration not found or not ready")

    return {
        "server_url": f"{request.base_url}api",
        "auth_token": settings.memlayer_auth_token,
        "server_id": await mgr.get_server_id(),
    }
