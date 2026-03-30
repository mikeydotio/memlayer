use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use memlayer_common::api_types::{EntitiesPage, EntityDetail, GraphStatsResponse};

use super::TabComponent;
use crate::tui::event::{Action, ApiResponsePayload, AppEvent};

struct BreadcrumbEntry {
    entity_id: i64,
    entity_name: String,
    detail: EntityDetail,
    nav_position: usize,
}

pub struct GraphTab {
    // Data
    stats: Option<GraphStatsResponse>,
    entities: Option<EntitiesPage>,
    list_state: ListState,
    loading: bool,
    last_refresh: std::time::Instant,
    detail_scroll: u16,

    // Filter
    filter_input: String,
    filter_cursor: usize,
    pub filter_focused: bool,
    pending_search: bool,
    last_filter_query: String,

    // Pagination
    current_offset: u32,

    // Breadcrumb navigation
    breadcrumb: Vec<BreadcrumbEntry>,
    neighbor_nav: usize,
    pending_drill_in: Option<i64>,

    // Preview detail when browsing the list (not drilling in)
    preview_detail: Option<EntityDetail>,
}

impl GraphTab {
    pub fn new() -> Self {
        GraphTab {
            stats: None,
            entities: None,
            list_state: ListState::default(),
            loading: false,
            last_refresh: std::time::Instant::now() - std::time::Duration::from_secs(60),
            detail_scroll: 0,
            filter_input: String::new(),
            filter_cursor: 0,
            filter_focused: false,
            pending_search: false,
            last_filter_query: String::new(),
            current_offset: 0,
            breadcrumb: Vec::new(),
            neighbor_nav: 0,
            pending_drill_in: None,
            preview_detail: None,
        }
    }

