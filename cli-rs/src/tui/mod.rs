mod app;
mod event;
mod sse;
pub mod tabs;
mod widgets;

use std::io;

use color_eyre::eyre::Result;
use crossterm::{
    event::DisableMouseCapture,
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;

use memlayer_common::config::Config;

use app::App;

pub async fn run(config: Config) -> std::result::Result<(), String> {
    // Install panic hook that restores terminal
    color_eyre::install().ok();

    let result = run_inner(config).await;

    // Ensure terminal is restored even on error
    let _ = restore_terminal();

    result.map_err(|e| format!("{e}"))
}

async fn run_inner(config: Config) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut app = App::new(config);
    app.run(&mut terminal).await?;

    Ok(())
}

fn restore_terminal() -> Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}
