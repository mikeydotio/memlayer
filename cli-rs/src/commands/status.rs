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

    let (health_result, embeddings_result, version_result) = tokio::join!(
        client.get_health(),
        client.get_embedding_status(),
        client.get_version(),
    );

    let health = match health_result {
        Ok(v) => v,
        Err(e) => serde_json::json!({ "error": e }),
    };

    let embeddings = match embeddings_result {
        Ok(v) => v,
        Err(e) => serde_json::json!({ "error": e }),
    };

    let version = match &version_result {
        Ok(v) => serde_json::to_value(v).unwrap_or(serde_json::json!({ "error": "parse failed" })),
        Err(e) => serde_json::json!({ "error": e }),
    };

    // Read daemon version error file (if any)
    let version_error = read_daemon_version_error();

    let output = if args.format == "text" {
        format_status_text(&health, &embeddings, &version, version_error.as_deref())
    } else {
        format_status_json(&health, &embeddings, &version, version_error.as_deref())
    };
    println!("{output}");
    Ok(())
}

/// Read the daemon's version error file, if it exists.
fn read_daemon_version_error() -> Option<String> {
    let data_dir = dirs::data_dir()?.join("memlayer");
    let error_path = data_dir.join("version_error");
    std::fs::read_to_string(error_path).ok()
}
