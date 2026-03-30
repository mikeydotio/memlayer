use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use memlayer_common::api_types::{EntitiesPage, EntityDetail, GraphStatsResponse};

use super::TabComponent;
use crate::tui::event::{Action, ApiResponsePayload, AppEvent};

pub struct GraphTab {
    stats: Option<GraphStatsResponse>,
    entities: Option<EntitiesPage>,
    selected_detail: Option<EntityDetail>,
    list_state: ListState,
    loading: bool,
    last_refresh: std::time::Instant,
    detail_scroll: u16,
}

impl GraphTab {
    pub fn new() -> Self {
        GraphTab {
            stats: None,
            entities: None,
            selected_detail: None,
            list_state: ListState::default(),
            loading: false,
            last_refresh: std::time::Instant::now()
                - std::time::Duration::from_secs(60),
            detail_scroll: 0,
        }
    }

    pub fn needs_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= std::time::Duration::from_secs(30)
    }
}

impl TabComponent for GraphTab {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('r') => {
                self.loading = true;
                self.last_refresh = std::time::Instant::now();
                Some(Action::FetchGraphData)
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ref entities) = self.entities {
                    let len = entities.entities.len();
                    if len > 0 {
                        let i = self.list_state.selected().unwrap_or(0);
                        let next = if i >= len - 1 { 0 } else { i + 1 };
                        self.list_state.select(Some(next));
                        self.detail_scroll = 0;
                        // Fetch detail for selected entity
                        let entity_id = entities.entities[next].id;
                        return Some(Action::FetchEntityDetail(entity_id));
                    }
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ref entities) = self.entities {
                    let len = entities.entities.len();
                    if len > 0 {
                        let i = self.list_state.selected().unwrap_or(0);
                        let next = if i == 0 { len - 1 } else { i - 1 };
                        self.list_state.select(Some(next));
                        self.detail_scroll = 0;
                        let entity_id = entities.entities[next].id;
                        return Some(Action::FetchEntityDetail(entity_id));
                    }
                }
                None
            }
            KeyCode::Enter => {
                // Load detail for current selection
                if let Some(ref entities) = self.entities {
                    if let Some(i) = self.list_state.selected() {
                        if i < entities.entities.len() {
                            let entity_id = entities.entities[i].id;
                            return Some(Action::FetchEntityDetail(entity_id));
                        }
                    }
                }
                None
            }
            KeyCode::Char('d') => {
                self.detail_scroll = self.detail_scroll.saturating_add(1);
                None
            }
            KeyCode::Char('u') => {
                self.detail_scroll = self.detail_scroll.saturating_sub(1);
                None
            }
            _ => None,
        }
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::ApiResponse(payload) = event {
            match payload {
                ApiResponsePayload::GraphStats(Ok(stats)) => {
                    self.stats = Some(stats.clone());
                    self.loading = false;
                }
                ApiResponsePayload::GraphStats(Err(_)) => {
                    self.loading = false;
                }
                ApiResponsePayload::GraphEntities(Ok(page)) => {
                    let had_entities = self.entities.is_some();
                    self.entities = Some(page.clone());
                    if !had_entities && !page.entities.is_empty() {
                        self.list_state.select(Some(0));
                    }
                    self.loading = false;
                }
                ApiResponsePayload::GraphEntities(Err(_)) => {
                    self.loading = false;
                }
                ApiResponsePayload::GraphEntityDetail(Ok(detail)) => {
                    self.selected_detail = Some(detail.clone());
                    self.detail_scroll = 0;
                }
                ApiResponsePayload::GraphEntityDetail(Err(_)) => {}
                _ => {}
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        // Split into left (entity list) and right (detail)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        // --- Left panel: entity list ---
        let left_block = Block::default()
            .borders(Borders::ALL)
            .title(" Entities (j/k=nav, r=refresh) ")
            .border_style(Style::default().fg(Color::Cyan));

        if self.loading && self.entities.is_none() {
            let loading = Paragraph::new("Loading...").block(left_block);
            frame.render_widget(loading, chunks[0]);
        } else if let Some(ref page) = self.entities {
            if page.entities.is_empty() {
                let empty = Paragraph::new("No entities found.\nEnable extraction to populate the graph.")
                    .block(left_block);
                frame.render_widget(empty, chunks[0]);
            } else {
                let items: Vec<ListItem> = page.entities.iter().enumerate().map(|(i, e)| {
                    let selected = self.list_state.selected() == Some(i);
                    let style = if selected {
                        Style::default().fg(Color::Black).bg(Color::Cyan)
                    } else {
                        Style::default()
                    };
                    let type_color = match e.entity_type.as_str() {
                        "decision" => Color::Yellow,
                        "bug" => Color::Red,
                        "tool" | "library" => Color::Green,
                        "pattern" | "architecture" => Color::Magenta,
                        _ => Color::Blue,
                    };
                    ListItem::new(Line::from(vec![
                        Span::styled(
                            format!("[{}] ", &e.entity_type[..e.entity_type.len().min(4)]),
                            if selected { style } else { Style::default().fg(type_color) },
                        ),
                        Span::styled(&e.canonical_name, style),
                        Span::styled(
                            format!(" ({})", e.mention_count),
                            if selected { style } else { Style::default().fg(Color::DarkGray) },
                        ),
                    ]))
                }).collect();

                let list = List::new(items).block(left_block);
                frame.render_widget(list, chunks[0]);
            }
        } else {
            let no_data = Paragraph::new("Press 'r' to load").block(left_block);
            frame.render_widget(no_data, chunks[0]);
        }

        // --- Right panel: detail ---
        let right_block = Block::default()
            .borders(Borders::ALL)
            .title(" Detail (d/u=scroll) ")
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines: Vec<Line> = Vec::new();

        if let Some(ref detail) = self.selected_detail {
            let e = &detail.entity;
            lines.push(Line::from(vec![
                Span::styled(&e.canonical_name, Style::default().fg(Color::White).bold()),
                Span::styled(format!(" #{}", e.id), Style::default().fg(Color::DarkGray)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Type: ", Style::default().fg(Color::Cyan)),
                Span::raw(&e.entity_type),
                Span::raw("  "),
                Span::styled("Status: ", Style::default().fg(Color::Cyan)),
                Span::raw(&e.status),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Mentions: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", e.mention_count)),
                Span::raw("  "),
                Span::styled("Confidence: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{:.0}%", e.confidence * 100.0)),
            ]));
            if let Some(ref desc) = e.description {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(desc.as_str(), Style::default().fg(Color::White))));
            }

            // Relationships
            if !detail.relationships.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Relationships:",
                    Style::default().fg(Color::Yellow).bold(),
                )));
                for r in &detail.relationships {
                    let arrow = if r.direction == "outgoing" { "→" } else { "←" };
                    let rel_color = match r.relationship_type.as_str() {
                        "supersedes" => Color::Red,
                        "contradicts" => Color::Red,
                        "depends_on" => Color::Yellow,
                        "supports" | "implements" => Color::Green,
                        _ => Color::Blue,
                    };
                    lines.push(Line::from(vec![
                        Span::raw(format!("  {arrow} ")),
                        Span::styled(
                            &r.relationship_type,
                            Style::default().fg(rel_color),
                        ),
                        Span::raw(": "),
                        Span::styled(
                            &r.related_entity.canonical_name,
                            Style::default().fg(Color::White),
                        ),
                    ]));
                }
            }

            // Aliases
            if !detail.aliases.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Aliases:",
                    Style::default().fg(Color::Magenta).bold(),
                )));
                for a in &detail.aliases {
                    lines.push(Line::from(format!("  {}", a.alias)));
                }
            }

            // Recent mentions
            if !detail.mentions.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("Recent Mentions ({}):", detail.mentions.len()),
                    Style::default().fg(Color::Green).bold(),
                )));
                for m in detail.mentions.iter().take(10) {
                    let text = m.mention_text.as_deref().unwrap_or("");
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  [{}] ", &m.session_id[..m.session_id.len().min(12)]),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::raw(text),
                    ]));
                }
            }
        } else if let Some(ref stats) = self.stats {
            // Show graph overview when no entity is selected
            lines.push(Line::from(Span::styled(
                "Knowledge Graph Overview",
                Style::default().fg(Color::White).bold(),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Active entities: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", stats.entities.active)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Active relationships: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", stats.relationships.active)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Total mentions: ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}", stats.mentions)),
            ]));

            if !stats.entities.by_type.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Entities by type:",
                    Style::default().fg(Color::Yellow).bold(),
                )));
                let mut sorted: Vec<_> = stats.entities.by_type.iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(a.1));
                for (t, count) in sorted {
                    lines.push(Line::from(format!("  {t}: {count}")));
                }
            }

            if !stats.relationships.by_type.is_empty() {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Relationships by type:",
                    Style::default().fg(Color::Yellow).bold(),
                )));
                let mut sorted: Vec<_> = stats.relationships.by_type.iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(a.1));
                for (t, count) in sorted {
                    lines.push(Line::from(format!("  {t}: {count}")));
                }
            }
        } else {
            lines.push(Line::from("Select an entity or press 'r' to load"));
        }

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text)
            .block(right_block)
            .wrap(Wrap { trim: false })
            .scroll((self.detail_scroll, 0));
        frame.render_widget(paragraph, chunks[1]);
    }

    fn on_activate(&mut self) -> Option<Action> {
        if self.entities.is_none() || self.needs_refresh() {
            self.loading = true;
            self.last_refresh = std::time::Instant::now();
            Some(Action::FetchGraphData)
        } else {
            None
        }
    }

    fn on_deactivate(&mut self) {}
}
