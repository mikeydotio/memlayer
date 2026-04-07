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
    #[serde(default)]
    pub content_truncated: bool,
    #[serde(default)]
    pub content_length: i64,
    #[serde(default)]
    pub graph_boost: f64,
    #[serde(default)]
    pub related_entities: Option<Vec<EntityRef>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expand_graph: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph_weight: Option<f64>,
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

// ── Recent entries types ────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentEntry {
    pub id: i64,
    pub session_id: String,
    pub message_type: String,
    pub content_type: String,
    pub content_preview: String,
    pub tool_name: Option<String>,
    pub created_at: String,
    pub project_path: Option<String>,
    pub slug: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentEntriesPage {
    pub entries: Vec<RecentEntry>,
    pub total: i64,
    pub limit: u32,
    pub machine_id: Option<String>,
}

// ── Stats types (dashboard) ─────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StatsResponse {
    pub totals: StatsTotals,
    pub embeddings: StatsEmbeddings,
    pub activity: Vec<DayActivity>,
    #[serde(default)]
    pub contributors: Vec<ContributorInfo>,
    #[serde(default)]
    pub database_size_bytes: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContributorInfo {
    pub machine_id: String,
    pub session_count: i64,
    pub entry_count: i64,
    pub last_active: String,
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

// ── Knowledge graph types ───────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityRef {
    pub id: i64,
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityInfo {
    pub id: i64,
    pub canonical_name: String,
    pub entity_type: String,
    pub description: Option<String>,
    pub project_path: Option<String>,
    pub status: String,
    pub confidence: f64,
    pub mention_count: i64,
    pub first_seen_at: String,
    pub last_seen_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AliasInfo {
    pub id: i64,
    pub alias: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MentionInfo {
    pub id: i64,
    pub entry_id: i64,
    pub session_id: String,
    pub mention_text: Option<String>,
    pub context_snippet: Option<String>,
    pub confidence: f64,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelationshipInfo {
    pub id: i64,
    pub direction: String,
    pub related_entity: EntityInfo,
    pub relationship_type: String,
    pub description: Option<String>,
    pub confidence: f64,
    pub valid_from: String,
    pub valid_until: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntityDetail {
    pub entity: EntityInfo,
    pub aliases: Vec<AliasInfo>,
    pub mentions: Vec<MentionInfo>,
    pub relationships: Vec<RelationshipInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EntitiesPage {
    pub entities: Vec<EntityInfo>,
    pub total: i64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: i64,
    pub source_id: i64,
    pub target_id: i64,
    pub relationship_type: String,
    pub confidence: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphNeighbors {
    pub center: EntityInfo,
    pub nodes: Vec<EntityInfo>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphStatsResponse {
    pub entities: GraphEntityStats,
    pub relationships: GraphRelStats,
    pub mentions: i64,
    pub extraction: serde_json::Value,
    pub top_entities: Vec<EntityRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphEntityStats {
    pub active: i64,
    pub total: i64,
    pub by_type: std::collections::HashMap<String, i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphRelStats {
    pub active: i64,
    pub by_type: std::collections::HashMap<String, i64>,
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

// ── Version types ──────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionInfo {
    pub server_version: String,
    pub schema_version: u32,
    pub min_client_version: Option<String>,
    pub read_only: bool,
    pub features: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VersionError {
    pub error: String,
    pub detail: String,
    pub server_version: String,
    #[serde(default)]
    pub required_major: Option<u32>,
    #[serde(default)]
    pub min_client_version: Option<String>,
    #[serde(default)]
    pub update_url: Option<String>,
}

/// Server info parsed from response headers.
#[derive(Clone, Debug, Default)]
pub struct ServerInfo {
    pub version: String,
    pub schema_version: u32,
    pub read_only: bool,
    pub min_client_version: Option<String>,
    pub features: Vec<String>,
    pub upgrade_required: bool,
}