    pub fn needs_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= std::time::Duration::from_secs(30)
    }

    pub fn check_debounce(&mut self) -> Option<Action> {
        if self.pending_search && self.filter_input != self.last_filter_query {
            self.pending_search = false;
            self.last_filter_query = self.filter_input.clone();
            self.current_offset = 0;
            let query = if self.filter_input.is_empty() {
                None
            } else {
                Some(self.filter_input.clone())
            };
            Some(Action::FetchSearchedEntities {
                query,
                offset: 0,
            })
        } else {
            self.pending_search = false;
            None
        }
    }

    fn current_detail(&self) -> Option<&EntityDetail> {
        if let Some(entry) = self.breadcrumb.last() {
            Some(&entry.detail)
        } else {
            self.preview_detail.as_ref()
        }
    }

    fn relationship_count(&self) -> usize {
        self.current_detail()
            .map(|d| d.relationships.len())
            .unwrap_or(0)
    }

    fn handle_filter_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char(c) => {
                self.filter_input.insert(self.filter_cursor, c);
                self.filter_cursor += 1;
                self.pending_search = true;
                None
            }
            KeyCode::Backspace => {
                if self.filter_cursor > 0 {
                    self.filter_cursor -= 1;
                    self.filter_input.remove(self.filter_cursor);
                    self.pending_search = true;
                }
                None
            }
            KeyCode::Left => {
                self.filter_cursor = self.filter_cursor.saturating_sub(1);
                None
            }
            KeyCode::Right => {
                self.filter_cursor = (self.filter_cursor + 1).min(self.filter_input.len());
                None
            }
            KeyCode::Enter => {
                // Immediate search
                self.filter_focused = false;
                if self.filter_input != self.last_filter_query {
                    self.last_filter_query = self.filter_input.clone();
                    self.current_offset = 0;
                    let query = if self.filter_input.is_empty() {
                        None
                    } else {
                        Some(self.filter_input.clone())
                    };
                    Some(Action::FetchSearchedEntities {
                        query,
                        offset: 0,
                    })
                } else {
                    None
                }
            }
            KeyCode::Esc | KeyCode::Down => {
                self.filter_focused = false;
                None
            }
            _ => None,
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('/') | KeyCode::Char('i') => {
                self.filter_focused = true;
                None
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ref entities) = self.entities {
                    let len = entities.entities.len();
                    if len > 0 {
                        let i = self.list_state.selected().unwrap_or(0);
                        let next = if i >= len - 1 { 0 } else { i + 1 };
                        self.list_state.select(Some(next));
                        self.detail_scroll = 0;
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
                // Drill into selected entity
                if let Some(ref entities) = self.entities {
                    if let Some(i) = self.list_state.selected() {
                        if i < entities.entities.len() {
                            let entity_id = entities.entities[i].id;
                            self.pending_drill_in = Some(entity_id);
                            self.detail_scroll = 0;
                            return Some(Action::FetchEntityDetail(entity_id));
                        }
                    }
                }
                None
            }
            KeyCode::Char('n') => {
                // Next page
                if let Some(ref page) = self.entities {
                    if (self.current_offset + 50) < page.total as u32 {
                        self.current_offset += 50;
                        let query = if self.filter_input.is_empty() {
                            None
                        } else {
                            Some(self.filter_input.clone())
                        };
                        return Some(Action::FetchSearchedEntities {
                            query,
                            offset: self.current_offset,
                        });
                    }
                }
                None
            }
            KeyCode::Char('p') => {
                // Previous page
                if self.current_offset > 0 {
                    self.current_offset = self.current_offset.saturating_sub(50);
                    let query = if self.filter_input.is_empty() {
                        None
                    } else {
                        Some(self.filter_input.clone())
                    };
                    return Some(Action::FetchSearchedEntities {
                        query,
                        offset: self.current_offset,
                    });
                }
                None
            }
            KeyCode::Char('r') => {
                self.loading = true;
                self.last_refresh = std::time::Instant::now();
                Some(Action::FetchGraphData)
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

    fn handle_breadcrumb_key(&mut self, key: KeyEvent) -> Option<Action> {
        let rel_count = self.relationship_count();
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                if rel_count > 0 {
                    self.neighbor_nav = if self.neighbor_nav >= rel_count - 1 {
                        0
                    } else {
                        self.neighbor_nav + 1
                    };
                }
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if rel_count > 0 {
                    self.neighbor_nav = if self.neighbor_nav == 0 {
                        rel_count.saturating_sub(1)
                    } else {
                        self.neighbor_nav - 1
                    };
                }
                None
            }
            KeyCode::Enter => {
                // Drill into selected relationship
                if let Some(entry) = self.breadcrumb.last_mut() {
                    entry.nav_position = self.neighbor_nav;
                    if let Some(rel) = entry.detail.relationships.get(self.neighbor_nav) {
                        let entity_id = rel.related_entity.id;
                        self.pending_drill_in = Some(entity_id);
                        self.detail_scroll = 0;
                        return Some(Action::FetchEntityDetail(entity_id));
                    }
                }
                None
            }
            KeyCode::Esc => {
                // Pop breadcrumb
                self.breadcrumb.pop();
                self.detail_scroll = 0;
                if let Some(entry) = self.breadcrumb.last() {
                    self.neighbor_nav = entry.nav_position;
                } else {
                    self.neighbor_nav = 0;
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
            KeyCode::Char('r') => {
                self.loading = true;
                self.last_refresh = std::time::Instant::now();
                Some(Action::FetchGraphData)
            }
            _ => None,
        }
    }

    fn render_entity_detail<'a>(&self, detail: &'a EntityDetail, lines: &mut Vec<Line<'a>>) {
        let e = &detail.entity;
        lines.push(Line::from(vec![
            Span::styled(
                &e.canonical_name,
                Style::default().fg(Color::White).bold(),
            ),
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
            lines.push(Line::from(Span::styled(
                desc.as_str(),
                Style::default().fg(Color::White),
            )));
        }

        // Relationships
        if !detail.relationships.is_empty() {
            lines.push(Line::from(""));
            let in_breadcrumb = !self.breadcrumb.is_empty();
            let header = if in_breadcrumb {
                "Relationships (j/k=nav, Enter=drill in):"
            } else {
                "Relationships:"
            };
            lines.push(Line::from(Span::styled(
                header,
                Style::default().fg(Color::Yellow).bold(),
            )));
            for (idx, r) in detail.relationships.iter().enumerate() {
                let arrow = if r.direction == "outgoing" {
                    "→"
                } else {
                    "←"
                };
                let rel_color = match r.relationship_type.as_str() {
                    "supersedes" | "contradicts" => Color::Red,
                    "depends_on" => Color::Yellow,
                    "supports" | "implements" => Color::Green,
                    _ => Color::Blue,
                };
                let selected = in_breadcrumb && idx == self.neighbor_nav;
                let prefix = if selected { "▸ " } else { "  " };
                let line_style = if selected {
                    Style::default().fg(Color::Black).bg(Color::Cyan)
                } else {
                    Style::default()
                };
                lines.push(Line::from(vec![
                    Span::styled(prefix, line_style),
                    Span::styled(format!("{arrow} "), line_style),
                    Span::styled(
                        &r.relationship_type,
                        if selected {
                            line_style
                        } else {
                            Style::default().fg(rel_color)
                        },
                    ),
                    Span::styled(": ", line_style),
                    Span::styled(
                        &r.related_entity.canonical_name,
                        if selected {
                            line_style
                        } else {
                            Style::default().fg(Color::White)
                        },
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
    }

    fn render_graph_overview<'a>(&self, stats: &'a GraphStatsResponse, lines: &mut Vec<Line<'a>>) {
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
            Span::styled(
                "Active relationships: ",
                Style::default().fg(Color::Cyan),
            ),
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

        // Top entities
        if !stats.top_entities.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Top Entities:",
                Style::default().fg(Color::Yellow).bold(),
            )));
            for e in &stats.top_entities {
                let type_color = match e.entity_type.as_str() {
                    "decision" => Color::Yellow,
                    "bug" => Color::Red,
                    "tool" | "library" => Color::Green,
                    "pattern" | "architecture" => Color::Magenta,
                    _ => Color::Blue,
                };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  [{}] ", &e.entity_type[..e.entity_type.len().min(4)]),
                        Style::default().fg(type_color),
                    ),
                    Span::styled(&e.name, Style::default().fg(Color::White)),
                ]));
            }
        }
    }
}

