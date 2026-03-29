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

pub struct SearchTab {
    input: String,
    cursor_pos: usize,
    results: Vec<SearchResult>,
    total: i64,
    search_ms: f64,
    nav: ListNav,
    detail: EntryDetail,
    input_focused: bool,
    /// Debounce: tracks whether we need to fire a search.
    pending_search: bool,
    last_query: String,
}

impl SearchTab {
    pub fn new() -> Self {
        SearchTab {
            input: String::new(),
            cursor_pos: 0,
            results: Vec::new(),
            total: 0,
            search_ms: 0.0,
            nav: ListNav::new(),
            detail: EntryDetail::new(),
            input_focused: true,
            pending_search: false,
            last_query: String::new(),
        }
    }

    /// Called on tick to check if debounced search should fire.
    pub fn check_debounce(&mut self) -> Option<Action> {
        if self.pending_search && !self.input.is_empty() && self.input != self.last_query {
            self.pending_search = false;
            self.last_query = self.input.clone();
            Some(Action::RunSearch(SearchRequest {
                query: self.input.clone(),
                session_id: None,
                project_path: None,
                limit: 20,
                after: None,
                before: None,
                types: None,
                truncate: None,
            }))
        } else {
            self.pending_search = false;
            None
        }
    }
}

impl TabComponent for SearchTab {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.input_focused {
            match key.code {
                KeyCode::Char(c) => {
                    self.input.insert(self.cursor_pos, c);
                    self.cursor_pos += 1;
                    self.pending_search = true;
                    None
                }
                KeyCode::Backspace => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.input.remove(self.cursor_pos);
                        self.pending_search = true;
                    }
                    None
                }
                KeyCode::Left => {
                    self.cursor_pos = self.cursor_pos.saturating_sub(1);
                    None
                }
                KeyCode::Right => {
                    self.cursor_pos = (self.cursor_pos + 1).min(self.input.len());
                    None
                }
                KeyCode::Enter => {
                    // Immediately search
                    if !self.input.is_empty() {
                        self.last_query = self.input.clone();
                        self.pending_search = false;
                        Some(Action::RunSearch(SearchRequest {
                            query: self.input.clone(),
                            session_id: None,
                            project_path: None,
                            limit: 20,
                            after: None,
                            before: None,
                            types: None,
                            truncate: None,
                        }))
                    } else {
                        None
                    }
                }
                KeyCode::Esc => {
                    self.input_focused = false;
                    None
                }
                KeyCode::Down | KeyCode::Tab => {
                    self.input_focused = false;
                    None
                }
                _ => None,
            }
        } else {
            match key.code {
                KeyCode::Char('/') | KeyCode::Char('i') => {
                    self.input_focused = true;
                    None
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.nav.next();
                    if let Some(r) = self.results.get(self.nav.selected) {
                        self.detail.set(
                            format!("[{}] {}", r.content_type, r.created_at),
                            r.raw_content.clone(),
                        );
                    }
                    None
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.nav.prev();
                    if let Some(r) = self.results.get(self.nav.selected) {
                        self.detail.set(
                            format!("[{}] {}", r.content_type, r.created_at),
                            r.raw_content.clone(),
                        );
                    }
                    None
                }
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
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::ApiResponse(ApiResponsePayload::Search(Ok(resp))) = event {
            self.results = resp.results.clone();
            self.total = resp.total;
            self.search_ms = resp.search_ms;
            self.nav.set_len(self.results.len());
            if let Some(r) = self.results.first() {
                self.detail.set(
                    format!("[{}] {}", r.content_type, r.created_at),
                    r.raw_content.clone(),
                );
            } else {
                self.detail.clear();
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // search input
                Constraint::Min(0),   // results + detail
            ])
            .split(area);

        // Search input
        let input_style = if self.input_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let input_block = Block::default()
            .borders(Borders::ALL)
            .title(" Search (/ to focus) ")
            .border_style(input_style);
        let input_text = Paragraph::new(self.input.as_str()).block(input_block);
        frame.render_widget(input_text, chunks[0]);

        // Cursor
        if self.input_focused {
            frame.set_cursor_position(Position::new(
                chunks[0].x + 1 + self.cursor_pos as u16,
                chunks[0].y + 1,
            ));
        }

        // Results + detail split
        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        // Results list
        let results_title = if self.results.is_empty() {
            " Results ".to_string()
        } else {
            format!(
                " {} results ({}ms) ",
                self.total,
                self.search_ms.round() as i64
            )
        };
        let results_block = Block::default()
            .borders(Borders::ALL)
            .title(results_title)
            .border_style(Style::default().fg(Color::Cyan));

        let items: Vec<ListItem> = self
            .results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let style = if i == self.nav.selected {
                    Style::default().bg(Color::DarkGray).fg(Color::White)
                } else {
                    Style::default()
                };
                let project = r.project_path.as_deref().unwrap_or("?");
                let preview: String = r.raw_content.lines().next().unwrap_or("").chars().take(50).collect();
                ListItem::new(format!(
                    "{:.3} [{}] {} — {}",
                    r.rrf_score, r.content_type, project, preview
                ))
                .style(style)
            })
            .collect();
        let list = List::new(items).block(results_block);
        frame.render_widget(list, bottom[0]);

        // Detail
        self.detail.render(frame, bottom[1]);
    }

    fn on_activate(&mut self) -> Option<Action> {
        self.input_focused = true;
        None
    }

    fn on_deactivate(&mut self) {
        self.input_focused = false;
    }
}
