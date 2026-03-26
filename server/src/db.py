import logging

import asyncpg
from pgvector.asyncpg import register_vector

from .config import settings

logger = logging.getLogger(__name__)

pool: asyncpg.Pool | None = None


async def init_pool() -> asyncpg.Pool:
    global pool

    async def _init_connection(conn):
        try:
            await register_vector(conn)
        except ValueError as e:
            if "unknown type" in str(e) and "vector" in str(e):
                # Supabase installs extensions in the 'extensions' schema
                logger.info("vector type not in public schema, trying extensions schema (Supabase)")
                await register_vector(conn, schema="extensions")
            else:
                raise

    try:
        pool = await asyncpg.create_pool(
            settings.database_url,
            min_size=2,
            max_size=10,
            init=_init_connection,
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
