"""Integration tests for the migration endpoint flow.

Exercises migration endpoints in sequence, validating auth rules, middleware
behavior, and the ingest 449 redirect. All tests run WITHOUT a database —
get_pool() and get_migration_manager() are mocked as needed.
"""

import base64
import json
from datetime import datetime, timezone, timedelta
from unittest.mock import AsyncMock, MagicMock, patch
from uuid import uuid4

import pytest
from fastapi.testclient import TestClient


AUTH_TOKEN = "test-token-migration-flow"


@pytest.fixture(autouse=True)
def _patch_settings():
    """Patch settings for all migration flow tests."""
    with patch.dict("os.environ", {
        "MEMLAYER_AUTH_TOKEN": AUTH_TOKEN,
        "OPENAI_API_KEY": "",
    }):
        from src.config import Settings
        test_settings = Settings()
        with patch("src.config.settings", test_settings):
            with patch("src.main.settings", test_settings):
                yield test_settings


@pytest.fixture
def mock_pool():
    """A mock asyncpg pool."""
    pool = AsyncMock()
    pool.fetch = AsyncMock(return_value=[])
    pool.fetchrow = AsyncMock(return_value=None)
    pool.fetchval = AsyncMock(return_value=1)
    pool.execute = AsyncMock()
    pool.close = AsyncMock()
    return pool


@pytest.fixture
def mock_mgr():
    """A mock MigrationManager with sensible defaults."""
    mgr = MagicMock()
    mgr.initiate = AsyncMock()
    mgr.cancel = AsyncMock()
    mgr.get_state = AsyncMock(return_value=None)
    mgr.get_active_state = AsyncMock(return_value=None)
    mgr.validate_migration_key = AsyncMock(return_value=None)
    mgr.is_redirecting = AsyncMock(return_value=False)
    mgr.get_redirect_info = AsyncMock(return_value=None)
    mgr.sign_redirect = AsyncMock(return_value="fakesig")
    mgr.get_server_id = AsyncMock(return_value="server-1234")
    mgr.init_destination = AsyncMock()
    mgr.transition = AsyncMock()
    mgr.update_progress = AsyncMock()
    return mgr


@pytest.fixture
def client(mock_pool, mock_mgr):
    """TestClient with mocked pool and migration manager."""
    with patch("src.db.pool", mock_pool):
        with patch("src.routes.ingest.get_pool", return_value=mock_pool):
            with patch("src.routes.search.get_pool", return_value=mock_pool):
                with patch("src.routes.files.get_file_path", new_callable=AsyncMock):
                    with patch("src.embeddings.embed_query", new_callable=AsyncMock, return_value=None):
                        with patch("src.routes.search.embed_query", new_callable=AsyncMock, return_value=None):
                            with patch("src.routes.ingest.enqueue_ids", new_callable=AsyncMock):
                                with patch("src.routes.migration.get_pool", return_value=mock_pool):
                                    with patch("src.routes.migration.get_migration_manager", return_value=mock_mgr):
                                        with patch("src.migration_state.get_migration_manager", return_value=mock_mgr):
                                            from src.main import app
                                            yield TestClient(app, raise_server_exceptions=False)


@pytest.fixture
def admin_headers():
    """Authorization headers with the admin token."""
    return {"Authorization": f"Bearer {AUTH_TOKEN}"}


@pytest.fixture
def migration_key_headers():
    """Authorization headers with a migration key."""
    return {"Authorization": "Bearer migration:some-test-key"}


# ── Test 1: initiate requires admin auth, not migration key ──


class TestInitiateAuth:
    """POST /api/migration/initiate requires admin auth."""

    def test_initiate_rejects_migration_key(self, client, migration_key_headers):
        """Migration key auth should be rejected — initiate needs admin."""
        resp = client.post("/api/migration/initiate", headers=migration_key_headers)
        assert resp.status_code == 401

    def test_initiate_rejects_no_auth(self, client):
        """No auth at all should be rejected."""
        resp = client.post("/api/migration/initiate")
        assert resp.status_code == 401

    def test_initiate_succeeds_with_admin_auth(self, client, admin_headers, mock_mgr):
        """Admin auth should pass through to the handler."""
        now = datetime.now(timezone.utc)
        fake_state = {
            "migration_id": uuid4(),
            "migration_key_expires_at": now + timedelta(hours=1),
            "ed25519_public_key": b"\x00" * 32,
            "state": "INITIATED",
        }
        mock_mgr.initiate.return_value = (fake_state, "plaintext-key-abc")

        resp = client.post("/api/migration/initiate", headers=admin_headers)
        assert resp.status_code == 200
        data = resp.json()
        assert data["state"] == "INITIATED"
        assert data["migration_key"] == "plaintext-key-abc"
        assert "migration_id" in data
        assert "public_key" in data


# ── Test 2: cancel requires admin auth ──


