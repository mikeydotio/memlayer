use std::path::Path;

const RAW_BASE: &str =
    "https://raw.githubusercontent.com/mikeydotio/memlayer/main";

const INSTRUCTIONS_TEMPLATE: &str = r#"## Memory (Cross-Session Recall + Knowledge Graph)

The `memlayer` CLI provides commands to search past conversations and browse a knowledge graph of extracted concepts and relationships:

**Search & recall:**
- `memlayer recent` — list recent sessions by last activity (great for "what was I working on?")
- `memlayer search "<query>"` — hybrid search across all past conversations
- `memlayer search "<query>" --expand-graph` — also surface entries connected through the knowledge graph
- `memlayer session <session-uuid>` — full chronological session history
- `memlayer read-file <file-uuid> --start <n> --end <n>` — read specific line ranges from large response files

**Knowledge graph:**
- `memlayer entities` — list extracted entities (concepts, decisions, bugs, patterns, tools)
- `memlayer entities --query "auth" --type decision` — search/filter entities
- `memlayer entity <id>` — view entity detail with relationships and mentions
- `memlayer entity <id> --neighbors` — include graph neighbors
- `memlayer graph stats` — knowledge graph statistics

Use `memlayer search` when the user references past work, asks about prior
decisions, or encounters a problem that may have been solved before.
Use `--expand-graph` when you want to find related context beyond direct matches.
Use `memlayer entities` to browse what concepts and decisions have been extracted.
Use keyword-rich queries for best results.

**Default behavior:** Search and session commands only return `user` and
`assistant` entries by default. Use `--all-types` for tool_use/tool_result.
Search results are truncated to 200 chars — use `--full` for complete content.

When a search or session response is too large to return inline,
the server offloads it to a file and returns a summary with a structural
index. Use `memlayer read-file` with the file_id and line range from the
index to retrieve the specific sections you need."#;

const SECTION_START: &str = "<!-- memlayer:start -->";
const SECTION_END: &str = "<!-- memlayer:end -->";

/// Download a file from the memlayer GitHub repo.
pub async fn download_file(client: &reqwest::Client, rel_path: &str) -> Result<Vec<u8>, String> {
    let url = format!("{RAW_BASE}/{rel_path}");
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to download {rel_path}: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Failed to download {rel_path}: HTTP {}",
            resp.status()
        ));
    }

    resp.bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Failed to read {rel_path}: {e}"))
}

/// Download a file and write it to the given path, creating parent dirs.
pub async fn download_to(
    client: &reqwest::Client,
    rel_path: &str,
    dest: &Path,
) -> Result<(), String> {
    let bytes = download_file(client, rel_path).await?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(dest, &bytes)
        .map_err(|e| format!("Failed to write {}: {e}", dest.display()))
}

/// Build the memlayer instructions block wrapped in sentinel markers.
pub fn instructions_block() -> String {
    format!("{SECTION_START}\n{INSTRUCTIONS_TEMPLATE}\n{SECTION_END}\n")
}

/// Inject the memlayer instructions section into a file.
/// If the section already exists, replace it. Otherwise append.
pub fn inject_instructions(path: &Path) -> Result<(), String> {
    let block = instructions_block();
    let content = std::fs::read_to_string(path).unwrap_or_default();

    let new_content = if content.contains(SECTION_START) {
        replace_section(&content, &block)
    } else if content.is_empty() {
        block
    } else {
        format!("{}\n\n{}", content.trim_end(), block)
    };

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    std::fs::write(path, new_content)
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

/// Remove the memlayer instructions section from a file.
/// Returns true if the section was found and removed.
pub fn remove_instructions(path: &Path) -> Result<bool, String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Ok(false),
    };

    if !content.contains(SECTION_START) {
        return Ok(false);
    }

    let new_content = remove_section(&content);
    let trimmed = new_content.trim();

    if trimmed.is_empty() {
        // File is empty after removal — delete it
        std::fs::remove_file(path)
            .map_err(|e| format!("Failed to remove {}: {e}", path.display()))?;
    } else {
        std::fs::write(path, format!("{trimmed}\n"))
            .map_err(|e| format!("Failed to write {}: {e}", path.display()))?;
    }

    Ok(true)
}

/// Replace content between sentinel markers.
fn replace_section(content: &str, replacement: &str) -> String {
    let mut result = String::new();
    let mut in_section = false;
    let mut replaced = false;

    for line in content.lines() {
        if line.trim() == SECTION_START {
            in_section = true;
            if !replaced {
                result.push_str(replacement);
                replaced = true;
            }
            continue;
        }
        if line.trim() == SECTION_END {
            in_section = false;
            continue;
        }
        if !in_section {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

/// Remove content between sentinel markers (inclusive).
fn remove_section(content: &str) -> String {
    let mut result = String::new();
    let mut in_section = false;

    for line in content.lines() {
        if line.trim() == SECTION_START {
            in_section = true;
            continue;
        }
        if line.trim() == SECTION_END {
            in_section = false;
            continue;
        }
        if !in_section {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

/// Read a JSON file, returning a default Value if it doesn't exist.
pub fn read_json(path: &Path, default: serde_json::Value) -> Result<serde_json::Value, String> {
    match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {e}", path.display())),
        Err(_) => Ok(default),
    }
}

/// Write a JSON Value to a file, creating parent dirs.
pub fn write_json(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create {}: {e}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize JSON: {e}"))?;
    std::fs::write(path, format!("{content}\n"))
        .map_err(|e| format!("Failed to write {}: {e}", path.display()))
}
