mod config;
mod cursor;
mod migration;
mod parser;
mod queue;
mod sender;
mod watcher;

use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tokio::sync::{watch, Mutex};
use tracing::{info, warn};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        println!("memlayer-daemon {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    // Log level controlled by RUST_LOG env var (default: info)
    // Examples: RUST_LOG=debug, RUST_LOG=claude_mem_daemon=trace
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
        "Starting memlayer-daemon"
    );

    // Ensure data directory exists
    std::fs::create_dir_all(&config.data_dir)?;

    let cursor_mgr = Arc::new(Mutex::new(cursor::CursorManager::new(&config.data_dir)?));
    let offline_queue = Arc::new(Mutex::new(queue::OfflineQueue::new(&config.data_dir)?));
    let sender = Arc::new(sender::Sender::new(config.clone(), offline_queue.clone()));

    // Create a shutdown signal channel
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn signal handler
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        let ctrl_c = signal::ctrl_c();
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to register SIGTERM handler");

        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }

        info!("Shutdown signal received, draining queue...");
        let _ = shutdown_tx_clone.send(true);
    });

    // Spawn queue drain loop
    let sender_clone = sender.clone();
    let drain_shutdown_rx = shutdown_rx.clone();
    let drain_handle = tokio::spawn(async move {
        sender_clone.drain_queue_loop(drain_shutdown_rx).await;
    });

    // Run file watcher (blocks until shutdown)
    let file_watcher = watcher::Watcher::new(config.clone(), cursor_mgr, sender);
    let watcher_handle = tokio::spawn(async move {
        if let Err(e) = file_watcher.run(shutdown_rx).await {
            tracing::error!(error = %e, "Watcher failed");
        }
    });

    // Wait for watcher to finish (it returns on shutdown signal)
    if let Err(e) = watcher_handle.await {
        warn!(error = %e, "Watcher task failed");
    }

    // Wait for drain loop to finish with a 30-second timeout
    tokio::select! {
        result = drain_handle => {
            match result {
                Ok(()) => info!("Queue drained successfully"),
                Err(e) => warn!(error = %e, "Drain task panicked"),
            }
        }
        _ = tokio::time::sleep(Duration::from_secs(30)) => {
            warn!("Shutdown timeout (30s), forcing exit");
        }
    }

    info!("memlayer-daemon stopped");
    Ok(())
}
