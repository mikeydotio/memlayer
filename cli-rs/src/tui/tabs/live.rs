use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph},
};

use memlayer_common::api_types::StreamEntry;

use super::TabComponent;
use crate::tui::event::{Action, AppEvent, SseConnectionStatus};

const MAX_ENTRIES: usize = 500;

pub struct LiveTab {
    entries: Vec<StreamEntry>,
    filter_input: String,
    filter_focused: bool,
    cursor_pos: usize,
    auto_scroll: bool,
    scroll_offset: usize,
    connection_status: String,
}

impl LiveTab {
    pub fn new() -> Self {
        LiveTab {
            entries: Vec::new(),
            filter_input: String::new(),
            filter_focused: false,
            cursor_pos: 0,
            auto_scroll: true,
            scroll_offset: 0,
            connection_status: "Connecting...".to_string(),
        }
    }

    fn filtered_entries(&self) -> Vec<&StreamEntry> {
        if self.filter_input.is_empty() {
            return self.entries.iter().collect();
        }
        let filter = self.filter_input.to_lowercase();
        self.entries
            .iter()
            .filter(|e| {
                e.message_type.to_lowercase().contains(&filter)
                    || e.content_preview.to_lowercase().contains(&filter)
                    || e.project_path
                        .as_ref()
                        .is_some_and(|p| p.to_lowercase().contains(&filter))
                    || e.tool_name
                        .as_ref()
                        .is_some_and(|t| t.to_lowercase().contains(&filter))
            })
            .collect()
    }
}

impl TabComponent for LiveTab {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.filter_focused {
            match key.code {
                KeyCode::Char(c) => {
                    self.filter_input.insert(self.cursor_pos, c);
                    self.cursor_pos += 1;
                    None
                }
                KeyCode::Backspace => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.filter_input.remove(self.cursor_pos);
                    }
                    None
                }
                KeyCode::Esc | KeyCode::Enter => {
                    self.filter_focused = false;
                    None
                }
                _ => None,
            }
        } else {
            match key.code {
                KeyCode::Char('/') => {
                    self.filter_focused = true;
                    None
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.auto_scroll = false;
                    let filtered_len = self.filtered_entries().len();
                    if filtered_len > 0 {
                        self.scroll_offset = (self.scroll_offset + 1).min(filtered_len.saturating_sub(1));
                    }
                    None
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.auto_scroll = false;
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                    None
                }
                KeyCode::Char('G') => {
                    // Jump to bottom, re-enable auto-scroll
                    self.auto_scroll = true;
                    None
                }
                KeyCode::Char('g') => {
                    self.auto_scroll = false;
                    self.scroll_offset = 0;
                    None
                }
                KeyCode::Char('c') => {
                    // Clear filter
                    self.filter_input.clear();
                    self.cursor_pos = 0;
                    None
                }
                _ => None,
            }
        }
    }

    fn handle_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::SseEntry(entry) => {
                self.entries.push(entry.clone());
                // Cap at MAX_ENTRIES
                if self.entries.len() > MAX_ENTRIES {
                    self.entries.drain(0..self.entries.len() - MAX_ENTRIES);
                }
                if self.auto_scroll {
                    let filtered_len = self.filtered_entries().len();
                    self.scroll_offset = filtered_len.saturating_sub(1);
                }
            }
            AppEvent::SseStatus(status) => {
                self.connection_status = match status {
                    SseConnectionStatus::Connected => "Connected".to_string(),
                    SseConnectionStatus::Disconnected(e) => format!("Disconnected: {e}"),
                    SseConnectionStatus::Reconnecting => "Reconnecting...".to_string(),
                };
            }
            _ => {}
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // filter + status
                Constraint::Min(0),   // log
            ])
            .split(area);

        // Filter bar + connection status
        let filter_style = if self.filter_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let status_color = if self.connection_status.starts_with("Connected") {
            Color::Green
        } else if self.connection_status.starts_with("Reconnecting") {
            Color::Yellow
        } else {
            Color::Red
        };
        let filter_title = format!(
            " Filter (/ to focus) | {} | {} entries ",
            self.connection_status,
            self.entries.len()
        );
        let filter_block = Block::default()
            .borders(Borders::ALL)
            .title(Span::styled(filter_title, Style::default().fg(status_color)))
            .border_style(filter_style);
        let filter_text = Paragraph::new(self.filter_input.as_str()).block(filter_block);
        frame.render_widget(filter_text, chunks[0]);

        if self.filter_focused {
            frame.set_cursor_position(Position::new(
                chunks[0].x + 1 + self.cursor_pos as u16,
                chunks[0].y + 1,
            ));
        }

        // Log entries
        let filtered = self.filtered_entries();
        let log_block = Block::default()
            .borders(Borders::ALL)
            .title(if self.auto_scroll {
                " Live Stream (auto-scroll) "
            } else {
                " Live Stream (paused — G to resume) "
            })
            .border_style(Style::default().fg(Color::Cyan));

        let visible_height = chunks[1].height.saturating_sub(2) as usize;
        let start = if self.auto_scroll {
            filtered.len().saturating_sub(visible_height)
        } else {
            self.scroll_offset.min(filtered.len().saturating_sub(visible_height))
        };
        let end = (start + visible_height).min(filtered.len());

        let items: Vec<ListItem> = filtered[start..end]
            .iter()
            .map(|e| {
                let type_color = match e.message_type.as_str() {
                    "user" => Color::Green,
                    "assistant" => Color::Blue,
                    "tool_use" => Color::Yellow,
                    "tool_result" => Color::Magenta,
                    _ => Color::White,
                };
                let tool = e
                    .tool_name
                    .as_ref()
                    .map(|t| format!(" ({t})"))
                    .unwrap_or_default();
                let preview: String = e.content_preview.chars().take(80).collect();
                let line = Line::from(vec![
                    Span::styled(
                        format!("[{}{}] ", e.message_type, tool),
                        Style::default().fg(type_color),
                    ),
                    Span::raw(preview),
                ]);
                ListItem::new(line)
            })
            .collect();

        let list = List::new(items).block(log_block);
        frame.render_widget(list, chunks[1]);
    }

    fn on_activate(&mut self) -> Option<Action> {
        None
    }

    fn on_deactivate(&mut self) {}
}
