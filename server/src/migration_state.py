import base64
import hashlib
import logging
import os
from datetime import datetime, timezone, timedelta
from enum import Enum
from typing import Optional

from cryptography.hazmat.primitives.asymmetric.ed25519 import (
    Ed25519PrivateKey,
    Ed25519PublicKey,
)
from cryptography.hazmat.primitives.serialization import (
    Encoding,
    NoEncryption,
    PrivateFormat,
    PublicFormat,
)

from .db import get_pool

logger = logging.getLogger(__name__)


class MigrationRole(str, Enum):
    SOURCE = "source"
    DESTINATION = "destination"


class MigrationState(str, Enum):
    IDLE = "IDLE"
    INITIATED = "INITIATED"
    KEY_EXCHANGED = "KEY_EXCHANGED"
    REDIRECTING = "REDIRECTING"
    DRAINING = "DRAINING"
    TRANSFERRING = "TRANSFERRING"
    VERIFYING = "VERIFYING"
    COMPLETE = "COMPLETE"
    FAILED = "FAILED"


# Valid state transitions
VALID_TRANSITIONS = {
    MigrationState.IDLE: {MigrationState.INITIATED},
    MigrationState.INITIATED: {MigrationState.KEY_EXCHANGED, MigrationState.FAILED, MigrationState.IDLE},
    MigrationState.KEY_EXCHANGED: {MigrationState.REDIRECTING, MigrationState.TRANSFERRING, MigrationState.FAILED, MigrationState.IDLE},
    MigrationState.REDIRECTING: {MigrationState.DRAINING, MigrationState.FAILED, MigrationState.IDLE},
    MigrationState.DRAINING: {MigrationState.TRANSFERRING, MigrationState.FAILED, MigrationState.IDLE},
    MigrationState.TRANSFERRING: {MigrationState.VERIFYING, MigrationState.FAILED, MigrationState.IDLE},
    MigrationState.VERIFYING: {MigrationState.COMPLETE, MigrationState.FAILED, MigrationState.IDLE},
    MigrationState.COMPLETE: {MigrationState.IDLE},
    MigrationState.FAILED: {MigrationState.IDLE},
}


