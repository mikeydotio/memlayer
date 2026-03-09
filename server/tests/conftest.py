import asyncio
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock, patch

import pytest


@pytest.fixture
def mock_pool():
    """Create a mock asyncpg pool that supports common operations."""
    pool = AsyncMock()
    pool.fetch = AsyncMock(return_value=[])
    pool.fetchrow = AsyncMock(return_value=None)
    pool.fetchval = AsyncMock(return_value=None)
    pool.execute = AsyncMock()
    pool.close = AsyncMock()
    return pool


@pytest.fixture
def auth_token():
    """Test auth token."""
    return "test-secret-token-12345"


@pytest.fixture
def sample_entry():
    """A single valid ingest entry as a dict."""
    return {
        "payload_hash": "abc123def456",
        "session_id": "sess-001",
        "message_type": "user",
        "content_type": "user",
        "raw_content": "Hello, how are you?",
        "timestamp": "2026-01-15T10:30:00Z",
        "project_path": "/home/user/project",
        "client_machine_id": "machine-01",
        "slug": "test-session",
        "source_uuid": "uuid-001",
        "parent_uuid": None,
        "tool_name": None,
        "cwd": "/home/user/project",
        "git_branch": "main",
    }


@pytest.fixture
def sample_entries(sample_entry):
    """Multiple ingest entries."""
    entry2 = sample_entry.copy()
    entry2["payload_hash"] = "def789ghi012"
    entry2["message_type"] = "assistant"
    entry2["content_type"] = "assistant"
    entry2["raw_content"] = "I'm doing well, thanks!"
    entry2["timestamp"] = "2026-01-15T10:30:05Z"
    return [sample_entry, entry2]


@pytest.fixture
def sample_search_row():
    """A mock database row for search results."""
    row = {
        "id": 1,
        "session_id": "sess-001",
        "message_type": "user",
        "content_type": "user",
        "raw_content": "Hello world",
        "tool_name": None,
        "created_at": datetime(2026, 1, 15, 10, 30, 0, tzinfo=timezone.utc),
        "project_path": "/home/user/project",
        "fts_rank": 1,
        "vector_rank": 0,
        "rrf_score": 0.5,
    }
    return row


@pytest.fixture
def sample_session_row():
    """A mock database row for session lookup."""
    return {
        "session_id": "sess-001",
        "project_path": "/home/user/project",
        "slug": "test-session",
        "created_at": datetime(2026, 1, 15, 10, 0, 0, tzinfo=timezone.utc),
    }


@pytest.fixture
def sample_session_message_row():
    """A mock database row for session messages."""
    return {
        "id": 1,
        "message_type": "user",
        "content_type": "user",
        "raw_content": "Hello world",
        "tool_name": None,
        "created_at": datetime(2026, 1, 15, 10, 30, 0, tzinfo=timezone.utc),
    }
