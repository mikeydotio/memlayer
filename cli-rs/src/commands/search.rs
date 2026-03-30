use clap::Args;
use memlayer_common::api_types::SearchRequest;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;
use memlayer_common::file_cache::FileCache;

use crate::format::search::{format_search_json, format_search_text};

#[derive(Args)]
pub struct SearchArgs {
    /// Natural language search query
    query: String,

    /// Filter to project path
    #[arg(long)]
    project: Option<String>,

    /// Filter to specific session
    #[arg(long)]
    session_id: Option<String>,

    /// Max results (1-50)
    #[arg(long, default_value = "10")]
    limit: u32,

    /// Entries after timestamp (ISO 8601)
    #[arg(long)]
    after: Option<String>,

    /// Entries before timestamp (ISO 8601)
    #[arg(long)]
    before: Option<String>,

    /// Comma-separated: user,assistant,tool_use,tool_result (default: user,assistant)
    #[arg(long)]
    types: Option<String>,

    /// Include all content types (overrides default user,assistant filter)
    #[arg(long, conflicts_with = "types")]
    all_types: bool,

    /// Return full untruncated content (default: truncated to 200 chars)
    #[arg(long)]
    full: bool,

    /// Expand results with graph-connected entries
    #[arg(long)]
    expand_graph: bool,

    /// Weight for graph-based re-ranking (0.0-2.0, default 0.5)
    #[arg(long)]
    graph_weight: Option<f64>,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

pub async fn run(args: SearchArgs) -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);
    let cache = FileCache::new(config.cache_dir.clone());

    let types = if args.all_types {
        None
    } else if let Some(t) = args.types {
        Some(t.split(',').map(|s| s.trim().to_string()).collect())
    } else {
        Some(vec!["user".to_string(), "assistant".to_string()])
    };

    let results = client
        .search(&SearchRequest {
            query: args.query,
            session_id: args.session_id,
            project_path: args.project,
            limit: args.limit,
            after: args.after,
            before: args.before,
            types,
            truncate: if args.full { Some(false) } else { None },
            expand_graph: if args.expand_graph { Some(true) } else { None },
            graph_weight: args.graph_weight,
        })
        .await?;

    // Pre-cache large response file if present
    if let Some(ref lr) = results.large_response {
        let file_id = lr.file_id.clone();
        let _ = cache
            .ensure_cached(&file_id, || client.download_file(&file_id))
            .await;
    }

    let output = if args.format == "text" {
        format_search_text(&results)
    } else {
        format_search_json(&results)
    };
    println!("{output}");
    Ok(())
}