class MigrationManager:
    """Manages server-to-server migration state, keys, and progress."""

    def __init__(self, key_ttl_secs: int = 3600):
        self.key_ttl_secs = key_ttl_secs

    async def get_state(self, role: MigrationRole) -> Optional[dict]:
        """Get current migration state for the given role."""
        pool = get_pool()
        row = await pool.fetchrow(
            """
            SELECT * FROM migration_state
            WHERE role = $1
            ORDER BY created_at DESC
            LIMIT 1
            """,
            role.value,
        )
        return dict(row) if row else None

    async def get_active_state(self, role: MigrationRole) -> Optional[dict]:
        """Get the active (non-terminal) migration state."""
        pool = get_pool()
        row = await pool.fetchrow(
            """
            SELECT * FROM migration_state
            WHERE role = $1 AND state NOT IN ('IDLE', 'COMPLETE', 'FAILED')
            ORDER BY created_at DESC
            LIMIT 1
            """,
            role.value,
        )
        return dict(row) if row else None

    async def _transition(
        self, migration_id: str, from_state: MigrationState, to_state: MigrationState, **kwargs
    ) -> dict:
        """Transition migration state with validation."""
        if to_state not in VALID_TRANSITIONS.get(from_state, set()):
            raise ValueError(
                f"Invalid state transition: {from_state.value} → {to_state.value}"
            )

        pool = get_pool()
        set_clauses = ["state = $2", "updated_at = NOW()"]
        params = [str(migration_id), to_state.value]
        param_idx = 3

        if to_state == MigrationState.COMPLETE:
            set_clauses.append("completed_at = NOW()")

        if to_state == MigrationState.FAILED and "error_message" in kwargs:
            set_clauses.append(f"error_message = ${param_idx}")
            params.append(kwargs["error_message"])
            param_idx += 1
            set_clauses.append("error_at = NOW()")

        for key in [
            "peer_url", "embedding_provider", "embedding_model",
            "embedding_dimensions", "embeddings_compatible",
            "total_entries", "transferred_entries", "last_transferred_entry_id",
            "total_files", "transferred_files", "total_bytes", "transferred_bytes",
        ]:
            if key in kwargs:
                set_clauses.append(f"{key} = ${param_idx}")
                params.append(kwargs[key])
                param_idx += 1

        sql = f"""
            UPDATE migration_state
            SET {', '.join(set_clauses)}
            WHERE migration_id::text = $1 AND state = '{from_state.value}'
            RETURNING *
        """
        row = await pool.fetchrow(sql, *params)
        if not row:
            raise ValueError(
                f"State transition failed: migration {migration_id} not in state {from_state.value}"
            )

        logger.info(f"Migration {migration_id}: {from_state.value} → {to_state.value}")
        return dict(row)

    async def initiate(self) -> tuple[dict, str]:
        """
        Initiate a migration as source server.
        Returns (state_row, plaintext_migration_key).
        """
        pool = get_pool()

        # Check for existing active migration
        active = await self.get_active_state(MigrationRole.SOURCE)
        if active:
            raise ValueError(
                f"Active migration already exists in state {active['state']}"
            )

        # Generate migration key
        raw_key = os.urandom(32)
        migration_key = base64.urlsafe_b64encode(raw_key).decode()
        key_hash = hashlib.sha256(migration_key.encode()).hexdigest()

        # Generate Ed25519 keypair
        private_key = Ed25519PrivateKey.generate()
        public_key = private_key.public_key()

        private_bytes = private_key.private_bytes(
            Encoding.Raw, PrivateFormat.Raw, NoEncryption()
        )
        public_bytes = public_key.public_bytes(Encoding.Raw, PublicFormat.Raw)

        expires_at = datetime.now(timezone.utc) + timedelta(seconds=self.key_ttl_secs)

        row = await pool.fetchrow(
            """
            INSERT INTO migration_state (
                role, state, migration_key_hash, migration_key_expires_at,
                ed25519_private_key, ed25519_public_key
            ) VALUES ('source', 'INITIATED', $1, $2, $3, $4)
            RETURNING *
            """,
            key_hash,
            expires_at,
            private_bytes,
            public_bytes,
        )

        logger.info(f"Migration initiated: {row['migration_id']}")
        return dict(row), migration_key

    async def validate_migration_key(self, migration_key: str) -> Optional[dict]:
        """Validate a migration key and return the migration state if valid."""
        pool = get_pool()
        key_hash = hashlib.sha256(migration_key.encode()).hexdigest()

        row = await pool.fetchrow(
            """
            SELECT * FROM migration_state
            WHERE role = 'source'
              AND migration_key_hash = $1
              AND migration_key_expires_at > NOW()
              AND state NOT IN ('IDLE', 'COMPLETE', 'FAILED')
            ORDER BY created_at DESC
            LIMIT 1
            """,
            key_hash,
        )
        return dict(row) if row else None

    async def transition(
        self, migration_id: str, from_state: MigrationState, to_state: MigrationState, **kwargs
    ) -> dict:
        """Public transition method."""
        return await self._transition(migration_id, from_state, to_state, **kwargs)

    async def cancel(self, migration_id: str) -> dict:
        """Cancel an active migration, returning to IDLE."""
        pool = get_pool()
        row = await pool.fetchrow(
            """
            UPDATE migration_state
            SET state = 'IDLE', updated_at = NOW(), error_message = 'Cancelled by admin'
            WHERE migration_id::text = $1
              AND state NOT IN ('IDLE', 'COMPLETE', 'FAILED')
            RETURNING *
            """,
            migration_id,
        )
        if not row:
            raise ValueError(f"No active migration found with id {migration_id}")
        logger.info(f"Migration {migration_id} cancelled")
        return dict(row)

    async def update_progress(
        self,
        migration_id: str,
        transferred_entries: Optional[int] = None,
        last_transferred_entry_id: Optional[int] = None,
        transferred_files: Optional[int] = None,
        transferred_bytes: Optional[int] = None,
    ):
        """Update transfer progress counters."""
        pool = get_pool()
        set_clauses = ["updated_at = NOW()"]
        params = [str(migration_id)]
        param_idx = 2

        if transferred_entries is not None:
            set_clauses.append(f"transferred_entries = ${param_idx}")
            params.append(transferred_entries)
            param_idx += 1
        if last_transferred_entry_id is not None:
            set_clauses.append(f"last_transferred_entry_id = ${param_idx}")
            params.append(last_transferred_entry_id)
            param_idx += 1
        if transferred_files is not None:
            set_clauses.append(f"transferred_files = ${param_idx}")
            params.append(transferred_files)
            param_idx += 1
        if transferred_bytes is not None:
            set_clauses.append(f"transferred_bytes = ${param_idx}")
            params.append(transferred_bytes)
            param_idx += 1

        await pool.execute(
            f"""
            UPDATE migration_state
            SET {', '.join(set_clauses)}
            WHERE migration_id::text = $1
            """,
            *params,
        )

    async def is_redirecting(self) -> bool:
        """Check if the source server is in a redirect state (REDIRECTING or later transfer states)."""
        pool = get_pool()
        row = await pool.fetchval(
            """
            SELECT EXISTS (
                SELECT 1 FROM migration_state
                WHERE role = 'source'
                  AND state IN ('REDIRECTING', 'DRAINING', 'TRANSFERRING', 'VERIFYING')
            )
            """
        )
        return bool(row)

    async def get_redirect_info(self) -> Optional[dict]:
        """Get redirect info (peer URL + signed payload) for 449 responses."""
        pool = get_pool()
        row = await pool.fetchrow(
            """
            SELECT migration_id, peer_url, ed25519_private_key, ed25519_public_key
            FROM migration_state
            WHERE role = 'source'
              AND state IN ('REDIRECTING', 'DRAINING', 'TRANSFERRING', 'VERIFYING')
              AND peer_url IS NOT NULL
            ORDER BY created_at DESC
            LIMIT 1
            """
        )
        return dict(row) if row else None

    async def sign_redirect(self, message: bytes, private_key_bytes: bytes) -> str:
        """Sign a redirect payload with Ed25519."""
        private_key = Ed25519PrivateKey.from_private_bytes(private_key_bytes)
        signature = private_key.sign(message)
        return base64.urlsafe_b64encode(signature).decode()

    async def get_server_id(self) -> str:
        """Get this server's unique identity."""
        pool = get_pool()
        row = await pool.fetchrow("SELECT server_id FROM server_identity LIMIT 1")
        if row:
            return str(row["server_id"])
        # Create one if missing
        row = await pool.fetchrow(
            "INSERT INTO server_identity (server_name) VALUES ('memlayer-server') RETURNING server_id"
        )
        return str(row["server_id"])

    async def cleanup_stale_migrations(self):
        """Clean up expired or stale migration states."""
        pool = get_pool()

        # Mark INITIATED migrations as FAILED if key expired
        result = await pool.execute(
            """
            UPDATE migration_state
            SET state = 'FAILED', error_message = 'Migration key expired', error_at = NOW(), updated_at = NOW()
            WHERE state = 'INITIATED'
              AND migration_key_expires_at < NOW()
            """
        )

        # Clear sensitive key material on terminal states
        await pool.execute(
            """
            UPDATE migration_state
            SET ed25519_private_key = NULL, migration_key_hash = NULL, updated_at = NOW()
            WHERE state IN ('COMPLETE', 'FAILED')
              AND ed25519_private_key IS NOT NULL
            """
        )

        # Mark stale transfers as FAILED (no progress for 24 hours)
        await pool.execute(
            """
            UPDATE migration_state
            SET state = 'FAILED', error_message = 'Migration timed out (24h no progress)', error_at = NOW(), updated_at = NOW()
            WHERE state IN ('TRANSFERRING', 'KEY_EXCHANGED', 'REDIRECTING', 'DRAINING')
              AND updated_at < NOW() - INTERVAL '24 hours'
            """
        )

    async def init_destination(self, migration_id: str, peer_url: str) -> dict:
        """Initialize this server as migration destination."""
        pool = get_pool()

        active = await self.get_active_state(MigrationRole.DESTINATION)
        if active:
            raise ValueError(
                f"Active destination migration already exists in state {active['state']}"
            )

        row = await pool.fetchrow(
            """
            INSERT INTO migration_state (
                role, state, migration_id, peer_url
            ) VALUES ('destination', 'KEY_EXCHANGED', $1::uuid, $2)
            RETURNING *
            """,
            migration_id,
            peer_url,
        )
        logger.info(f"Destination initialized for migration {migration_id}")
        return dict(row)


# Module-level singleton
_manager: Optional[MigrationManager] = None


def get_migration_manager() -> MigrationManager:
    global _manager
    if _manager is None:
        from .config import settings
        _manager = MigrationManager(key_ttl_secs=settings.migration_key_ttl_secs)
    return _manager
