use memlayer_common::config::Config;

use crate::tui;

pub async fn run() -> Result<(), String> {
    let config = Config::load();
    tui::run(config).await
}
