use clap::Args;
use std::fs;

#[derive(Args)]
pub struct RollbackArgs {
    /// List available archived versions
    #[arg(long)]
    list: bool,
}

pub async fn run(args: RollbackArgs) -> Result<(), String> {
    let versions_dir = dirs::data_dir()
        .ok_or_else(|| "Could not determine data directory".to_string())?
        .join("memlayer")
        .join("versions");

    if !versions_dir.exists() {
        println!("No archived versions found.");
        println!("Directory does not exist: {}", versions_dir.display());
        return Ok(());
    }

    let mut entries: Vec<_> = fs::read_dir(&versions_dir)
        .map_err(|e| format!("Failed to read versions directory: {e}"))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .file_type()
                .map(|ft| ft.is_file())
                .unwrap_or(false)
        })
        .collect();

    if entries.is_empty() {
        println!("No archived versions found in {}", versions_dir.display());
        return Ok(());
    }

    // Sort by modification time, most recent last
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH));

    if args.list {
        println!("Archived versions in {}:", versions_dir.display());
        println!();
        for entry in &entries {
            let name = entry.file_name();
            let modified = entry
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| {
                    let duration = t
                        .duration_since(std::time::SystemTime::UNIX_EPOCH)
                        .ok()?;
                    let dt = chrono::DateTime::from_timestamp(duration.as_secs() as i64, 0)?;
                    Some(dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                })
                .unwrap_or_else(|| "unknown".to_string());
            println!("  {} ({})", name.to_string_lossy(), modified);
        }
    } else {
        let most_recent = entries.last().unwrap();
        let archive_path = most_recent.path();

        println!("Most recent archived version:");
        println!("  {}", archive_path.display());
        println!();
        println!("To rollback, replace the current binary:");
        println!("  cp {} $(which memlayer)", archive_path.display());
    }

    Ok(())
}
