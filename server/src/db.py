import logging

import asyncpg
from pgvector.asyncpg import register_vector

from .config import settings

logger = logging.getLogger(__name__)

pool: asyncpg.Pool | None = None


async def init_pool() -> asyncpg.Pool:
    global pool

    # Detect if extensions are in a non-public schema (e.g. Supabase
    # installs extensions in the 'extensions' schema). We probe once and
    # set the search_path accordingly so migrations and queries work.
    probe = await asyncpg.connect(settings.database_url)
    try:
        vector_schema = await probe.fetchval("""
            SELECT nspname FROM pg_extension e
            JOIN pg_namespace n ON n.oid = e.extnamespace
            WHERE e.extname = 'vector'
        """)
        trgm_schema = await probe.fetchval("""
            SELECT nspname FROM pg_extension e
            JOIN pg_namespace n ON n.oid = e.extnamespace
            WHERE e.extname = 'pg_trgm'
        """)
    finally:
        await probe.close()

    extra_schemas = []
    if vector_schema and vector_schema not in ("public", "pg_catalog"):
        logger.info(f"vector extension found in '{vector_schema}' schema, adding to search_path")
        extra_schemas.append(vector_schema)
    if trgm_schema and trgm_schema not in ("public", "pg_catalog") and trgm_schema not in extra_schemas:
        logger.info(f"pg_trgm extension found in '{trgm_schema}' schema, adding to search_path")
        extra_schemas.append(trgm_schema)

    search_path = ", ".join(["public"] + extra_schemas)

    async def _init_connection(conn):
        if extra_schemas:
            await conn.execute(f"SET search_path TO {search_path}")
        await register_vector(conn, schema=vector_schema or "public")

    try:
        pool = await asyncpg.create_pool(
            settings.database_url,
            min_size=2,
            max_size=10,
            init=_init_connection,
            server_settings={"search_path": search_path} if extra_schemas else None,
        )
    except RuntimeError:
        raise
    except Exception as e:
        raise RuntimeError(
            f"Failed to connect to database: {e}. "
            "Check your DATABASE_URL and ensure the database is accessible."
        ) from e
    return pool


async def close_pool():
    global pool
    if pool:
        await pool.close()
        pool = None


def get_pool() -> asyncpg.Pool:
    assert pool is not None, "Database pool not initialized"
    return pool
