pub mod dashboard;
pub mod read_file;
pub mod recent;
pub mod search;
pub mod session;
pub mod status;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum Commands {
    /// Search across all past Claude Code conversations
    Search(search::SearchArgs),
    /// Retrieve full conversation history for a session
    Session(session::SessionArgs),
    /// List recent sessions without a search query
    Recent(recent::RecentArgs),
    /// Read a line range from a large response file
    #[command(name = "read-file")]
    ReadFile(read_file::ReadFileArgs),
    /// Show server health and embedding status
    Status(status::StatusArgs),
    /// Launch interactive TUI dashboard
    Dashboard,
}

impl Commands {
    pub async fn run(self) -> Result<(), String> {
        match self {
            Commands::Search(args) => search::run(args).await,
            Commands::Session(args) => session::run(args).await,
            Commands::Recent(args) => recent::run(args).await,
            Commands::ReadFile(args) => read_file::run(args).await,
            Commands::Status(args) => status::run(args).await,
            Commands::Dashboard => dashboard::run().await,
        }
    }
}
