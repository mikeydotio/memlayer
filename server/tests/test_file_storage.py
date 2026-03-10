"""Tests for file_storage.py, files.py routes, and _maybe_offload in search.py."""

import os
import tempfile
import uuid
from datetime import datetime, timezone
from unittest.mock import AsyncMock, MagicMock, patch

import pytest
from fastapi.testclient import TestClient


# ---------------------------------------------------------------------------
# file_storage.py unit tests
# ---------------------------------------------------------------------------

class TestStoreResponseFile:
    """store_response_file creates a disk file and DB record."""

    @pytest.mark.asyncio
    async def test_store_creates_file_and_db_record(self, mock_pool):
        """store_response_file should write content to disk and INSERT into DB."""
        file_id = str(uuid.uuid4())
        fake_row = {
            "id": uuid.UUID(file_id),
            "file_path": f"/tmp/test_files/{file_id}.txt",
            "size_bytes": 13,
            "content_type": "text",
            "summary": "test summary",
            "structural_index": "idx",
            "source_endpoint": "/api/search",
            "created_at": datetime(2026, 1, 1, tzinfo=timezone.utc),
            "last_accessed_at": datetime(2026, 1, 1, tzinfo=timezone.utc),
        }
        mock_pool.fetchrow.return_value = fake_row

        with tempfile.TemporaryDirectory() as tmpdir:
            test_settings = _make_settings(file_storage_path=tmpdir)
            with (
                patch("src.file_storage.get_pool", return_value=mock_pool),
                patch("src.file_storage.settings", test_settings),
            ):
                from src.file_storage import store_response_file

                result = await store_response_file(
                    content="hello, world!",
                    source_endpoint="/api/search",
                    source_params={"query": "test"},
                    summary="test summary",
                    structural_index="idx",
                    content_type="text",
                )

            # DB INSERT was called
            mock_pool.fetchrow.assert_called_once()
            call_args = mock_pool.fetchrow.call_args
            assert "INSERT INTO response_files" in call_args[0][0]

            # File was written to disk
            written_files = os.listdir(tmpdir)
            assert len(written_files) == 1
            written_path = os.path.join(tmpdir, written_files[0])
            with open(written_path) as f:
                assert f.read() == "hello, world!"

            # Returns dict(row)
            assert result["id"] == uuid.UUID(file_id)
            assert result["size_bytes"] == 13


class TestGetFilePath:
    """get_file_path returns path, updates last_accessed_at, and raises on missing."""

    @pytest.mark.asyncio
    async def test_returns_path_for_existing_file(self, mock_pool):
        """get_file_path should return the disk path when DB record and file exist."""
        with tempfile.NamedTemporaryFile(delete=False, suffix=".txt") as tmp:
            tmp.write(b"data")
            tmp_path = tmp.name

        try:
            mock_pool.fetchrow.return_value = {"file_path": tmp_path}
            with patch("src.file_storage.get_pool", return_value=mock_pool):
                from src.file_storage import get_file_path

                path = await get_file_path(str(uuid.uuid4()))

            assert path == tmp_path
            # Verify UPDATE query was used (updates last_accessed_at)
            call_sql = mock_pool.fetchrow.call_args[0][0]
            assert "UPDATE response_files" in call_sql
            assert "last_accessed_at" in call_sql
        finally:
            os.unlink(tmp_path)

    @pytest.mark.asyncio
    async def test_raises_for_missing_db_record(self, mock_pool):
        """get_file_path should raise FileNotFoundError when DB returns no row."""
        mock_pool.fetchrow.return_value = None
        with patch("src.file_storage.get_pool", return_value=mock_pool):
            from src.file_storage import get_file_path

            with pytest.raises(FileNotFoundError, match="not found or deleted"):
                await get_file_path(str(uuid.uuid4()))

    @pytest.mark.asyncio
    async def test_raises_for_missing_disk_file(self, mock_pool):
        """get_file_path should raise FileNotFoundError when file is gone from disk."""
        mock_pool.fetchrow.return_value = {"file_path": "/tmp/nonexistent_file_xyz.txt"}
        with patch("src.file_storage.get_pool", return_value=mock_pool):
            from src.file_storage import get_file_path

            with pytest.raises(FileNotFoundError, match="missing from disk"):
                await get_file_path(str(uuid.uuid4()))


class TestEvictLruFiles:
    """evict_lru_files removes oldest files until under target."""

    @pytest.mark.asyncio
    async def test_evicts_files_until_under_target(self, mock_pool):
        """evict_lru_files should delete files LRU-first until total <= target."""
        file_id = uuid.uuid4()

        # get_total_file_size: first call over target (500), second still over (300), third under (100)
        total_sizes = [500, 300, 100]
        total_iter = iter(total_sizes)

        async def mock_fetchrow_for_total(sql, *args):
            if "SUM(size_bytes)" in sql:
                return {"total": next(total_iter)}
            # LRU select returns a file to evict
            if "ORDER BY last_accessed_at" in sql:
                return {"id": file_id, "file_path": "/tmp/fake_evict.txt"}
            return None

        mock_pool.fetchrow.side_effect = mock_fetchrow_for_total
        mock_pool.execute = AsyncMock()

        with (
            patch("src.file_storage.get_pool", return_value=mock_pool),
            patch("os.remove") as mock_remove,
        ):
            from src.file_storage import evict_lru_files

            evicted = await evict_lru_files(target_bytes=200)

        assert evicted == 2
        assert mock_remove.call_count == 2
        mock_remove.assert_called_with("/tmp/fake_evict.txt")


