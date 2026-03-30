pub mod browse;
pub mod graph;
pub mod live;
pub mod search;
pub mod stats;

use crossterm::event::KeyEvent;
use ratatui::prelude::*;

use super::event::{Action, AppEvent};

/// Each tab implements this trait.
pub trait TabComponent {
    fn handle_key(&mut self, key: KeyEvent) -> Option<Action>;
    fn handle_event(&mut self, event: &AppEvent);
    fn render(&self, frame: &mut Frame, area: Rect);
    fn on_activate(&mut self) -> Option<Action>;
    fn on_deactivate(&mut self);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Browse,
    Search,
    Live,
    Stats,
    Graph,
}

impl Tab {
    pub const ALL: [Tab; 5] = [Tab::Browse, Tab::Search, Tab::Live, Tab::Stats, Tab::Graph];

    pub fn title(&self) -> &'static str {
        match self {
            Tab::Browse => "Browse",
            Tab::Search => "Search",
            Tab::Live => "Live",
            Tab::Stats => "Stats",
            Tab::Graph => "Graph",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Tab::Browse => 0,
            Tab::Search => 1,
            Tab::Live => 2,
            Tab::Stats => 3,
            Tab::Graph => 4,
        }
    }

    pub fn from_index(i: usize) -> Self {
        match i % 5 {
            0 => Tab::Browse,
            1 => Tab::Search,
            2 => Tab::Live,
            3 => Tab::Stats,
            _ => Tab::Graph,
        }
    }
}
