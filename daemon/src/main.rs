mod config;
mod cursor;
mod parser;
mod queue;
mod sender;
mod watcher;

use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let config = Arc::new(config::Config::from_env());
    info!(
        server = %config.server_url,
        watch_path = %config.watch_path.display(),
        machine_id = %config.machine_id,
        "Starting claude-mem-daemon"
    );

    // Ensure data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    let cursor_mgr = Arc::new(Mutex::new(cursor::CursorManager::new(&config.data_dir)?));
    let offline_queue = Arc::new(Mutex::new(queue::OfflineQueue::new(&config.data_dir)?));
    let sender = Arc::new(sender::Sender::new(config.clone(), offline_queue.clone()));

    // Spawn queue drain loop
    let sender_clone = sender.clone();
    tokio::spawn(async move {
        sender_clone.drain_queue_loop().await;
    });

    // Run file watcher (blocks)
    let file_watcher = watcher::Watcher::new(config.clone(), cursor_mgr, sender);
    file_watcher.run().await?;

    Ok(())
}
