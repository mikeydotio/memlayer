"""Unit tests for Settings configuration."""

import os
from unittest.mock import patch

import pytest

from src.config import Settings


class TestSettingsDefaults:
    """Tests for default Settings values."""

    def test_default_database_url(self):
        """Default database URL should point to localhost."""
        s = Settings()
        assert "localhost:5432" in s.database_url
        assert "memlayer" in s.database_url

    def test_default_auth_token_empty(self):
        """Default auth token should be empty (no auth required)."""
        s = Settings()
        assert s.memlayer_auth_token == ""

    def test_default_embedding_provider(self):
        """Default embedding provider should be openai."""
        s = Settings()
        assert s.embedding_provider == "openai"

    def test_default_embedding_dimensions(self):
        """Default embedding dimensions should be 1536."""
        s = Settings()
        assert s.embedding_dimensions == 1536

    def test_default_embedding_model(self):
        """Default embedding model should be text-embedding-3-small."""
        s = Settings()
        assert s.embedding_model == "text-embedding-3-small"

    def test_default_response_budget(self):
        """Default response budget should be 200KB."""
        s = Settings()
        assert s.response_budget_bytes == 200000

    def test_default_file_storage_limits(self):
        """Default file storage limits should be 0 (unlimited)."""
        s = Settings()
        assert s.file_storage_soft_limit == 0
        assert s.file_storage_hard_limit == 0
        assert s.max_file_size == 0
        assert s.max_db_size == 0

    def test_default_index_mode(self):
        """Default index mode should be off."""
        s = Settings()
        assert s.index_mode == "off"

    def test_default_index_llm_provider_empty(self):
        """Default LLM provider for indexing should be empty."""
        s = Settings()
        assert s.index_llm_provider == ""

    def test_default_log_settings(self):
        """Default log format and level."""
        s = Settings()
        assert s.log_format == "text"
        assert s.log_level == "INFO"

    def test_default_eviction_interval(self):
        """Default eviction interval should be 60 seconds."""
        s = Settings()
        assert s.eviction_interval_secs == 60.0

    def test_default_embedding_batch_size(self):
        """Default embedding batch size should be 20."""
        s = Settings()
        assert s.embedding_batch_size == 20

    def test_default_embedding_interval(self):
        """Default embedding interval should be 5 seconds."""
        s = Settings()
        assert s.embedding_interval_secs == 5.0


class TestSettingsOverride:
    """Tests for overriding Settings via environment variables."""

    def test_override_database_url(self):
        """DATABASE_URL env var should override default."""
        with patch.dict(os.environ, {"DATABASE_URL": "postgresql://user:pass@db:5432/mydb"}):
            s = Settings()
            assert s.database_url == "postgresql://user:pass@db:5432/mydb"

    def test_override_auth_token(self):
        """MEMLAYER_AUTH_TOKEN env var should override default."""
        with patch.dict(os.environ, {"MEMLAYER_AUTH_TOKEN": "my-secret"}):
            s = Settings()
            assert s.memlayer_auth_token == "my-secret"

    def test_override_embedding_provider(self):
        """EMBEDDING_PROVIDER env var should override default."""
        with patch.dict(os.environ, {"EMBEDDING_PROVIDER": "ollama"}):
            s = Settings()
            assert s.embedding_provider == "ollama"

    def test_override_embedding_dimensions(self):
        """EMBEDDING_DIMENSIONS env var should override default."""
        with patch.dict(os.environ, {"EMBEDDING_DIMENSIONS": "768"}):
            s = Settings()
            assert s.embedding_dimensions == 768

    def test_override_file_storage_path(self):
        """FILE_STORAGE_PATH env var should override default."""
        with patch.dict(os.environ, {"FILE_STORAGE_PATH": "/tmp/files"}):
            s = Settings()
            assert s.file_storage_path == "/tmp/files"

    def test_override_index_mode(self):
        """INDEX_MODE env var should override default."""
        with patch.dict(os.environ, {"INDEX_MODE": "hybrid"}):
            s = Settings()
            assert s.index_mode == "hybrid"

    def test_override_index_llm_provider(self):
        """INDEX_LLM_PROVIDER env var should override default."""
        with patch.dict(os.environ, {"INDEX_LLM_PROVIDER": "anthropic"}):
            s = Settings()
            assert s.index_llm_provider == "anthropic"

    def test_override_file_storage_limits(self):
        """File storage limit env vars should override defaults."""
        with patch.dict(os.environ, {
            "FILE_STORAGE_SOFT_LIMIT": "1000000",
            "FILE_STORAGE_HARD_LIMIT": "5000000",
        }):
            s = Settings()
            assert s.file_storage_soft_limit == 1000000
            assert s.file_storage_hard_limit == 5000000

    def test_override_log_format(self):
        """LOG_FORMAT env var should override default."""
        with patch.dict(os.environ, {"LOG_FORMAT": "json"}):
            s = Settings()
            assert s.log_format == "json"

    def test_override_anthropic_key(self):
        """ANTHROPIC_API_KEY env var should be read."""
        with patch.dict(os.environ, {"ANTHROPIC_API_KEY": "sk-ant-test"}):
            s = Settings()
            assert s.anthropic_api_key == "sk-ant-test"
