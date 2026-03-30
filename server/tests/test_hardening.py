"""Tests for security hardening changes (Waves 1-4)."""

import re
from unittest.mock import AsyncMock, patch

import pytest
from pydantic import ValidationError


class TestSecretRedaction:
    """Test that secrets are redacted from log output."""

    def test_redacts_postgres_url(self):
        from src.main import _redact
        msg = "Failed to connect: postgresql://user:password@host:5432/db"
        result = _redact(msg)
        assert "password" not in result
        assert "[REDACTED]" in result

    def test_redacts_bearer_token(self):
        from src.main import _redact
        msg = "Auth header: Bearer sk-abc123xyz"
        result = _redact(msg)
        assert "sk-abc123xyz" not in result
        assert "[REDACTED]" in result

    def test_redacts_openai_key(self):
        from src.main import _redact
        msg = "OPENAI_API_KEY=sk-proj-abcdefghij1234567890"
        result = _redact(msg)
        assert "sk-proj-" not in result
        assert "[REDACTED]" in result

    def test_preserves_safe_text(self):
        from src.main import _redact
        msg = "Ingest: accepted=5 duplicates=0 errors=0"
        assert _redact(msg) == msg


class TestModelValidation:
    """Test that Pydantic models enforce field length limits."""

    def test_raw_content_max_length(self):
        from src.models import IngestEntry
        with pytest.raises(ValidationError):
            IngestEntry(
                payload_hash="abc",
                session_id="sess-1",
                message_type="user",
                content_type="text",
                raw_content="x" * 50001,  # Over 50000 limit
                timestamp="2026-01-01T00:00:00Z",
                project_path="/test",
                client_machine_id="m1",
            )

    def test_raw_content_at_limit(self):
        from src.models import IngestEntry
        entry = IngestEntry(
            payload_hash="abc",
            session_id="sess-1",
            message_type="user",
            content_type="text",
            raw_content="x" * 50000,  # Exactly at limit
            timestamp="2026-01-01T00:00:00Z",
            project_path="/test",
            client_machine_id="m1",
        )
        assert len(entry.raw_content) == 50000

    def test_session_id_max_length(self):
        from src.models import IngestEntry
        with pytest.raises(ValidationError):
            IngestEntry(
                payload_hash="abc",
                session_id="x" * 257,  # Over 256 limit
                message_type="user",
                content_type="text",
                raw_content="hello",
                timestamp="2026-01-01T00:00:00Z",
                project_path="/test",
                client_machine_id="m1",
            )


class TestExtractionSanitization:
    """Test LLM extraction input/output sanitization."""

    def test_sanitize_for_prompt_escapes_special_chars(self):
        from src.extraction.llm_extract import _sanitize_for_prompt
        text = 'Hello "world"\nNew line\ttab'
        result = _sanitize_for_prompt(text, 1000)
        assert '"' not in result or '\\"' in result  # Escaped
        assert '\\n' in result  # Newline escaped

    def test_sanitize_for_prompt_truncates(self):
        from src.extraction.llm_extract import _sanitize_for_prompt
        text = "a" * 100
        result = _sanitize_for_prompt(text, 10)
        # After JSON escaping, should be ~10 chars of content
        assert len(result) <= 15  # Small margin for escaping overhead

    def test_parse_extraction_rejects_unsafe_entity_names(self):
        import json
        from src.extraction.llm_extract import _parse_extraction
        payload = json.dumps({
            "entities": [
                {"name": "<script>alert(1)</script>", "type": "concept", "description": "xss", "confidence": 0.9},
                {"name": "valid-entity", "type": "concept", "description": "ok", "confidence": 0.9},
            ],
            "relationships": [],
        })
        entities, rels = _parse_extraction(payload)
        assert len(entities) == 1
        assert entities[0]["name"] == "valid-entity"

    def test_parse_extraction_rejects_unknown_entity_types(self):
        import json
        from src.extraction.llm_extract import _parse_extraction
        payload = json.dumps({
            "entities": [
                {"name": "test", "type": "malicious_type", "description": "bad", "confidence": 0.9},
                {"name": "test2", "type": "concept", "description": "ok", "confidence": 0.9},
            ],
            "relationships": [],
        })
        entities, rels = _parse_extraction(payload)
        assert len(entities) == 1
        assert entities[0]["type"] == "concept"

    def test_parse_extraction_rejects_unknown_relationship_types(self):
        import json
        from src.extraction.llm_extract import _parse_extraction
        payload = json.dumps({
            "entities": [],
            "relationships": [
                {"source": "a", "target": "b", "type": "hacks", "reason": "bad", "confidence": 0.9},
                {"source": "a", "target": "b", "type": "supports", "reason": "ok", "confidence": 0.9},
            ],
        })
        entities, rels = _parse_extraction(payload)
        assert len(rels) == 1
        assert rels[0]["type"] == "supports"

    def test_parse_extraction_clamps_confidence(self):
        import json
        from src.extraction.llm_extract import _parse_extraction
        payload = json.dumps({
            "entities": [
                {"name": "test", "type": "concept", "description": "ok", "confidence": 5.0},
            ],
            "relationships": [],
        })
        entities, _ = _parse_extraction(payload)
        assert entities[0]["confidence"] == 1.0

    def test_parse_extraction_truncates_names(self):
        import json
        from src.extraction.llm_extract import _parse_extraction
        payload = json.dumps({
            "entities": [
                {"name": "a" * 300, "type": "concept", "description": "long", "confidence": 0.5},
            ],
            "relationships": [],
        })
        entities, _ = _parse_extraction(payload)
        assert len(entities[0]["name"]) == 256


class TestRetentionConfig:
    """Test retention configuration."""

    def test_retention_defaults_to_disabled(self):
        from src.config import Settings
        s = Settings()
        assert s.retention_days == 0

    def test_retention_check_interval_default(self):
        from src.config import Settings
        s = Settings()
        assert s.retention_check_interval_secs == 86400.0
