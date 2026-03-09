import logging
import uuid as uuid_mod

from fastapi import APIRouter, HTTPException, Query
from fastapi.responses import FileResponse, PlainTextResponse

from ..analytics import response_analytics
from ..file_storage import get_file_path

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/files/{file_id}")
async def download_file(file_id: str):
    """Download the full file. Client should prefer /lines endpoint when only
    a portion of the response is relevant."""
    # Validate UUID format
    try:
        uuid_mod.UUID(file_id)
    except ValueError:
        raise HTTPException(400, "Invalid file ID format")

    try:
        path = await get_file_path(file_id)
    except FileNotFoundError:
        raise HTTPException(404, f"File {file_id} not found")

    response_analytics.record("/api/files/download", 0, offloaded=False)
    return FileResponse(path, media_type="text/plain", filename=f"{file_id}.txt")


@router.get("/files/{file_id}/lines")
async def read_lines(
    file_id: str,
    start: int = Query(ge=1, description="Start line number (1-indexed, inclusive)"),
    end: int = Query(ge=1, description="End line number (1-indexed, inclusive)"),
):
    """Read a specific line range from a response file. More efficient than
    downloading the full file when only a portion is needed."""
    try:
        uuid_mod.UUID(file_id)
    except ValueError:
        raise HTTPException(400, "Invalid file ID format")

    try:
        path = await get_file_path(file_id)
    except FileNotFoundError:
        raise HTTPException(404, f"File {file_id} not found")

    if end < start:
        raise HTTPException(400, "end must be >= start")

    with open(path, "r", encoding="utf-8") as f:
        lines = f.readlines()

    # 1-indexed, inclusive
    start_idx = max(0, start - 1)
    end_idx = min(len(lines), end)
    selected = lines[start_idx:end_idx]

    response_analytics.record("/api/files/lines", sum(len(l) for l in selected), offloaded=False)

    return PlainTextResponse(
        "".join(selected),
        headers={"X-Line-Range": f"{start}-{end}", "X-Total-Lines": str(len(lines))},
    )
