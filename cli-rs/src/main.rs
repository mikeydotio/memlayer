mod commands;
mod format;
pub mod tui;

use clap::Parser;
use commands::Commands;

#[derive(Parser)]
#[command(name = "memlayer", version = "1.5.0")]
#[command(about = "Memlayer — search and recall past Claude Code conversations")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    if let Err(e) = cli.command.run().await {
        eprint!("{}", format::error::format_error(&e));
        eprintln!();
        std::process::exit(1);
    }
}
