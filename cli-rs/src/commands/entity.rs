use clap::Args;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;

use crate::format::entity::{format_entity_json, format_entity_text};

#[derive(Args)]
pub struct EntityArgs {
    /// Entity ID
    id: i64,

    /// Include graph neighbors
    #[arg(long)]
    neighbors: bool,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

pub async fn run(args: EntityArgs) -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);

    let detail = client.get_entity(args.id).await?;

    let output = if args.format == "text" {
        format_entity_text(&detail, args.neighbors)
    } else {
        format_entity_json(&detail)
    };
    println!("{output}");

    if args.neighbors {
        let neighbors = client.get_entity_neighbors(args.id, 1).await?;
        if args.format == "text" {
            println!("\n--- Graph Neighbors ---");
            for node in &neighbors.nodes {
                // Find the edge connecting to this node
                let edge_label = neighbors.edges.iter()
                    .find(|e| e.source_id == node.id || e.target_id == node.id)
                    .map(|e| e.relationship_type.as_str())
                    .unwrap_or("related_to");
                println!(
                    "  --[{}]--> {} [{}] (mentions: {})",
                    edge_label, node.canonical_name, node.entity_type, node.mention_count
                );
            }
        } else {
            let json = serde_json::to_string_pretty(&neighbors)
                .unwrap_or_else(|_| "{}".to_string());
            println!("{json}");
        }
    }

    Ok(())
}
