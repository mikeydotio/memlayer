"""Unit tests for Pydantic models."""

from datetime import datetime, timezone

import pytest
from pydantic import ValidationError

from src.models import (
    IngestEntry,
    IngestRequest,
    SearchRequest,
    SearchResponse,
    SearchResult,
    SessionMessage,
    SessionSummary,
    LargeResponseRef,
)


class TestSearchRequest:
    """Tests for the SearchRequest model."""

    def test_valid_search_request_minimal(self):
        """SearchRequest with only the required query field."""
        req = SearchRequest(query="test query")
        assert req.query == "test query"
        assert req.session_id is None
        assert req.project_path is None
        assert req.after is None
        assert req.before is None
        assert req.types is None
        assert req.limit == 20

    def test_valid_search_request_all_fields(self):
        """SearchRequest with all fields populated."""
        req = SearchRequest(
            query="test query",
            session_id="sess-001",
            project_path="/home/user/project",
            after="2026-01-01T00:00:00Z",
            before="2026-12-31T23:59:59Z",
            types=["user", "assistant"],
            limit=50,
        )
        assert req.query == "test query"
        assert req.session_id == "sess-001"
        assert req.project_path == "/home/user/project"
        assert req.after is not None
        assert req.before is not None
        assert req.types == ["user", "assistant"]
        assert req.limit == 50

    def test_search_request_limit_min_boundary(self):
        """Limit of 1 should be valid (ge=1)."""
        req = SearchRequest(query="test", limit=1)
        assert req.limit == 1

    def test_search_request_limit_max_boundary(self):
        """Limit of 100 should be valid (le=100)."""
        req = SearchRequest(query="test", limit=100)
        assert req.limit == 100

    def test_search_request_limit_too_low(self):
        """Limit of 0 should fail validation (ge=1)."""
        with pytest.raises(ValidationError) as exc_info:
            SearchRequest(query="test", limit=0)
        errors = exc_info.value.errors()
        assert any(e["loc"] == ("limit",) for e in errors)

    def test_search_request_limit_too_high(self):
        """Limit of 101 should fail validation (le=100)."""
        with pytest.raises(ValidationError) as exc_info:
            SearchRequest(query="test", limit=101)
        errors = exc_info.value.errors()
        assert any(e["loc"] == ("limit",) for e in errors)

    def test_search_request_limit_negative(self):
        """Negative limit should fail validation."""
        with pytest.raises(ValidationError):
            SearchRequest(query="test", limit=-5)

    def test_search_request_after_datetime_parsing(self):
        """Datetime strings should be parsed correctly for 'after' filter."""
        req = SearchRequest(query="test", after="2026-03-01T12:00:00Z")
        assert isinstance(req.after, datetime)
        assert req.after.year == 2026
        assert req.after.month == 3
        assert req.after.day == 1
        assert req.after.hour == 12

    def test_search_request_before_datetime_parsing(self):
        """Datetime strings should be parsed correctly for 'before' filter."""
        req = SearchRequest(query="test", before="2026-06-15T18:30:00+00:00")
        assert isinstance(req.before, datetime)
        assert req.before.year == 2026
        assert req.before.month == 6

    def test_search_request_datetime_iso_formats(self):
        """Various ISO 8601 formats should be accepted."""
        # With timezone offset
        req1 = SearchRequest(query="test", after="2026-01-01T00:00:00+05:00")
        assert req1.after is not None

        # With Z suffix
        req2 = SearchRequest(query="test", after="2026-01-01T00:00:00Z")
        assert req2.after is not None

    def test_search_request_missing_query(self):
        """Missing query should fail validation."""
        with pytest.raises(ValidationError):
            SearchRequest()

    def test_search_request_empty_types_list(self):
        """Empty types list should be valid."""
        req = SearchRequest(query="test", types=[])
        assert req.types == []


class TestIngestEntry:
    """Tests for the IngestEntry model."""

    def test_valid_ingest_entry_minimal(self, sample_entry):
        """IngestEntry with required fields only."""
        minimal = {
            "payload_hash": sample_entry["payload_hash"],
            "session_id": sample_entry["session_id"],
            "message_type": sample_entry["message_type"],
            "content_type": sample_entry["content_type"],
            "raw_content": sample_entry["raw_content"],
            "timestamp": sample_entry["timestamp"],
            "project_path": sample_entry["project_path"],
            "client_machine_id": sample_entry["client_machine_id"],
        }
        entry = IngestEntry(**minimal)
        assert entry.payload_hash == "abc123def456"
        assert entry.session_id == "sess-001"
        assert entry.slug is None
        assert entry.source_uuid is None
        assert entry.parent_uuid is None
        assert entry.tool_name is None
        assert entry.cwd is None
        assert entry.git_branch is None

    def test_valid_ingest_entry_all_fields(self, sample_entry):
        """IngestEntry with all fields populated."""
        entry = IngestEntry(**sample_entry)
        assert entry.payload_hash == "abc123def456"
        assert entry.slug == "test-session"
        assert entry.source_uuid == "uuid-001"
        assert entry.cwd == "/home/user/project"
        assert entry.git_branch == "main"

    def test_ingest_entry_missing_required_field(self):
        """Missing required field should fail validation."""
        with pytest.raises(ValidationError):
            IngestEntry(
                payload_hash="abc",
                session_id="sess",
                # missing message_type and others
            )

    def test_ingest_request_with_entries(self, sample_entries):
        """IngestRequest wrapping multiple entries."""
        req = IngestRequest(entries=[IngestEntry(**e) for e in sample_entries])
        assert len(req.entries) == 2
        assert req.entries[0].message_type == "user"
        assert req.entries[1].message_type == "assistant"


class TestSearchResult:
    """Tests for SearchResult model."""

    def test_search_result_creation(self):
        """SearchResult should accept all required fields."""
        result = SearchResult(
            id=1,
            session_id="sess-001",
            message_type="user",
            content_type="user",
            raw_content="Hello",
            tool_name=None,
            created_at=datetime(2026, 1, 1, tzinfo=timezone.utc),
            project_path="/project",
            fts_rank=1,
            vector_rank=2,
            rrf_score=0.75,
        )
        assert result.id == 1
        assert result.rrf_score == 0.75
        assert result.tool_name is None


class TestLargeResponseRef:
    """Tests for LargeResponseRef model."""

    def test_large_response_ref_defaults(self):
        """schema_version should default to 1."""
        ref = LargeResponseRef(
            file_id="abc-123",
            file_url="/api/files/abc-123",
            size_bytes=10000,
            summary="A large response",
            index="L1: heading",
            content_type="markdown",
        )
        assert ref.schema_version == 1
        assert ref.file_id == "abc-123"


class TestSessionModels:
    """Tests for SessionMessage and SessionSummary models."""

    def test_session_message(self):
        msg = SessionMessage(
            id=1,
            message_type="user",
            content_type="user",
            raw_content="Hello",
            tool_name=None,
            created_at=datetime(2026, 1, 1, tzinfo=timezone.utc),
        )
        assert msg.id == 1
        assert msg.tool_name is None

    def test_session_summary(self):
        msg = SessionMessage(
            id=1,
            message_type="user",
            content_type="user",
            raw_content="Hello",
            tool_name=None,
            created_at=datetime(2026, 1, 1, tzinfo=timezone.utc),
        )
        summary = SessionSummary(
            session_id="sess-001",
            project_path="/project",
            slug="test",
            created_at=datetime(2026, 1, 1, tzinfo=timezone.utc),
            message_count=1,
            messages=[msg],
        )
        assert summary.session_id == "sess-001"
        assert summary.message_count == 1
        assert summary.large_response is None
