use memlayer_common::api_types::*;

/// Events from across the application.
#[derive(Debug)]
pub enum AppEvent {
    /// Terminal key/mouse/resize
    Terminal(crossterm::event::Event),
    /// Periodic tick (250ms)
    Tick,
    /// SSE entry received
    SseEntry(StreamEntry),
    /// SSE connection status changed
    SseStatus(SseConnectionStatus),
    /// API response for a request
    ApiResponse(ApiResponsePayload),
}

#[derive(Debug, Clone)]
pub enum SseConnectionStatus {
    Connected,
    Disconnected(String),
    Reconnecting,
}

#[derive(Debug)]
pub enum ApiResponsePayload {
    Projects(Result<Vec<ProjectInfo>, String>),
    Sessions(Result<SessionsPage, String>),
    Entries(Result<EntriesPage, String>),
    Stats(Result<StatsResponse, String>),
    Health(Result<serde_json::Value, String>),
    Search(Result<SearchResponse, String>),
    FullEntry(Result<SessionSummary, String>),
}

/// Actions that tabs can request from the app.
#[derive(Debug)]
pub enum Action {
    Quit,
    SwitchTab(usize),
    FetchProjects,
    FetchSessions(String),
    FetchEntries(String, Option<i64>),
    FetchStats,
    FetchHealth,
    RunSearch(SearchRequest),
    FetchFullEntry(String, i64),
}
