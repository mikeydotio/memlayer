use clap::{Args, Subcommand};
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;

#[derive(Args)]
pub struct GraphArgs {
    #[command(subcommand)]
    command: GraphCommand,
}

#[derive(Subcommand)]
pub enum GraphCommand {
    /// Show knowledge graph statistics
    Stats,
}

pub async fn run(args: GraphArgs) -> Result<(), String> {
    match args.command {
        GraphCommand::Stats => run_stats().await,
    }
}

async fn run_stats() -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);

    let stats = client.get_graph_stats().await?;

    let json = serde_json::to_string_pretty(&stats)
        .unwrap_or_else(|_| "{}".to_string());
    println!("{json}");
    Ok(())
}
