"""API tests using FastAPI TestClient with mocked database."""

import asyncio
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from fastapi import HTTPException
from fastapi.testclient import TestClient


# Patch settings before importing the app so auth token is set
@pytest.fixture(autouse=True)
def _patch_settings():
    """Patch settings for all API tests."""
    with patch.dict("os.environ", {
        "MEMLAYER_AUTH_TOKEN": "test-token-abc",
        "OPENAI_API_KEY": "",
    }):
        # Force re-creation of Settings with test env
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
def client(mock_pool):
    """TestClient with mocked dependencies (raise_server_exceptions=False for route tests)."""
    with patch("src.db.pool", mock_pool):
        with patch("src.routes.ingest.get_pool", return_value=mock_pool):
            with patch("src.routes.search.get_pool", return_value=mock_pool):
                with patch("src.routes.files.get_file_path", new_callable=AsyncMock):
                    with patch("src.embeddings.embed_query", new_callable=AsyncMock, return_value=None):
                        with patch("src.routes.search.embed_query", new_callable=AsyncMock, return_value=None):
                            with patch("src.routes.ingest.enqueue_ids", new_callable=AsyncMock):
                                from src.main import app
                                yield TestClient(app, raise_server_exceptions=False)



@pytest.fixture
def auth_headers():
    """Authorization headers with test token."""
    return {"Authorization": "Bearer test-token-abc"}


class TestHealthEndpoint:
    """Tests for GET /health."""

    def test_health_returns_200(self, client, mock_pool):
        """Health endpoint should return 200 with component status."""
        mock_pool.fetchval.return_value = 1
        resp = client.get("/health")
        assert resp.status_code == 200
        data = resp.json()
        assert "status" in data
        assert "components" in data

    def test_health_no_auth_required(self, client):
        """Health endpoint should not require authentication."""
        resp = client.get("/health")
        assert resp.status_code == 200


class TestAuthMiddleware:
    """Tests for auth middleware."""

    def test_api_endpoint_requires_auth(self, client):
        """API endpoints should return 401 without auth token."""
        resp = client.post("/api/search", json={"query": "test"})
        assert resp.status_code == 401

    def test_api_endpoint_wrong_token(self, client):
        """API endpoints should return 401 with wrong auth token."""
        resp = client.post(
            "/api/search",
            json={"query": "test"},
            headers={"Authorization": "Bearer wrong-token"},
        )
        assert resp.status_code == 401

    def test_api_endpoint_valid_token(self, client, auth_headers, mock_pool):
        """API endpoints should accept valid auth token."""
        mock_pool.fetch.return_value = []
        resp = client.post(
            "/api/search",
            json={"query": "test"},
            headers=auth_headers,
        )
        assert resp.status_code == 200

    def test_health_bypasses_auth(self, client):
        """Health endpoint should bypass auth middleware."""
        resp = client.get("/health")
        assert resp.status_code == 200

    def test_ingest_requires_auth(self, client):
        """Ingest endpoint should require auth."""
        resp = client.post("/api/ingest", json={"entries": []})
        assert resp.status_code == 401

    def test_session_summary_requires_auth(self, client):
        """Session summary should require auth."""
        resp = client.get("/api/sessions/sess-001/summary")
        assert resp.status_code == 401


