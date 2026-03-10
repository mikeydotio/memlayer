"""Unit tests for src/embeddings.py — provider factory, query embedding, queue, status."""

from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from src.config import Settings


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------

@pytest.fixture(autouse=True)
def _clean_globals():
    """Reset module-level globals before each test so tests don't leak state."""
    import src.embeddings as mod
    original_embedder = mod._embedder
    original_queue = mod._queue.copy()
    mod._embedder = None
    mod._queue.clear()
    yield
    mod._embedder = original_embedder
    mod._queue[:] = original_queue


# ---------------------------------------------------------------------------
# get_embedder
# ---------------------------------------------------------------------------

class TestGetEmbedder:
    """Tests for the get_embedder() factory function."""

    def test_returns_openai_when_provider_openai_and_key_set(self):
        """get_embedder should return OpenAIEmbedder when provider=openai and key present."""
        with patch.dict("os.environ", {
            "EMBEDDING_PROVIDER": "openai",
            "OPENAI_API_KEY": "sk-test-key-123",
        }):
            test_settings = Settings()
            with patch("src.embeddings.settings", test_settings):
                from src.embeddings import get_embedder, OpenAIEmbedder
                result = get_embedder()
                assert isinstance(result, OpenAIEmbedder)

    def test_returns_none_when_provider_openai_and_no_key(self):
        """get_embedder should return None when provider=openai but no API key."""
        with patch.dict("os.environ", {
            "EMBEDDING_PROVIDER": "openai",
            "OPENAI_API_KEY": "",
        }):
            test_settings = Settings()
            with patch("src.embeddings.settings", test_settings):
                from src.embeddings import get_embedder
                result = get_embedder()
                assert result is None

    def test_returns_ollama_when_provider_ollama(self):
        """get_embedder should return OllamaEmbedder when provider=ollama."""
        with patch.dict("os.environ", {"EMBEDDING_PROVIDER": "ollama"}):
            test_settings = Settings()
            with patch("src.embeddings.settings", test_settings):
                from src.embeddings import get_embedder, OllamaEmbedder
                result = get_embedder()
                assert isinstance(result, OllamaEmbedder)

    def test_returns_none_for_unknown_provider(self):
        """get_embedder should return None for an unrecognised provider string."""
        with patch.dict("os.environ", {"EMBEDDING_PROVIDER": "banana"}):
            test_settings = Settings()
            with patch("src.embeddings.settings", test_settings):
                from src.embeddings import get_embedder
                result = get_embedder()
                assert result is None


# ---------------------------------------------------------------------------
# embed_query
# ---------------------------------------------------------------------------

class TestEmbedQuery:
    """Tests for the embed_query() coroutine."""

    async def test_returns_none_when_no_embedder(self):
        """embed_query should return None when _embedder is None."""
        import src.embeddings as mod
        mod._embedder = None
        result = await mod.embed_query("hello world")
        assert result is None

    async def test_returns_vector_on_success(self):
        """embed_query should return the first embedding vector on success."""
        import src.embeddings as mod
        mock_embedder = AsyncMock()
        mock_embedder.embed = AsyncMock(return_value=[[0.1, 0.2, 0.3]])
        mod._embedder = mock_embedder

        result = await mod.embed_query("test query")
        assert result == [0.1, 0.2, 0.3]
        mock_embedder.embed.assert_awaited_once_with(["test query"])

    async def test_returns_none_on_exception(self):
        """embed_query should return None and not raise when embed() throws."""
        import src.embeddings as mod
        mock_embedder = AsyncMock()
        mock_embedder.embed = AsyncMock(side_effect=Exception("API down"))
        mod._embedder = mock_embedder

        result = await mod.embed_query("test query")
        assert result is None


# ---------------------------------------------------------------------------
# enqueue_ids
# ---------------------------------------------------------------------------

class TestEnqueueIds:
    """Tests for the enqueue_ids() coroutine."""

    async def test_adds_ids_to_queue(self):
        """enqueue_ids should append given IDs to the module-level _queue."""
        import src.embeddings as mod
        assert mod._queue == []

        await mod.enqueue_ids([10, 20, 30])
        assert mod._queue == [10, 20, 30]

        # Calling again should extend, not replace
        await mod.enqueue_ids([40])
        assert mod._queue == [10, 20, 30, 40]


# ---------------------------------------------------------------------------
# get_embedding_status
# ---------------------------------------------------------------------------

class TestGetEmbeddingStatus:
    """Tests for get_embedding_status()."""

    async def test_returns_correct_counts(self):
        """get_embedding_status should query DB and return structured dict."""
        import src.embeddings as mod

        mock_pool = AsyncMock()
        # First fetchval call: total count; second: embedded count
        mock_pool.fetchval = AsyncMock(side_effect=[100, 75])

        # Put some items in the queue to verify queue_depth
        mod._queue.extend([1, 2, 3])
        mod._embedder = None  # No provider

        with patch("src.embeddings.get_pool", return_value=mock_pool):
            with patch.dict("os.environ", {"EMBEDDING_PROVIDER": "openai", "OPENAI_API_KEY": ""}):
                test_settings = Settings()
                with patch("src.embeddings.settings", test_settings):
                    result = await mod.get_embedding_status()

        assert result["total_entries"] == 100
        assert result["embedded"] == 75
        assert result["pending"] == 25
        assert result["queue_depth"] == 3
        assert result["enabled"] is False
        assert result["provider"] is None
        assert result["model"] is None


# ---------------------------------------------------------------------------
# init_embedder
# ---------------------------------------------------------------------------

class TestInitEmbedder:
    """Tests for init_embedder()."""

    def test_sets_global_embedder(self):
        """init_embedder should set the module-level _embedder via get_embedder()."""
        import src.embeddings as mod

        mock_provider = MagicMock()
        with patch("src.embeddings.get_embedder", return_value=mock_provider):
            mod.init_embedder()
            assert mod._embedder is mock_provider

    def test_sets_none_when_no_provider(self):
        """init_embedder should set _embedder to None when get_embedder returns None."""
        import src.embeddings as mod

        with patch("src.embeddings.get_embedder", return_value=None):
            mod.init_embedder()
            assert mod._embedder is None
