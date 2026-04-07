use clap::Args;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;

use crate::format::sessions::{format_sessions_json, format_sessions_text};

#[derive(Args)]
pub struct SessionsArgs {
    /// Max sessions to show (1-50)
    #[arg(long, default_value = "10")]
    limit: u32,

    /// Filter to project path
    #[arg(long)]
    project: Option<String>,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

pub async fn run(args: SessionsArgs) -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);

    let page = client
        .get_sessions(args.project.as_deref(), 0, args.limit)
        .await?;

    let output = if args.format == "text" {
        format_sessions_text(&page)
    } else {
        format_sessions_json(&page)
    };
    println!("{output}");
    Ok(())
}
