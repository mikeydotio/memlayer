use clap::Args;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;

use crate::format::entities::{format_entities_json, format_entities_text};

#[derive(Args)]
pub struct EntitiesArgs {
    /// Fuzzy search on entity name
    #[arg(long)]
    query: Option<String>,

    /// Filter by entity type (concept, decision, bug, pattern, tool, library, etc.)
    #[arg(long, name = "type")]
    entity_type: Option<String>,

    /// Filter to project path
    #[arg(long)]
    project: Option<String>,

    /// Entity status filter (default: active)
    #[arg(long, default_value = "active")]
    status: String,

    /// Max results
    #[arg(long, default_value = "20")]
    limit: u32,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

pub async fn run(args: EntitiesArgs) -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);

    let page = client
        .get_entities(
            args.query.as_deref(),
            args.entity_type.as_deref(),
            args.project.as_deref(),
            &args.status,
            args.limit,
            0,
        )
        .await?;

    let output = if args.format == "text" {
        format_entities_text(&page)
    } else {
        format_entities_json(&page)
    };
    println!("{output}");
    Ok(())
}
