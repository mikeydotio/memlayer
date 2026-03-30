use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use memlayer_common::api_types::StatsResponse;

use super::TabComponent;
use crate::tui::event::{Action, ApiResponsePayload, AppEvent};

pub struct StatsTab {
    stats: Option<StatsResponse>,
    health: Option<serde_json::Value>,
    loading: bool,
    last_refresh: std::time::Instant,
    scroll: u16,
}

impl StatsTab {
    pub fn new() -> Self {
        StatsTab {
            stats: None,
            health: None,
            loading: false,
            last_refresh: std::time::Instant::now()
                - std::time::Duration::from_secs(60), // trigger immediate refresh
            scroll: 0,
        }
    }

    /// Check if we need a periodic refresh (every 30s).
    pub fn needs_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= std::time::Duration::from_secs(30)
    }
}

impl TabComponent for StatsTab {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action> {
        match key.code {
            KeyCode::Char('r') => {
                self.loading = true;
                self.last_refresh = std::time::Instant::now();
                Some(Action::FetchStats)
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll = self.scroll.saturating_add(1);
                None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll = self.scroll.saturating_sub(1);
                None
            }
            _ => None,
        }
    }

    fn handle_event(&mut self, event: &AppEvent) {
        if let AppEvent::ApiResponse(payload) = event {
            match payload {
                ApiResponsePayload::Stats(Ok(stats)) => {
                    self.stats = Some(stats.clone());
                    self.loading = false;
                }
                ApiResponsePayload::Stats(Err(_)) => {
                    self.loading = false;
                }
                ApiResponsePayload::Health(Ok(health)) => {
                    self.health = Some(health.clone());
                }
                _ => {}
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Stats (r=refresh) ")
            .border_style(Style::default().fg(Color::Cyan));

        if self.loading && self.stats.is_none() {
            let loading = Paragraph::new("Loading...").block(block);
            frame.render_widget(loading, area);
            return;
        }

        let mut lines: Vec<Line> = Vec::new();

        if let Some(ref stats) = self.stats {
            lines.push(Line::from(vec![
                Span::styled("Entries: ", Style::default().fg(Color::Cyan).bold()),
                Span::raw(format!("{}", stats.totals.entries)),
                Span::raw("  |  "),
                Span::styled("Sessions: ", Style::default().fg(Color::Cyan).bold()),
                Span::raw(format!("{}", stats.totals.sessions)),
                Span::raw("  |  "),
                Span::styled("Projects: ", Style::default().fg(Color::Cyan).bold()),
                Span::raw(format!("{}", stats.totals.projects)),
            ]));
            lines.push(Line::from(""));

            // Embeddings
            let pct = if stats.embeddings.total > 0 {
                (stats.embeddings.embedded as f64 / stats.embeddings.total as f64 * 100.0) as u32
            } else {
                0
            };
            lines.push(Line::from(vec![
                Span::styled("Embeddings: ", Style::default().fg(Color::Green).bold()),
                Span::raw(format!(
                    "{}/{} ({}%) — {} pending",
                    stats.embeddings.embedded, stats.embeddings.total, pct, stats.embeddings.pending
                )),
            ]));
            if let Some(ref provider) = stats.embeddings.provider {
                lines.push(Line::from(vec![
                    Span::styled("  Provider: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!(
                        "{} ({})",
                        provider,
                        stats.embeddings.model.as_deref().unwrap_or("?")
                    )),
                ]));
            }
            lines.push(Line::from(""));

            // Activity
            if !stats.activity.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Recent Activity:",
                    Style::default().fg(Color::Yellow).bold(),
                )));
                let max_entries = stats
                    .activity
                    .iter()
                    .map(|d| d.entries)
                    .max()
                    .unwrap_or(1)
                    .max(1);
                for day in &stats.activity {
                    let bar_width = ((day.entries as f64 / max_entries as f64) * 30.0) as usize;
                    let bar: String = "█".repeat(bar_width);
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {} ", day.day),
                            Style::default().fg(Color::DarkGray),
                        ),
                        Span::styled(bar, Style::default().fg(Color::Cyan)),
                        Span::raw(format!(" {}", day.entries)),
                    ]));
                }
                lines.push(Line::from(""));
            }

            // Contributors
            if !stats.contributors.is_empty() {
                lines.push(Line::from(Span::styled(
                    "Contributors:",
                    Style::default().fg(Color::Yellow).bold(),
                )));
                for c in &stats.contributors {
                    let last = if c.last_active.is_empty() {
                        "never".to_string()
                    } else {
                        c.last_active.chars().take(19).collect()
                    };
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("  {:<24} ", c.machine_id),
                            Style::default().fg(Color::White),
                        ),
                        Span::styled(
                            format!("{:>6} entries  ", c.entry_count),
                            Style::default().fg(Color::Cyan),
                        ),
                        Span::styled(
                            format!("{:>4} sessions  ", c.session_count),
                            Style::default().fg(Color::Green),
                        ),
                        Span::styled(
                            format!("last: {last}"),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                }
                lines.push(Line::from(""));
            }

            // Database size
            if let Some(size_bytes) = stats.database_size_bytes {
                let (size_val, unit) = if size_bytes >= 1_073_741_824 {
                    (size_bytes as f64 / 1_073_741_824.0, "GB")
                } else {
                    (size_bytes as f64 / 1_048_576.0, "MB")
                };
                lines.push(Line::from(vec![
                    Span::styled("Database Size: ", Style::default().fg(Color::Cyan).bold()),
                    Span::raw(format!("{size_val:.1} {unit}")),
                ]));
                lines.push(Line::from(""));
            }
        }

        if let Some(ref health) = self.health {
            lines.push(Line::from(Span::styled(
                "Server Health:",
                Style::default().fg(Color::Magenta).bold(),
            )));
            let health_str = serde_json::to_string_pretty(health).unwrap_or_default();
            for line in health_str.lines() {
                lines.push(Line::from(Span::styled(
                    format!("  {line}"),
                    Style::default().fg(Color::DarkGray),
                )));
            }
        }

        let text = Text::from(lines);
        let paragraph = Paragraph::new(text)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));
        frame.render_widget(paragraph, area);
    }

    fn on_activate(&mut self) -> Option<Action> {
        if self.stats.is_none() || self.needs_refresh() {
            self.loading = true;
            self.last_refresh = std::time::Instant::now();
            Some(Action::FetchStats)
        } else {
            None
        }
    }

    fn on_deactivate(&mut self) {}
}
