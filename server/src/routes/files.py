import logging
import uuid as uuid_mod

from fastapi import APIRouter, HTTPException
from fastapi.responses import FileResponse

from ..file_storage import get_file_path

logger = logging.getLogger(__name__)
router = APIRouter()


@router.get("/files/{file_id}")
async def download_file(file_id: str):
    # Validate UUID format
    try:
        uuid_mod.UUID(file_id)
    except ValueError:
        raise HTTPException(400, "Invalid file ID format")

    try:
        path = await get_file_path(file_id)
    except FileNotFoundError:
        raise HTTPException(404, f"File {file_id} not found")

    return FileResponse(path, media_type="text/plain", filename=f"{file_id}.txt")
