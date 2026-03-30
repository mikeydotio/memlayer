pub mod dashboard;
pub mod entities;
pub mod entity;
pub mod graph;
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
    /// List and search knowledge graph entities
    Entities(entities::EntitiesArgs),
    /// View entity detail and relationships
    Entity(entity::EntityArgs),
    /// Knowledge graph operations
    Graph(graph::GraphArgs),
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
            Commands::Entities(args) => entities::run(args).await,
            Commands::Entity(args) => entity::run(args).await,
            Commands::Graph(args) => graph::run(args).await,
        }
    }
}