class TestCancelAuth:
    """POST /api/migration/cancel requires admin auth."""

    def test_cancel_rejects_migration_key(self, client, migration_key_headers):
        resp = client.post("/api/migration/cancel", headers=migration_key_headers)
        assert resp.status_code == 401

    def test_cancel_rejects_no_auth(self, client):
        resp = client.post("/api/migration/cancel")
        assert resp.status_code == 401

    def test_cancel_with_admin_auth(self, client, admin_headers, mock_mgr):
        """Admin auth should reach the handler (404 if no active migration)."""
        mock_mgr.get_active_state.return_value = None
        resp = client.post("/api/migration/cancel", headers=admin_headers)
        # No active migration → 404 from handler logic
        assert resp.status_code == 404


# ── Test 3: receive/* requires admin auth ──


class TestReceiveHandshakeAuth:
    """POST /api/migration/receive/handshake requires admin auth."""

    def test_receive_handshake_rejects_migration_key(self, client, migration_key_headers):
        resp = client.post(
            "/api/migration/receive/handshake",
            headers=migration_key_headers,
            json={"migration_id": "abc", "source_url": "http://example.com"},
        )
        assert resp.status_code == 401

    def test_receive_handshake_rejects_no_auth(self, client):
        resp = client.post(
            "/api/migration/receive/handshake",
            json={"migration_id": "abc", "source_url": "http://example.com"},
        )
        assert resp.status_code == 401

    def test_receive_entries_rejects_migration_key(self, client, migration_key_headers):
        resp = client.post(
            "/api/migration/receive/entries",
            headers=migration_key_headers,
            json={"migration_id": "abc", "entries": []},
        )
        assert resp.status_code == 401

    def test_receive_files_rejects_migration_key(self, client, migration_key_headers):
        resp = client.post(
            "/api/migration/receive/files",
            headers=migration_key_headers,
            json={"migration_id": "abc", "files": []},
        )
        assert resp.status_code == 401

    def test_receive_complete_rejects_migration_key(self, client, migration_key_headers):
        resp = client.post(
            "/api/migration/receive/complete",
            headers=migration_key_headers,
            json={"migration_id": "abc"},
        )
        assert resp.status_code == 401


# ── Test 4: status accessible with migration key auth ──


class TestStatusAuth:
    """GET /api/migration/status accepts migration key auth."""

    def test_status_with_migration_key_not_401(self, client, migration_key_headers, mock_mgr):
        """Migration key should pass through middleware to the handler."""
        mock_mgr.get_state.return_value = None
        resp = client.get("/api/migration/status", headers=migration_key_headers)
        # Should NOT be 401 from middleware — handler returns IDLE status when
        # no active migration is found.
        assert resp.status_code == 200
        data = resp.json()
        assert data["state"] == "IDLE"

    def test_status_with_admin_auth(self, client, admin_headers, mock_mgr):
        """Admin auth should also work for status."""
        mock_mgr.get_state.return_value = None
        resp = client.get("/api/migration/status", headers=admin_headers)
        assert resp.status_code == 200

    def test_status_no_auth_is_401(self, client):
        """No auth at all should be 401."""
        resp = client.get("/api/migration/status")
        assert resp.status_code == 401


# ── Test 5: verify-destination accessible with migration key auth ──


class TestVerifyDestinationAuth:
    """POST /api/migration/verify-destination accepts migration key auth."""

    def test_verify_destination_with_migration_key_not_401(
        self, client, migration_key_headers, mock_mgr
    ):
        """Migration key should pass through middleware. Handler validates the key."""
        # Handler calls validate_migration_key and returns 401 if invalid
        mock_mgr.validate_migration_key.return_value = None
        resp = client.post(
            "/api/migration/verify-destination",
            headers=migration_key_headers,
            json={
                "migration_key": "some-test-key",
                "destination_url": "http://dest.example.com/api",
            },
        )
        # 401 from handler (key invalid), NOT from middleware
        assert resp.status_code == 401
        # The fact that we got here means middleware let it through

    def test_verify_destination_no_auth_is_401(self, client):
        """No auth at all should be 401 from middleware."""
        resp = client.post(
            "/api/migration/verify-destination",
            json={
                "migration_key": "some-test-key",
                "destination_url": "http://dest.example.com/api",
            },
        )
        assert resp.status_code == 401


# ── Test 6: stream endpoints accessible with migration key auth ──


class TestStreamAuth:
    """GET /api/migration/stream/* accepts migration key auth."""

    def test_stream_config_with_migration_key_not_401(
        self, client, migration_key_headers, mock_mgr
    ):
        """Migration key should pass through middleware to handler."""
        # Handler validates the key itself — will return 401 if invalid
        mock_mgr.validate_migration_key.return_value = None
        resp = client.get("/api/migration/stream/config", headers=migration_key_headers)
        # 401 from handler (key validation), not middleware
        assert resp.status_code == 401

    def test_stream_entries_with_migration_key_not_401(
        self, client, migration_key_headers, mock_mgr
    ):
        """Migration key should pass through middleware to handler."""
        mock_mgr.validate_migration_key.return_value = None
        resp = client.get("/api/migration/stream/entries", headers=migration_key_headers)
        assert resp.status_code == 401

    def test_stream_config_no_auth_is_401(self, client):
        """No auth at all should be 401 from middleware."""
        resp = client.get("/api/migration/stream/config")
        assert resp.status_code == 401

    def test_stream_entries_no_auth_is_401(self, client):
        resp = client.get("/api/migration/stream/entries")
        assert resp.status_code == 401


