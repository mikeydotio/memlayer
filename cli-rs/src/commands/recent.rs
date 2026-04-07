use clap::Args;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;

use crate::format::recent::{format_recent_json, format_recent_text};

#[derive(Args)]
pub struct RecentArgs {
    /// Number of entries to show
    #[arg(default_value = "10")]
    limit: u32,

    /// Show entries from all hosts (default: current host only)
    #[arg(long)]
    all: bool,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

fn resolve_machine_id() -> String {
    std::env::var("MEMLAYER_MACHINE_ID").unwrap_or_else(|_| {
        hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "unknown".to_string())
    })
}

pub async fn run(args: RecentArgs) -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);

    let machine_id = if args.all {
        None
    } else {
        Some(resolve_machine_id())
    };

    let page = client
        .get_recent_entries(machine_id.as_deref(), args.limit)
        .await?;

    let output = if args.format == "text" {
        format_recent_text(&page)
    } else {
        format_recent_json(&page)
    };
    println!("{output}");
    Ok(())
}
