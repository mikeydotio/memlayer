use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Scrollable text viewer for entry content.
pub struct EntryDetail {
    pub content: String,
    pub title: String,
    pub scroll: u16,
}

impl EntryDetail {
    pub fn new() -> Self {
        EntryDetail {
            content: String::new(),
            title: String::new(),
            scroll: 0,
        }
    }

    pub fn set(&mut self, title: String, content: String) {
        self.title = title;
        self.content = content;
        self.scroll = 0;
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.title.clear();
        self.scroll = 0;
    }

    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(self.title.as_str())
            .border_style(Style::default().fg(Color::DarkGray));

        let paragraph = Paragraph::new(self.content.as_str())
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        frame.render_widget(paragraph, area);
    }
}
