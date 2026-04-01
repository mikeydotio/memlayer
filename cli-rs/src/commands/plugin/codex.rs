use std::path::PathBuf;

use super::common;

fn instructions_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_default()
        .join(".codex")
        .join("instructions.md")
}

pub async fn install() -> Result<(), String> {
    let path = instructions_path();
    common::inject_instructions(&path)?;

    eprintln!("Memlayer instructions installed for Codex.");
    eprintln!();
    eprintln!("  File: {}", path.display());
    eprintln!();
    eprintln!("Codex will now use the `memlayer` CLI for cross-session recall.");
    eprintln!("MCP server integration will be available in a future release.");

    Ok(())
}

pub async fn uninstall() -> Result<(), String> {
    let path = instructions_path();
    if common::remove_instructions(&path)? {
        eprintln!("Memlayer instructions removed from Codex.");
    } else {
        eprintln!("Memlayer is not installed for Codex.");
    }

    Ok(())
}

pub fn is_installed() -> Option<(String, PathBuf)> {
    let path = instructions_path();
    let content = std::fs::read_to_string(&path).ok()?;
    if content.contains("<!-- memlayer:start -->") {
        Some(("instructions".to_string(), path))
    } else {
        None
    }
}
