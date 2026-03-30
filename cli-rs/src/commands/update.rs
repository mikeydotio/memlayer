use clap::Args;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;

#[derive(Args)]
pub struct UpdateArgs {}

pub async fn run(_args: UpdateArgs) -> Result<(), String> {
    let config = Config::load();
    let client = MemlayerClient::new(&config);

    let current = env!("CARGO_PKG_VERSION");

    let version_info = client.get_version().await.map_err(|e| {
        format!("Failed to reach server: {e}\nCheck that the memlayer server is running.")
    })?;

    let server_version = &version_info.server_version;

    if current == server_version {
        println!("Already up to date (v{current})");
    } else {
        println!("Current CLI version:  v{current}");
        println!("Server version:       v{server_version}");
        println!();
        println!("A newer version may be available.");
        println!("Download the latest release from:");
        println!();
        println!("  https://github.com/mikeydotio/memlayer/releases/latest");
        println!();
        println!("Or rebuild from source:");
        println!("  cargo build -p memlayer-cli --release");
    }

    Ok(())
}
