use clap::Args;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;
use memlayer_common::file_cache::FileCache;

use crate::format::session::{format_session_json, format_session_text};

#[derive(Args)]
pub struct SessionArgs {
    /// Session UUID to retrieve
    session_id: String,

    /// Max entries (1-500)
    #[arg(long, default_value = "200")]
    limit: u32,

    /// Comma-separated: user,assistant,tool_use,tool_result (default: user,assistant)
    #[arg(long)]
    types: Option<String>,

    /// Include all content types (overrides default user,assistant filter)
    #[arg(long, conflicts_with = "types")]
    all_types: bool,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

pub async fn run(args: SessionArgs) -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);
    let cache = FileCache::new(config.cache_dir.clone());

    let types: Option<Vec<String>> = if args.all_types {
        None
    } else if let Some(t) = args.types {
        Some(t.split(',').map(|s| s.trim().to_string()).collect())
    } else {
        Some(vec!["user".to_string(), "assistant".to_string()])
    };

    let summary = client
        .get_session_summary(&args.session_id, args.limit, types.as_deref())
        .await?;

    // Pre-cache large response file if present
    if let Some(ref lr) = summary.large_response {
        let file_id = lr.file_id.clone();
        let _ = cache
            .ensure_cached(&file_id, || client.download_file(&file_id))
            .await;
    }

    let output = if args.format == "text" {
        format_session_text(&summary)
    } else {
        format_session_json(&summary)
    };
    println!("{output}");
    Ok(())
}
