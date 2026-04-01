use std::path::PathBuf;

use super::common;

const PLUGIN_KEY: &str = "memlayer@memlayer-github";
const MARKETPLACE: &str = "memlayer-github";
const PLUGIN_NAME: &str = "memlayer";

const PLUGIN_FILES: &[(&str, &str)] = &[
    ("plugin/.claude-plugin/plugin.json", ".claude-plugin/plugin.json"),
    ("plugin/hooks/hooks.json", "hooks/hooks.json"),
    ("plugin/hooks/memory-read-hook.sh", "hooks/memory-read-hook.sh"),
    ("plugin/skills/recall/SKILL.md", "skills/recall/SKILL.md"),
    ("plugin/skills/health/SKILL.md", "skills/health/SKILL.md"),
    ("plugin/skills/graph/SKILL.md", "skills/graph/SKILL.md"),
];

fn claude_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".claude")
}

fn cache_dir() -> PathBuf {
    claude_dir()
        .join("plugins")
        .join("cache")
        .join(MARKETPLACE)
        .join(PLUGIN_NAME)
}

fn installed_plugins_path() -> PathBuf {
    claude_dir().join("plugins").join("installed_plugins.json")
}

fn settings_path() -> PathBuf {
    claude_dir().join("settings.json")
}

fn claude_md_path() -> PathBuf {
    claude_dir().join("CLAUDE.md")
}

pub async fn install() -> Result<(), String> {
    let http = reqwest::Client::new();

    // Determine plugin version from plugin.json
    eprintln!("Downloading memlayer plugin for Claude Code...");

    let plugin_json_bytes = common::download_file(&http, "plugin/.claude-plugin/plugin.json").await?;
    let plugin_meta: serde_json::Value = serde_json::from_slice(&plugin_json_bytes)
        .map_err(|e| format!("Failed to parse plugin.json: {e}"))?;
    let version = plugin_meta["version"]
        .as_str()
        .unwrap_or("latest")
        .to_string();

    let dest_dir = cache_dir().join(&version);

    // Download all plugin files
    for (src, rel_dest) in PLUGIN_FILES {
        let dest = dest_dir.join(rel_dest);
        common::download_to(&http, src, &dest).await?;
    }

    // Make hook script executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let hook_path = dest_dir.join("hooks/memory-read-hook.sh");
        if hook_path.exists() {
            let perms = std::fs::Permissions::from_mode(0o755);
            let _ = std::fs::set_permissions(&hook_path, perms);
        }
    }

    eprintln!("Registering plugin...");

    // Register in installed_plugins.json
    let installed_path = installed_plugins_path();
    let default = serde_json::json!({"version": 2, "plugins": {}});
    let mut installed = common::read_json(&installed_path, default)?;

    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    installed["plugins"][PLUGIN_KEY] = serde_json::json!([{
        "scope": "user",
        "installPath": dest_dir.to_string_lossy(),
        "version": version,
        "installedAt": now,
        "lastUpdated": now,
    }]);

    common::write_json(&installed_path, &installed)?;

    // Enable in settings.json
    let settings_path = settings_path();
    let default_settings = serde_json::json!({"permissions": {}, "enabledPlugins": {}});
    let mut settings = common::read_json(&settings_path, default_settings)?;

    settings["enabledPlugins"][PLUGIN_KEY] = serde_json::json!(true);

    common::write_json(&settings_path, &settings)?;

    // Inject CLAUDE.md instructions
    common::inject_instructions(&claude_md_path())?;

    eprintln!("Plugin installed (v{version})");
    eprintln!();
    eprintln!("  Cache:    {}", dest_dir.display());
    eprintln!("  Skills:   memlayer:recall, memlayer:health, memlayer:graph");
    eprintln!("  Hooks:    PreToolUse (memory file augmentation)");
    eprintln!();
    eprintln!("Restart Claude Code to activate the plugin.");

    Ok(())
}

pub async fn uninstall() -> Result<(), String> {
    let mut removed = false;

    // Remove from installed_plugins.json
    let installed_path = installed_plugins_path();
    if installed_path.exists() {
        let mut installed = common::read_json(&installed_path, serde_json::json!({}))?;
        if let Some(plugins) = installed.get_mut("plugins").and_then(|p| p.as_object_mut()) {
            if plugins.remove(PLUGIN_KEY).is_some() {
                common::write_json(&installed_path, &installed)?;
                removed = true;
            }
        }
    }

    // Remove from settings.json
    let settings_path = settings_path();
    if settings_path.exists() {
        let mut settings = common::read_json(&settings_path, serde_json::json!({}))?;
        if let Some(enabled) = settings.get_mut("enabledPlugins").and_then(|e| e.as_object_mut()) {
            if enabled.remove(PLUGIN_KEY).is_some() {
                common::write_json(&settings_path, &settings)?;
                removed = true;
            }
        }
    }

    // Remove cache directory
    let cache = cache_dir();
    if cache.exists() {
        std::fs::remove_dir_all(&cache)
            .map_err(|e| format!("Failed to remove {}: {e}", cache.display()))?;
        removed = true;
    }

    // Remove CLAUDE.md instructions
    if common::remove_instructions(&claude_md_path())? {
        removed = true;
    }

    if removed {
        eprintln!("Memlayer plugin removed from Claude Code.");
        eprintln!("Restart Claude Code to complete removal.");
    } else {
        eprintln!("Memlayer plugin is not installed for Claude Code.");
    }

    Ok(())
}

pub fn is_installed() -> Option<(String, PathBuf)> {
    let installed_path = installed_plugins_path();
    let installed = common::read_json(&installed_path, serde_json::json!({})).ok()?;

    let entry = installed.get("plugins")?.get(PLUGIN_KEY)?.as_array()?;
    let first = entry.first()?;
    let version = first.get("version")?.as_str()?.to_string();
    let path = first.get("installPath")?.as_str()?;

    Some((version, PathBuf::from(path)))
}
