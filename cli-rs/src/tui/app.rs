use std::sync::Arc;

use color_eyre::eyre::Result;
use crossterm::event::{Event as CtEvent, EventStream, KeyCode, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::{prelude::*, widgets::Tabs as TabsWidget};
use tokio::sync::mpsc;

use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;

use super::event::*;
use super::sse;
use super::tabs::browse::BrowseTab;
use super::tabs::graph::GraphTab;
use super::tabs::live::LiveTab;
use super::tabs::search::SearchTab;
use super::tabs::stats::StatsTab;
use super::tabs::{Tab, TabComponent};

pub struct App {
    active_tab: Tab,
    should_quit: bool,
    client: Arc<MemlayerClient>,
    config: Config,

    browse: BrowseTab,
    search: SearchTab,
    live: LiveTab,
    stats: StatsTab,
    graph: GraphTab,

    status_message: String,
    tick_count: u32,
}

impl App {
    pub fn new(config: Config) -> Self {
        let client = Arc::new(MemlayerClient::new(&config));
        App {
            active_tab: Tab::Browse,
            should_quit: false,
            client,
            config,
            browse: BrowseTab::new(),
            search: SearchTab::new(),
            live: LiveTab::new(),
            stats: StatsTab::new(),
            graph: GraphTab::new(),
            status_message: String::new(),
            tick_count: 0,
        }
    }

    pub async fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AppEvent>();

        // Start SSE client task
        let sse_tx = event_tx.clone();
        let base_url = self.config.server_url.clone();
        let auth_token = self.config.auth_token.clone();
        tokio::spawn(async move {
            sse::run_sse_client(base_url, auth_token, sse_tx).await;
        });

        // Activate initial tab
        if let Some(action) = self.active_tab_mut().on_activate() {
            self.dispatch_action(action, &event_tx).await;
        }

        // Terminal event stream
        let mut reader = EventStream::new();
        let mut tick_interval = tokio::time::interval(std::time::Duration::from_millis(250));

        // Main loop
        loop {
            // Render
            terminal.draw(|frame| self.render(frame))?;

            // Wait for next event
            tokio::select! {
                Some(ct_event) = reader.next() => {
                    if let Ok(event) = ct_event {
                        self.handle_terminal_event(event, &event_tx).await;
                    }
                }
                Some(app_event) = event_rx.recv() => {
                    self.handle_app_event(app_event, &event_tx).await;
                }
                _ = tick_interval.tick() => {
                    self.handle_tick(&event_tx).await;
                }
            }

            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    fn active_tab_mut(&mut self) -> &mut dyn TabComponent {
        match self.active_tab {
            Tab::Browse => &mut self.browse,
            Tab::Search => &mut self.search,
            Tab::Live => &mut self.live,
            Tab::Stats => &mut self.stats,
            Tab::Graph => &mut self.graph,
        }
    }

    async fn handle_terminal_event(
        &mut self,
        event: CtEvent,
        tx: &mpsc::UnboundedSender<AppEvent>,
    ) {
        if let CtEvent::Key(key) = &event {
            // Only handle key press events (not release)
            if key.kind != KeyEventKind::Press {
                return;
            }

            // Global keybindings
            match key.code {
                KeyCode::Char('q')
                    if !matches!(self.active_tab, Tab::Search | Tab::Live)
                        || key.modifiers.contains(KeyModifiers::CONTROL) =>
                {
                    self.should_quit = true;
                    return;
                }
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.should_quit = true;
                    return;
                }
                KeyCode::Tab if !self.is_input_focused() => {
                    self.switch_tab((self.active_tab.index() + 1) % 5, tx).await;
                    return;
                }
                KeyCode::BackTab if !self.is_input_focused() => {
                    self.switch_tab((self.active_tab.index() + 4) % 5, tx).await;
                    return;
                }
                KeyCode::Char('1') if !self.is_input_focused() => {
                    self.switch_tab(0, tx).await;
                    return;
                }
                KeyCode::Char('2') if !self.is_input_focused() => {
                    self.switch_tab(1, tx).await;
                    return;
                }
                KeyCode::Char('3') if !self.is_input_focused() => {
                    self.switch_tab(2, tx).await;
                    return;
                }
                KeyCode::Char('4') if !self.is_input_focused() => {
                    self.switch_tab(3, tx).await;
                    return;
                }
                KeyCode::Char('5') if !self.is_input_focused() => {
                    self.switch_tab(4, tx).await;
                    return;
                }
                _ => {}
            }
        }

        // Delegate to active tab
        if let CtEvent::Key(key) = event {
            if key.kind == KeyEventKind::Press {
                if let Some(action) = self.active_tab_mut().handle_key(key) {
                    self.dispatch_action(action, tx).await;
                }
            }
        }
    }

    fn is_input_focused(&self) -> bool {
        match self.active_tab {
            Tab::Search => true, // Search always captures keys when active
            Tab::Live => false,  // Live filter uses / to focus
            Tab::Graph => self.graph.filter_focused,
            _ => false,
        }
    }

    async fn handle_app_event(
        &mut self,
        event: AppEvent,
        tx: &mpsc::UnboundedSender<AppEvent>,
    ) {
        // Dispatch to all tabs (they filter by event type internally)
        self.browse.handle_event(&event);
        self.search.handle_event(&event);
        self.live.handle_event(&event);
        self.stats.handle_event(&event);
        self.graph.handle_event(&event);

        // Handle actions from events
        if let AppEvent::SseStatus(ref status) = event {
            self.status_message = match status {
                SseConnectionStatus::Connected => "SSE: Connected".to_string(),
                SseConnectionStatus::Disconnected(e) => format!("SSE: {e}"),
                SseConnectionStatus::Reconnecting => "SSE: Reconnecting...".to_string(),
            };
        }

        let _ = tx; // suppress unused warning
    }

    async fn handle_tick(&mut self, tx: &mpsc::UnboundedSender<AppEvent>) {
        self.tick_count += 1;

        // Debounced search (fire every other tick = ~500ms)
        if self.tick_count % 2 == 0 {
            if let Some(action) = self.search.check_debounce() {
                self.dispatch_action(action, tx).await;
            }
            if let Some(action) = self.graph.check_debounce() {
                self.dispatch_action(action, tx).await;
            }
        }

        // Stats auto-refresh
        if matches!(self.active_tab, Tab::Stats) && self.stats.needs_refresh() {
            self.dispatch_action(Action::FetchStats, tx).await;
        }

        // Graph auto-refresh
        if matches!(self.active_tab, Tab::Graph) && self.graph.needs_refresh() {
            self.dispatch_action(Action::FetchGraphData, tx).await;
        }
    }

    async fn switch_tab(&mut self, index: usize, tx: &mpsc::UnboundedSender<AppEvent>) {
        let new_tab = Tab::from_index(index);
        if new_tab == self.active_tab {
            return;
        }
        self.active_tab_mut().on_deactivate();
        self.active_tab = new_tab;
        if let Some(action) = self.active_tab_mut().on_activate() {
            self.dispatch_action(action, tx).await;
        }
    }

    async fn dispatch_action(&self, action: Action, tx: &mpsc::UnboundedSender<AppEvent>) {
        let client = self.client.clone();
        let tx = tx.clone();

        match action {
            Action::Quit => { /* handled at a higher level */ }
            Action::SwitchTab(_) => { /* handled at a higher level */ }
            Action::FetchProjects => {
                tokio::spawn(async move {
                    let result = client.get_projects().await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::Projects(result)))
                        .ok();
                });
            }
            Action::FetchSessions(project_path) => {
                tokio::spawn(async move {
                    let result = client.get_sessions(Some(&project_path), 0, 100).await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::Sessions(result)))
                        .ok();
                });
            }
            Action::FetchEntries(session_id, cursor) => {
                tokio::spawn(async move {
                    let result = client.get_session_entries(&session_id, cursor, 50).await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::Entries(result)))
                        .ok();
                });
            }
            Action::FetchStats => {
                let tx2 = tx.clone();
                let client2 = client.clone();
                tokio::spawn(async move {
                    let result = client.get_stats().await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::Stats(result)))
                        .ok();
                });
                tokio::spawn(async move {
                    let result = client2.get_health().await;
                    tx2.send(AppEvent::ApiResponse(ApiResponsePayload::Health(result)))
                        .ok();
                });
            }
            Action::FetchHealth => {
                tokio::spawn(async move {
                    let result = client.get_health().await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::Health(result)))
                        .ok();
                });
            }
            Action::RunSearch(req) => {
                tokio::spawn(async move {
                    let result = client.search(&req).await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::Search(result)))
                        .ok();
                });
            }
            Action::FetchFullEntry(session_id, _entry_id) => {
                tokio::spawn(async move {
                    let result = client.get_session_summary(&session_id, 500, None).await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::FullEntry(
                        result,
                    )))
                    .ok();
                });
            }
            Action::FetchGraphData => {
                let tx2 = tx.clone();
                let client2 = client.clone();
                tokio::spawn(async move {
                    let result = client.get_graph_stats().await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::GraphStats(result)))
                        .ok();
                });
                tokio::spawn(async move {
                    let result = client2.get_entities(None, None, None, "active", 50, 0).await;
                    tx2.send(AppEvent::ApiResponse(ApiResponsePayload::GraphEntities(result)))
                        .ok();
                });
            }
            Action::FetchEntityDetail(entity_id) => {
                tokio::spawn(async move {
                    let result = client.get_entity(entity_id).await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::GraphEntityDetail(result)))
                        .ok();
                });
            }
            Action::FetchSearchedEntities { query, offset } => {
                tokio::spawn(async move {
                    let result = client
                        .get_entities(query.as_deref(), None, None, "active", 50, offset)
                        .await;
                    tx.send(AppEvent::ApiResponse(ApiResponsePayload::GraphEntities(
                        result,
                    )))
                    .ok();
                });
            }
        }
    }

    fn render(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // tab bar
                Constraint::Min(0),   // content
                Constraint::Length(1), // status bar
            ])
            .split(frame.area());

        // Tab bar
        let tab_titles: Vec<&str> = Tab::ALL.iter().map(|t| t.title()).collect();
        let tabs = TabsWidget::new(tab_titles)
            .select(self.active_tab.index())
            .highlight_style(Style::default().fg(Color::Cyan).bold())
            .divider(" | ");
        frame.render_widget(tabs, chunks[0]);

        // Tab content
        match self.active_tab {
            Tab::Browse => self.browse.render(frame, chunks[1]),
            Tab::Search => self.search.render(frame, chunks[1]),
            Tab::Live => self.live.render(frame, chunks[1]),
            Tab::Stats => self.stats.render(frame, chunks[1]),
            Tab::Graph => self.graph.render(frame, chunks[1]),
        }

        // Status bar
        let status = Line::from(vec![
            Span::styled(
                " Tab/Shift+Tab: switch | q: quit | ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled(&self.status_message, Style::default().fg(Color::DarkGray)),
        ]);
        frame.render_widget(status, chunks[2]);
    }
}
