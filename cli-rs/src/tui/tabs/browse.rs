use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use memlayer_common::api_types::*;

use super::TabComponent;
use crate::tui::event::{Action, ApiResponsePayload, AppEvent};
use crate::tui::widgets::entry_detail::EntryDetail;
use crate::tui::widgets::list_nav::ListNav;

#[derive(Clone, PartialEq)]
enum BrowseLevel {
    Projects,
    Sessions,
    Entries,
}

pub struct BrowseTab {
    level: BrowseLevel,
    projects: Vec<ProjectInfo>,
    sessions: Vec<SessionInfo>,
    entries: Vec<EntryPreview>,
    entries_has_more: bool,
    project_nav: ListNav,
    session_nav: ListNav,
    entry_nav: ListNav,
    detail: EntryDetail,
    loading: bool,
    selected_project: Option<String>,
    selected_session: Option<String>,
}

impl BrowseTab {
    pub fn new() -> Self {
        BrowseTab {
            level: BrowseLevel::Projects,
            projects: Vec::new(),
            sessions: Vec::new(),
            entries: Vec::new(),
            entries_has_more: false,
            project_nav: ListNav::new(),
            session_nav: ListNav::new(),
            entry_nav: ListNav::new(),
            detail: EntryDetail::new(),
            loading: false,
            selected_project: None,
            selected_session: None,
        }
    }
}

