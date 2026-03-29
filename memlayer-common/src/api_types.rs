use serde::{Deserialize, Serialize};

// ── Large response reference ────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LargeResponseRef {
    pub schema_version: i32,
    pub file_id: String,
    pub file_url: String,
    pub size_bytes: i64,
    pub summary: String,
    pub index: String,
    pub content_type: String,
}

// ── Search types ────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: i64,
    pub session_id: String,
    pub message_type: String,
    pub content_type: String,
    pub raw_content: String,
    pub tool_name: Option<String>,
    pub created_at: String,
    pub project_path: Option<String>,
    pub fts_rank: i32,
    pub vector_rank: i32,
    pub rrf_score: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total: i64,
    pub query_embedding_ms: f64,
    pub search_ms: f64,
    #[serde(default)]
    pub large_response: Option<LargeResponseRef>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,
}

// ── Session types ───────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: i64,
    pub message_type: String,
    pub content_type: String,
    pub raw_content: String,
    pub tool_name: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub project_path: Option<String>,
    pub slug: Option<String>,
    pub created_at: String,
    pub message_count: i64,
    pub messages: Vec<SessionMessage>,
    #[serde(default)]
    pub large_response: Option<LargeResponseRef>,
}

// ── Browse types (dashboard) ────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub project_path: String,
    pub session_count: i64,
    pub entry_count: i64,
    pub last_activity: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub slug: Option<String>,
    pub created_at: String,
    pub last_seen_at: String,
    pub entry_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SessionsPage {
    pub sessions: Vec<SessionInfo>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntryPreview {
    pub id: i64,
    pub message_type: String,
    pub content_type: String,
    pub content_preview: String,
    pub tool_name: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntriesPage {
    pub entries: Vec<EntryPreview>,
    pub cursor: Option<String>,
    pub has_more: bool,
}

// ── Stats types (dashboard) ─────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatsResponse {
    pub totals: StatsTotals,
    pub embeddings: StatsEmbeddings,
    pub activity: Vec<DayActivity>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatsTotals {
    pub entries: i64,
    pub sessions: i64,
    pub projects: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatsEmbeddings {
    pub total: i64,
    pub embedded: i64,
    pub pending: i64,
    pub provider: Option<String>,
    pub model: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DayActivity {
    pub day: String,
    pub entries: i64,
}

// ── SSE stream types ────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamEntry {
    pub id: i64,
    pub session_id: String,
    pub message_type: String,
    pub content_type: String,
    pub content_preview: String,
    pub project_path: Option<String>,
    pub tool_name: Option<String>,
    pub created_at: String,
}
