use clap::Args;
use memlayer_common::client::MemlayerClient;
use memlayer_common::config::Config;
use memlayer_common::file_cache::FileCache;

use crate::format::read_file::{format_read_file_json, format_read_file_text};

#[derive(Args)]
pub struct ReadFileArgs {
    /// File ID from large_response reference
    file_id: String,

    /// Start line (1-indexed, inclusive)
    #[arg(long)]
    start: Option<usize>,

    /// End line (1-indexed, inclusive)
    #[arg(long)]
    end: Option<usize>,

    /// Output format: json or text
    #[arg(long, default_value = "json")]
    format: String,
}

pub async fn run(args: ReadFileArgs) -> Result<(), String> {
    let start = args
        .start
        .ok_or_else(|| "--start and --end are required".to_string())?;
    let end = args
        .end
        .ok_or_else(|| "--start and --end are required".to_string())?;

    let config = Config::load();
    let client = MemlayerClient::new(&config);
    let cache = FileCache::new(config.cache_dir.clone());

    let file_id = args.file_id.clone();
    let local_path = cache
        .ensure_cached(&file_id, || client.download_file(&file_id))
        .await?;

    let content = FileCache::read_lines(&local_path, start, end)?;

    let output = if args.format == "text" {
        format_read_file_text(&args.file_id, start, end, &content)
    } else {
        format_read_file_json(&args.file_id, start, end, &content)
    };
    println!("{output}");
    Ok(())
}
