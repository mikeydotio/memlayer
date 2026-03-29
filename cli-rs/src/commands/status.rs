use clap::Args;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;

use crate::format::status::{format_status_json, format_status_text};

#[derive(Args)]
pub struct StatusArgs {
    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

pub async fn run(args: StatusArgs) -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);

    let (health_result, embeddings_result) =
        tokio::join!(client.get_health(), client.get_embedding_status());

    let health = match health_result {
        Ok(v) => v,
        Err(e) => serde_json::json!({ "error": e }),
    };

    let embeddings = match embeddings_result {
        Ok(v) => v,
        Err(e) => serde_json::json!({ "error": e }),
    };

    let output = if args.format == "text" {
        format_status_text(&health, &embeddings)
    } else {
        format_status_json(&health, &embeddings)
    };
    println!("{output}");
    Ok(())
}
