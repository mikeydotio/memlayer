from pydantic import BaseModel, Field
from datetime import datetime


class IngestEntry(BaseModel):
    payload_hash: str
    session_id: str
    message_type: str
    content_type: str
    raw_content: str
    timestamp: str
    project_path: str
    client_machine_id: str
    slug: str | None = None
    source_uuid: str | None = None
    parent_uuid: str | None = None
    tool_name: str | None = None
    cwd: str | None = None
    git_branch: str | None = None


class IngestRequest(BaseModel):
    entries: list[IngestEntry]


class IngestResponse(BaseModel):
    accepted: int
    duplicates: int
    errors: int


class SearchRequest(BaseModel):
    query: str
    session_id: str | None = None
    project_path: str | None = None
    limit: int = Field(default=20, ge=1, le=100)


class SearchResult(BaseModel):
    id: int
    session_id: str
    message_type: str
    content_type: str
    raw_content: str
    tool_name: str | None
    created_at: datetime
    project_path: str | None
    fts_rank: int
    vector_rank: int
    rrf_score: float


class LargeResponseRef(BaseModel):
    schema_version: int = 1
    file_id: str
    file_url: str
    size_bytes: int
    summary: str
    index: str
    content_type: str


class SearchResponse(BaseModel):
    results: list[SearchResult]
    total: int
    query_embedding_ms: float
    search_ms: float
    large_response: LargeResponseRef | None = None


class SessionMessage(BaseModel):
    id: int
    message_type: str
    content_type: str
    raw_content: str
    tool_name: str | None
    created_at: datetime


class SessionSummary(BaseModel):
    session_id: str
    project_path: str | None
    slug: str | None
    created_at: datetime
    message_count: int
    messages: list[SessionMessage]
    large_response: LargeResponseRef | None = None