impl TabComponent for BrowseTab {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                match self.level {
                    BrowseLevel::Projects => self.project_nav.next(),
                    BrowseLevel::Sessions => self.session_nav.next(),
                    BrowseLevel::Entries => self.entry_nav.next(),
                }
                // Update detail for entries
                if self.level == BrowseLevel::Entries {
                    if let Some(entry) = self.entries.get(self.entry_nav.selected) {
                        self.detail.set(
                            format!("[{}] {}", entry.message_type, entry.created_at),
                            entry.content_preview.clone(),
                        );
                    }
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                match self.level {
                    BrowseLevel::Projects => self.project_nav.prev(),
                    BrowseLevel::Sessions => self.session_nav.prev(),
                    BrowseLevel::Entries => self.entry_nav.prev(),
                }
                if self.level == BrowseLevel::Entries {
                    if let Some(entry) = self.entries.get(self.entry_nav.selected) {
                        self.detail.set(
                            format!("[{}] {}", entry.message_type, entry.created_at),
                            entry.content_preview.clone(),
                        );
                    }
                }
                None
            }
            KeyCode::Enter => match self.level {
                BrowseLevel::Projects => {
                    if let Some(project) = self.projects.get(self.project_nav.selected) {
                        self.selected_project = Some(project.project_path.clone());
                        self.level = BrowseLevel::Sessions;
                        self.session_nav = ListNav::new();
                        self.loading = true;
                        return Some(Action::FetchSessions(project.project_path.clone()));
                    }
                    None
                }
                BrowseLevel::Sessions => {
                    if let Some(session) = self.sessions.get(self.session_nav.selected) {
                        self.selected_session = Some(session.session_id.clone());
                        self.level = BrowseLevel::Entries;
                        self.entry_nav = ListNav::new();
                        self.detail.clear();
                        self.loading = true;
                        return Some(Action::FetchEntries(session.session_id.clone(), None));
                    }
                    None
                }
                BrowseLevel::Entries => {
                    // Fetch full entry content
                    if let Some(entry) = self.entries.get(self.entry_nav.selected) {
                        if let Some(ref sid) = self.selected_session {
                            return Some(Action::FetchFullEntry(sid.clone(), entry.id));
                        }
                    }
                    None
                }
            },
            KeyCode::Esc => match self.level {
                BrowseLevel::Sessions => {
                    self.level = BrowseLevel::Projects;
                    self.sessions.clear();
                    None
                }
                BrowseLevel::Entries => {
                    self.level = BrowseLevel::Sessions;
                    self.entries.clear();
                    self.detail.clear();
                    None
                }
                _ => None,
            },
            KeyCode::Char('G') => {
                match self.level {
                    BrowseLevel::Projects => self.project_nav.end(),
                    BrowseLevel::Sessions => self.session_nav.end(),
                    BrowseLevel::Entries => self.entry_nav.end(),
                }
                None
            }
            KeyCode::Char('g') => {
                match self.level {
                    BrowseLevel::Projects => self.project_nav.home(),
                    BrowseLevel::Sessions => self.session_nav.home(),
                    BrowseLevel::Entries => self.entry_nav.home(),
                }
                None
            }
            // Scroll detail
            KeyCode::Char('l') | KeyCode::Right => {
                self.detail.scroll_down();
                None
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.detail.scroll_up();
                None
            }
            _ => None,
        }
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::ApiResponse(payload) = event {
            match payload {
                ApiResponsePayload::Projects(Ok(projects)) => {
                    self.projects = projects.clone();
                    self.project_nav.set_len(projects.len());
                    self.loading = false;
                }
                ApiResponsePayload::Projects(Err(_)) => {
                    self.loading = false;
                }
                ApiResponsePayload::Sessions(Ok(page)) => {
                    self.sessions = page.sessions.clone();
                    self.session_nav.set_len(self.sessions.len());
                    self.loading = false;
                }
                ApiResponsePayload::Sessions(Err(_)) => {
                    self.loading = false;
                }
                ApiResponsePayload::Entries(Ok(page)) => {
                    self.entries = page.entries.clone();
                    self.entries_has_more = page.has_more;
                    self.entry_nav.set_len(self.entries.len());
                    self.loading = false;
                    // Select first entry
                    if let Some(entry) = self.entries.first() {
                        self.detail.set(
                            format!("[{}] {}", entry.message_type, entry.created_at),
                            entry.content_preview.clone(),
                        );
                    }
                }
                ApiResponsePayload::Entries(Err(_)) => {
                    self.loading = false;
                }
                ApiResponsePayload::FullEntry(Ok(summary)) => {
                    // Show full content from first matching message
                    if let Some(msg) = summary.messages.first() {
                        self.detail.set(
                            format!("[{}] {}", msg.message_type, msg.created_at),
                            msg.raw_content.clone(),
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        // Left pane: list
        let title = match self.level {
            BrowseLevel::Projects => " Projects ",
            BrowseLevel::Sessions => {
                " Sessions (Esc=back) "
            }
            BrowseLevel::Entries => {
                " Entries (Esc=back) "
            }
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(Color::Cyan));

        if self.loading {
            let loading = Paragraph::new("Loading...").block(block);
            frame.render_widget(loading, chunks[0]);
        } else {
            match self.level {
                BrowseLevel::Projects => {
                    let items: Vec<ListItem> = self
                        .projects
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            let style = if i == self.project_nav.selected {
                                Style::default().bg(Color::DarkGray).fg(Color::White)
                            } else {
                                Style::default()
                            };
                            ListItem::new(format!(
                                "{} ({} sessions, {} entries)",
                                p.project_path, p.session_count, p.entry_count
                            ))
                            .style(style)
                        })
                        .collect();
                    let list = List::new(items).block(block);
                    frame.render_widget(list, chunks[0]);
                }
                BrowseLevel::Sessions => {
                    let items: Vec<ListItem> = self
                        .sessions
                        .iter()
                        .enumerate()
                        .map(|(i, s)| {
                            let style = if i == self.session_nav.selected {
                                Style::default().bg(Color::DarkGray).fg(Color::White)
                            } else {
                                Style::default()
                            };
                            let slug = s.slug.as_deref().unwrap_or("no-slug");
                            ListItem::new(format!(
                                "{} [{}] ({} entries)",
                                &s.session_id[..8.min(s.session_id.len())],
                                slug,
                                s.entry_count
                            ))
                            .style(style)
                        })
                        .collect();
                    let list = List::new(items).block(block);
                    frame.render_widget(list, chunks[0]);
                }
                BrowseLevel::Entries => {
                    let items: Vec<ListItem> = self
                        .entries
                        .iter()
                        .enumerate()
                        .map(|(i, e)| {
                            let style = if i == self.entry_nav.selected {
                                Style::default().bg(Color::DarkGray).fg(Color::White)
                            } else {
                                Style::default()
                            };
                            let tool = e
                                .tool_name
                                .as_ref()
                                .map(|t| format!(" ({t})"))
                                .unwrap_or_default();
                            let preview: String =
                                e.content_preview.chars().take(60).collect();
                            ListItem::new(format!(
                                "[{}{}] {}",
                                e.message_type, tool, preview
                            ))
                            .style(style)
                        })
                        .collect();
                    let list = List::new(items).block(block);
                    frame.render_widget(list, chunks[0]);
                }
            }
        }

        // Right pane: detail
        self.detail.render(frame, chunks[1]);
    }

    fn on_activate(&mut self) -> Option<Action> {
        if self.projects.is_empty() {
            self.loading = true;
            Some(Action::FetchProjects)
        } else {
            None
        }
    }

    fn on_deactivate(&mut self) {}
}