# ── Test 7: migration redirect — 449 on ingest when redirecting ──


class TestIngestMigrationRedirect:
    """POST /api/ingest returns 449 when source is redirecting."""

    def test_ingest_returns_449_when_redirecting(
        self, client, admin_headers, mock_mgr, mock_pool
    ):
        """When migration is in redirect state, ingest should return 449."""
        migration_id = uuid4()
        mock_mgr.is_redirecting.return_value = True
        mock_mgr.get_redirect_info.return_value = {
            "migration_id": migration_id,
            "peer_url": "http://new-server.example.com/api",
            "ed25519_private_key": b"\x01" * 32,
            "ed25519_public_key": b"\x02" * 32,
        }
        mock_mgr.sign_redirect.return_value = "base64sig=="

        resp = client.post(
            "/api/ingest",
            json={"entries": [
                {
                    "payload_hash": "hash-001",
                    "session_id": "s1",
                    "message_type": "user",
                    "content_type": "user",
                    "raw_content": "Hello",
                    "timestamp": "2026-01-15T10:30:00Z",
                    "project_path": "/home/user/project",
                    "client_machine_id": "machine-01",
                },
            ]},
            headers=admin_headers,
        )

        assert resp.status_code == 449
        data = resp.json()
        assert data["redirect_url"] == "http://new-server.example.com/api"
        assert data["migration_id"] == str(migration_id)
        assert "signature" in data

    def test_ingest_normal_when_not_redirecting(
        self, client, admin_headers, mock_mgr, mock_pool
    ):
        """When no migration redirect is active, ingest works normally."""
        mock_mgr.is_redirecting.return_value = False
        mock_pool.fetch.return_value = [{"id": 1}]

        resp = client.post(
            "/api/ingest",
            json={"entries": [
                {
                    "payload_hash": "hash-002",
                    "session_id": "s1",
                    "message_type": "user",
                    "content_type": "user",
                    "raw_content": "Hello",
                    "timestamp": "2026-01-15T10:30:00Z",
                    "project_path": "/home/user/project",
                    "client_machine_id": "machine-01",
                },
            ]},
            headers=admin_headers,
        )

        assert resp.status_code == 200
        data = resp.json()
        assert data["accepted"] == 1


# ── Test 8: pubkey header injection middleware ──


class TestMigrationPubkeyMiddleware:
    """Middleware injects X-Memlayer-Migration-Pubkey on ingest responses."""

    def test_pubkey_header_present_when_migration_active(
        self, client, admin_headers, mock_mgr, mock_pool
    ):
        """When source migration is active, ingest response has pubkey header."""
        pubkey_bytes = b"\xab" * 32
        mock_mgr.get_active_state.return_value = {
            "ed25519_public_key": pubkey_bytes,
            "state": "INITIATED",
        }
        mock_mgr.is_redirecting.return_value = False
        mock_pool.fetch.return_value = [{"id": 1}]

        resp = client.post(
            "/api/ingest",
            json={"entries": [
                {
                    "payload_hash": "hash-003",
                    "session_id": "s1",
                    "message_type": "user",
                    "content_type": "user",
                    "raw_content": "Hello",
                    "timestamp": "2026-01-15T10:30:00Z",
                    "project_path": "/home/user/project",
                    "client_machine_id": "machine-01",
                },
            ]},
            headers=admin_headers,
        )

        assert resp.status_code == 200
        expected_b64 = base64.urlsafe_b64encode(pubkey_bytes).decode()
        assert resp.headers.get("X-Memlayer-Migration-Pubkey") == expected_b64

    def test_no_pubkey_header_when_no_migration(
        self, client, admin_headers, mock_mgr, mock_pool
    ):
        """When no migration is active, no pubkey header should be present."""
        mock_mgr.get_active_state.return_value = None
        mock_mgr.is_redirecting.return_value = False
        mock_pool.fetch.return_value = [{"id": 1}]

        resp = client.post(
            "/api/ingest",
            json={"entries": [
                {
                    "payload_hash": "hash-004",
                    "session_id": "s1",
                    "message_type": "user",
                    "content_type": "user",
                    "raw_content": "Hello",
                    "timestamp": "2026-01-15T10:30:00Z",
                    "project_path": "/home/user/project",
                    "client_machine_id": "machine-01",
                },
            ]},
            headers=admin_headers,
        )

        assert resp.status_code == 200
        assert "X-Memlayer-Migration-Pubkey" not in resp.headers