class TestIngestEndpoint:
    """Tests for POST /api/ingest."""

    def test_ingest_empty_batch(self, client, auth_headers, mock_pool):
        """Ingest with empty entries should return zeros."""
        resp = client.post(
            "/api/ingest",
            json={"entries": []},
            headers=auth_headers,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["accepted"] == 0
        assert data["duplicates"] == 0
        assert data["errors"] == 0

    def test_ingest_valid_entry(self, client, auth_headers, mock_pool, sample_entry):
        """Ingest with a valid entry should succeed."""
        # Batch INSERT uses pool.fetch() and returns rows with id
        mock_pool.fetch.return_value = [{"id": 1}]
        resp = client.post(
            "/api/ingest",
            json={"entries": [sample_entry]},
            headers=auth_headers,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["accepted"] == 1
        assert data["duplicates"] == 0

    def test_ingest_duplicate_entry(self, client, auth_headers, mock_pool, sample_entry):
        """Duplicate entry (ON CONFLICT DO NOTHING returns no rows) should count as duplicate."""
        # Batch INSERT returns empty list when all entries are duplicates
        mock_pool.fetch.return_value = []
        resp = client.post(
            "/api/ingest",
            json={"entries": [sample_entry]},
            headers=auth_headers,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["accepted"] == 0
        assert data["duplicates"] == 1

    def test_ingest_max_batch_exceeded(self, client, auth_headers, sample_entry):
        """Batch of >200 entries should be rejected."""
        entries = [sample_entry.copy() for _ in range(201)]
        for i, e in enumerate(entries):
            e["payload_hash"] = f"hash-{i}"
        resp = client.post(
            "/api/ingest",
            json={"entries": entries},
            headers=auth_headers,
        )
        assert resp.status_code == 400


class TestSearchEndpoint:
    """Tests for POST /api/search."""

    def test_search_valid_query(self, client, auth_headers, mock_pool, sample_search_row):
        """Search with valid query should return results."""
        mock_pool.fetch.return_value = [sample_search_row]
        resp = client.post(
            "/api/search",
            json={"query": "hello world"},
            headers=auth_headers,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["total"] == 1
        assert len(data["results"]) == 1
        assert data["results"][0]["raw_content"] == "Hello world"
        assert "query_embedding_ms" in data
        assert "search_ms" in data

    def test_search_empty_results(self, client, auth_headers, mock_pool):
        """Search with no matches should return empty results."""
        mock_pool.fetch.return_value = []
        resp = client.post(
            "/api/search",
            json={"query": "nonexistent query xyz"},
            headers=auth_headers,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["total"] == 0
        assert data["results"] == []

    def test_search_with_type_filters(self, client, auth_headers, mock_pool, sample_search_row):
        """Search with types filter should pass filters to query."""
        mock_pool.fetch.return_value = [sample_search_row]
        resp = client.post(
            "/api/search",
            json={"query": "hello", "types": ["user"]},
            headers=auth_headers,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["total"] == 1

    def test_search_with_date_filters(self, client, auth_headers, mock_pool):
        """Search with after/before date filters should be accepted."""
        mock_pool.fetch.return_value = []
        resp = client.post(
            "/api/search",
            json={
                "query": "test",
                "after": "2026-01-01T00:00:00Z",
                "before": "2026-12-31T23:59:59Z",
            },
            headers=auth_headers,
        )
        assert resp.status_code == 200

    def test_search_with_session_filter(self, client, auth_headers, mock_pool):
        """Search scoped to a specific session."""
        mock_pool.fetch.return_value = []
        resp = client.post(
            "/api/search",
            json={"query": "test", "session_id": "sess-001"},
            headers=auth_headers,
        )
        assert resp.status_code == 200

    def test_search_with_project_filter(self, client, auth_headers, mock_pool):
        """Search scoped to a specific project."""
        mock_pool.fetch.return_value = []
        resp = client.post(
            "/api/search",
            json={"query": "test", "project_path": "/home/user/project"},
            headers=auth_headers,
        )
        assert resp.status_code == 200

    def test_search_with_custom_limit(self, client, auth_headers, mock_pool):
        """Search with custom limit."""
        mock_pool.fetch.return_value = []
        resp = client.post(
            "/api/search",
            json={"query": "test", "limit": 5},
            headers=auth_headers,
        )
        assert resp.status_code == 200

    def test_search_invalid_limit(self, client, auth_headers):
        """Search with invalid limit should return 422."""
        resp = client.post(
            "/api/search",
            json={"query": "test", "limit": 0},
            headers=auth_headers,
        )
        assert resp.status_code == 422

    def test_search_missing_query(self, client, auth_headers):
        """Search without query field should return 422."""
        resp = client.post(
            "/api/search",
            json={},
            headers=auth_headers,
        )
        assert resp.status_code == 422


class TestSessionSummaryEndpoint:
    """Tests for GET /api/sessions/{id}/summary."""

    def test_session_not_found(self, client, auth_headers, mock_pool):
        """Non-existent session should return 404."""
        mock_pool.fetchrow.return_value = None
        resp = client.get(
            "/api/sessions/nonexistent-session/summary",
            headers=auth_headers,
        )
        assert resp.status_code == 404

    def test_session_found(
        self, client, auth_headers, mock_pool,
        sample_session_row, sample_session_message_row,
    ):
        """Existing session should return summary with messages."""
        mock_pool.fetchrow.return_value = sample_session_row
        mock_pool.fetch.return_value = [sample_session_message_row]
        resp = client.get(
            "/api/sessions/sess-001/summary",
            headers=auth_headers,
        )
        assert resp.status_code == 200
        data = resp.json()
        assert data["session_id"] == "sess-001"
        assert data["message_count"] == 1
        assert len(data["messages"]) == 1
        assert data["messages"][0]["raw_content"] == "Hello world"