# ---------------------------------------------------------------------------
# Route tests (files.py) via TestClient
# ---------------------------------------------------------------------------

@pytest.fixture(autouse=True)
def _patch_settings_for_routes():
    """Patch settings for route tests."""
    with patch.dict("os.environ", {
        "MEMLAYER_AUTH_TOKEN": "test-token-abc",
        "OPENAI_API_KEY": "",
    }):
        from src.config import Settings
        test_settings = Settings()
        with patch("src.config.settings", test_settings):
            with patch("src.main.settings", test_settings):
                yield test_settings


@pytest.fixture
def client(mock_pool):
    """TestClient with mocked deps for file route tests."""
    with patch("src.db.pool", mock_pool):
        with patch("src.routes.ingest.get_pool", return_value=mock_pool):
            with patch("src.routes.search.get_pool", return_value=mock_pool):
                with patch("src.embeddings.embed_query", new_callable=AsyncMock, return_value=None):
                    with patch("src.routes.search.embed_query", new_callable=AsyncMock, return_value=None):
                        with patch("src.routes.ingest.enqueue_ids", new_callable=AsyncMock):
                            from src.main import app
                            yield TestClient(app, raise_server_exceptions=False)


@pytest.fixture
def auth_headers():
    return {"Authorization": "Bearer test-token-abc"}


class TestFileDownloadRoute:
    """Tests for GET /api/files/{file_id}."""

    def test_invalid_uuid_returns_400(self, client, auth_headers):
        """Request with non-UUID file_id should return 400."""
        resp = client.get("/api/files/not-a-uuid", headers=auth_headers)
        assert resp.status_code == 400
        assert "Invalid file ID format" in resp.json()["detail"]

    def test_valid_uuid_file_not_found_returns_404(self, client, auth_headers):
        """Request with valid UUID but missing file should return 404."""
        file_id = str(uuid.uuid4())
        with patch(
            "src.routes.files.get_file_path",
            new_callable=AsyncMock,
            side_effect=FileNotFoundError("gone"),
        ):
            resp = client.get(f"/api/files/{file_id}", headers=auth_headers)
        assert resp.status_code == 404


class TestFileLinesRoute:
    """Tests for GET /api/files/{file_id}/lines."""

    def test_returns_correct_line_range(self, client, auth_headers):
        """Lines endpoint should return the requested line range."""
        file_id = str(uuid.uuid4())

        with tempfile.NamedTemporaryFile(
            mode="w", delete=False, suffix=".txt"
        ) as tmp:
            tmp.write("line1\nline2\nline3\nline4\nline5\n")
            tmp_path = tmp.name

        try:
            with patch(
                "src.routes.files.get_file_path",
                new_callable=AsyncMock,
                return_value=tmp_path,
            ):
                resp = client.get(
                    f"/api/files/{file_id}/lines?start=2&end=4",
                    headers=auth_headers,
                )
            assert resp.status_code == 200
            assert resp.text == "line2\nline3\nline4\n"
            assert resp.headers["X-Line-Range"] == "2-4"
            assert resp.headers["X-Total-Lines"] == "5"
        finally:
            os.unlink(tmp_path)


# ---------------------------------------------------------------------------
# _maybe_offload tests
# ---------------------------------------------------------------------------

class TestMaybeOffload:
    """Tests for _maybe_offload in search.py."""

    @pytest.mark.asyncio
    async def test_under_budget_returns_none(self):
        """Small response under budget should return None (no offload)."""
        test_settings = _make_settings(response_budget_bytes=200000)
        small_json = '{"results": []}'

        with patch("src.routes.search.settings", test_settings):
            from src.routes.search import _maybe_offload

            result = await _maybe_offload(small_json, "/api/search")

        assert result is None

    @pytest.mark.asyncio
    async def test_over_budget_returns_large_response_ref(self):
        """Response exceeding budget should be offloaded and return LargeResponseRef."""
        test_settings = _make_settings(response_budget_bytes=100)
        large_json = '{"data": "' + "x" * 200 + '"}'

        file_id = str(uuid.uuid4())
        fake_record = {
            "id": uuid.UUID(file_id),
            "size_bytes": len(large_json.encode()),
        }

        with (
            patch("src.routes.search.settings", test_settings),
            patch(
                "src.routes.search.generate_index",
                new_callable=AsyncMock,
                return_value=("A summary", "An index", "json"),
            ),
            patch(
                "src.routes.search.store_response_file",
                new_callable=AsyncMock,
                return_value=fake_record,
            ) as mock_store,
        ):
            from src.routes.search import _maybe_offload

            result = await _maybe_offload(
                large_json,
                "/api/search",
                source_params={"query": "test"},
            )

        assert result is not None
        assert result.file_id == file_id
        assert result.file_url == f"/api/files/{file_id}"
        assert result.summary == "A summary"
        assert result.index == "An index"
        assert result.content_type == "json"

        mock_store.assert_called_once()
        call_kwargs = mock_store.call_args
        assert call_kwargs.kwargs["source_endpoint"] == "/api/search"


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def _make_settings(**overrides):
    """Create a lightweight settings-like object with defaults + overrides."""
    defaults = {
        "response_budget_bytes": 200000,
        "file_storage_path": "/data/response_files",
        "file_storage_soft_limit": 0,
        "file_storage_hard_limit": 0,
    }
    defaults.update(overrides)

    class FakeSettings:
        pass

    s = FakeSettings()
    for k, v in defaults.items():
        setattr(s, k, v)
    return s