impl TabComponent for GraphTab {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.filter_focused {
            self.handle_filter_key(key)
        } else if self.breadcrumb.is_empty() {
            self.handle_list_key(key)
        } else {
            self.handle_breadcrumb_key(key)
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
                    if let Some(pending_id) = self.pending_drill_in {
                        if detail.entity.id == pending_id {
                            // This is a drill-in — push breadcrumb
                            self.breadcrumb.push(BreadcrumbEntry {
                                entity_id: detail.entity.id,
                                entity_name: detail.entity.canonical_name.clone(),
                                detail: detail.clone(),
                                nav_position: 0,
                            });
                            self.neighbor_nav = 0;
                            self.pending_drill_in = None;
                            self.detail_scroll = 0;
                            return;
                        }
                    }
                    // Normal list preview
                    self.preview_detail = Some(detail.clone());
                    self.pending_drill_in = None;
                    self.detail_scroll = 0;
                }
                ApiResponsePayload::GraphEntityDetail(Err(_)) => {
                    self.pending_drill_in = None;
                }
                _ => {}
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(area);

        // === Left panel ===
        let left_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // filter bar
                Constraint::Min(0),   // entity list
                Constraint::Length(1), // pagination
            ])
            .split(chunks[0]);

        // Filter bar
        let filter_style = if self.filter_focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let filter_block = Block::default()
            .borders(Borders::ALL)
            .title(" / Filter ")
            .border_style(filter_style);
        let filter_text = Paragraph::new(self.filter_input.as_str()).block(filter_block);
        frame.render_widget(filter_text, left_chunks[0]);

        if self.filter_focused {
            frame.set_cursor_position(Position::new(
                left_chunks[0].x + 1 + self.filter_cursor as u16,
                left_chunks[0].y + 1,
            ));
        }

        // Entity list or relationship list (when in breadcrumb)
        if !self.breadcrumb.is_empty() {
            // Show relationships of current entity as navigable list
            let entry = self.breadcrumb.last().unwrap();
            let rel_block = Block::default()
                .borders(Borders::ALL)
                .title(" Relationships (j/k=nav) ")
                .border_style(Style::default().fg(Color::Cyan));

            if entry.detail.relationships.is_empty() {
                let empty = Paragraph::new("No relationships").block(rel_block);
                frame.render_widget(empty, left_chunks[1]);
            } else {
                let items: Vec<ListItem> = entry
                    .detail
                    .relationships
                    .iter()
                    .enumerate()
                    .map(|(idx, r)| {
                        let selected = idx == self.neighbor_nav;
                        let style = if selected {
                            Style::default().fg(Color::Black).bg(Color::Cyan)
                        } else {
                            Style::default()
                        };
                        let arrow = if r.direction == "outgoing" {
                            "→"
                        } else {
                            "←"
                        };
                        let rel_color = match r.relationship_type.as_str() {
                            "supersedes" | "contradicts" => Color::Red,
                            "depends_on" => Color::Yellow,
                            "supports" | "implements" => Color::Green,
                            _ => Color::Blue,
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("{arrow} "),
                                if selected {
                                    style
                                } else {
                                    Style::default().fg(Color::DarkGray)
                                },
                            ),
                            Span::styled(
                                format!("{} ", r.relationship_type),
                                if selected {
                                    style
                                } else {
                                    Style::default().fg(rel_color)
                                },
                            ),
                            Span::styled(&r.related_entity.canonical_name, style),
                        ]))
                    })
                    .collect();
                let list = List::new(items).block(rel_block);
                frame.render_widget(list, left_chunks[1]);
            }
        } else {
            // Normal entity list
            let list_title = if self.loading && self.entities.is_none() {
                " Entities (loading...) ".to_string()
            } else {
                " Entities (j/k=nav, Enter=drill) ".to_string()
            };
            let left_block = Block::default()
                .borders(Borders::ALL)
                .title(list_title)
                .border_style(Style::default().fg(Color::Cyan));

            if self.loading && self.entities.is_none() {
                let loading = Paragraph::new("Loading...").block(left_block);
                frame.render_widget(loading, left_chunks[1]);
            } else if let Some(ref page) = self.entities {
                if page.entities.is_empty() {
                    let empty = Paragraph::new(
                        "No entities found.\nEnable extraction to populate the graph.",
                    )
                    .block(left_block);
                    frame.render_widget(empty, left_chunks[1]);
                } else {
                    let items: Vec<ListItem> = page
                        .entities
                        .iter()
                        .enumerate()
                        .map(|(i, e)| {
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
                                    format!(
                                        "[{}] ",
                                        &e.entity_type[..e.entity_type.len().min(4)]
                                    ),
                                    if selected {
                                        style
                                    } else {
                                        Style::default().fg(type_color)
                                    },
                                ),
                                Span::styled(&e.canonical_name, style),
                                Span::styled(
                                    format!(" ({})", e.mention_count),
                                    if selected {
                                        style
                                    } else {
                                        Style::default().fg(Color::DarkGray)
                                    },
                                ),
                            ]))
                        })
                        .collect();
                    let list = List::new(items).block(left_block);
                    frame.render_widget(list, left_chunks[1]);
                }
            } else {
                let no_data = Paragraph::new("Press 'r' to load").block(left_block);
                frame.render_widget(no_data, left_chunks[1]);
            }
        }

        // Pagination bar
        if let Some(ref page) = self.entities {
            if page.total > 50 {
                let current_page = (self.current_offset / 50) + 1;
                let total_pages = ((page.total as u32).saturating_sub(1) / 50) + 1;
                let pag_text = format!(" Page {current_page}/{total_pages}  n=next p=prev ");
                let pag = Paragraph::new(pag_text).style(Style::default().fg(Color::DarkGray));
                frame.render_widget(pag, left_chunks[2]);
            }
        }

        // === Right panel ===
        let right_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(if self.breadcrumb.is_empty() { 0 } else { 1 }),
                Constraint::Min(0),
            ])
            .split(chunks[1]);

        // Breadcrumb bar
        if !self.breadcrumb.is_empty() {
            let mut crumbs: Vec<Span> = Vec::new();
            for (i, entry) in self.breadcrumb.iter().enumerate() {
                if i > 0 {
                    crumbs.push(Span::styled(
                        " > ",
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                let is_current = i == self.breadcrumb.len() - 1;
                let style = if is_current {
                    Style::default().fg(Color::White).bold()
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                crumbs.push(Span::styled(&entry.entity_name, style));
            }
            crumbs.push(Span::styled(
                "  (Esc=back)",
                Style::default().fg(Color::DarkGray),
            ));
            let crumb_line = Paragraph::new(Line::from(crumbs))
                .style(Style::default().bg(Color::Rgb(30, 30, 30)));
            frame.render_widget(crumb_line, right_chunks[0]);
        }

        // Detail pane
        let right_block = Block::default()
            .borders(Borders::ALL)
            .title(" Detail (d/u=scroll) ")
            .border_style(Style::default().fg(Color::Cyan));

        let mut lines: Vec<Line> = Vec::new();

        if let Some(detail) = self.current_detail() {
            self.render_entity_detail(detail, &mut lines);
        } else if let Some(ref stats) = self.stats {
            self.render_graph_overview(stats, &mut lines);
        } else {
            lines.push(Line::from("Select an entity or press 'r' to load"));
        }

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text)
            .block(right_block)
            .wrap(Wrap { trim: false })
            .scroll((self.detail_scroll, 0));
        frame.render_widget(paragraph, right_chunks[1]);
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
