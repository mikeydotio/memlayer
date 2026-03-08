import asyncpg
from pgvector.asyncpg import register_vector

from .config import settings

pool: asyncpg.Pool | None = None


async def init_pool() -> asyncpg.Pool:
    global pool

    async def _init_connection(conn):
        await register_vector(conn)

    pool = await asyncpg.create_pool(
        settings.database_url,
        min_size=2,
        max_size=10,
        init=_init_connection,
    )
    return pool


async def close_pool():
    global pool
    if pool:
        await pool.close()
        pool = None


def get_pool() -> asyncpg.Pool:
    assert pool is not None, "Database pool not initialized"
    return pool
