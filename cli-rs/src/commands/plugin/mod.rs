mod claude_code;
mod codex;
pub mod common;
mod gemini;

use clap::{Args, Subcommand, ValueEnum};

#[derive(Args)]
pub struct PluginArgs {
    #[command(subcommand)]
    command: PluginCommand,
}

#[derive(Subcommand)]
pub enum PluginCommand {
    /// Install memlayer into an AI coding tool
    Install(InstallArgs),
    /// Remove memlayer from an AI coding tool
    Uninstall(UninstallArgs),
    /// Show which AI coding tools have memlayer installed
    List,
}

#[derive(Args)]
pub struct InstallArgs {
    /// Target tool to install the memlayer plugin into
    target: Target,
}

#[derive(Args)]
pub struct UninstallArgs {
    /// Target tool to remove the memlayer plugin from
    target: Option<Target>,

    /// Remove memlayer from all AI coding tools
    #[arg(long)]
    all: bool,
}

#[derive(Clone, ValueEnum)]
pub enum Target {
    /// Claude Code (full plugin with skills and hooks)
    ClaudeCode,
    /// OpenAI Codex CLI (instructions file)
    Codex,
    /// Google Gemini CLI (instructions file)
    Gemini,
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Target::ClaudeCode => write!(f, "claude-code"),
            Target::Codex => write!(f, "codex"),
            Target::Gemini => write!(f, "gemini"),
        }
    }
}

pub async fn run(args: PluginArgs) -> Result<(), String> {
    match args.command {
        PluginCommand::Install(install_args) => run_install(install_args).await,
        PluginCommand::Uninstall(uninstall_args) => run_uninstall(uninstall_args).await,
        PluginCommand::List => run_list().await,
    }
}

async fn run_install(args: InstallArgs) -> Result<(), String> {
    match args.target {
        Target::ClaudeCode => claude_code::install().await,
        Target::Codex => codex::install().await,
        Target::Gemini => gemini::install().await,
    }
}

async fn run_uninstall(args: UninstallArgs) -> Result<(), String> {
    if args.all {
        claude_code::uninstall().await?;
        codex::uninstall().await?;
        gemini::uninstall().await?;
        return Ok(());
    }

    match args.target {
        Some(target) => match target {
            Target::ClaudeCode => claude_code::uninstall().await,
            Target::Codex => codex::uninstall().await,
            Target::Gemini => gemini::uninstall().await,
        },
        None => Err("Specify a target (claude-code, codex, gemini) or use --all".to_string()),
    }
}

async fn run_list() -> Result<(), String> {
    let tools: Vec<(&str, Option<(String, std::path::PathBuf)>)> = vec![
        ("claude-code", claude_code::is_installed()),
        ("codex", codex::is_installed()),
        ("gemini", gemini::is_installed()),
    ];

    eprintln!("{:<14} {:<12} {}", "TOOL", "STATUS", "LOCATION");
    eprintln!("{:<14} {:<12} {}", "----", "------", "--------");

    for (name, info) in &tools {
        match info {
            Some((version, path)) => {
                eprintln!("{:<14} {:<12} {}", name, version, path.display());
            }
            None => {
                eprintln!("{:<14} {:<12} -", name, "not installed");
            }
        }
    }

    Ok(())
}
