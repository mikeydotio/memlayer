use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedEntry {
    pub payload_hash: String,
    pub session_id: String,
    pub message_type: String,
    pub content_type: String,
    pub raw_content: String,
    pub timestamp: String,
    pub project_path: String,
    pub client_machine_id: String,
    pub slug: Option<String>,
    pub source_uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub tool_name: Option<String>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
}

/// Decode a Claude project directory name into a filesystem path.
/// e.g. "-home-mikey-memlayer" -> "/home/mikey/memlayer"
pub fn decode_project_path(dir_name: &str) -> String {
    if dir_name.starts_with('-') {
        dir_name.replacen('-', "/", 1).replace('-', "/")
    } else {
        dir_name.to_string()
    }
}

/// Derive the project path from a JSONL file's path.
pub fn project_path_from_file(file_path: &Path) -> String {
    // Path is like ~/.claude/projects/-home-mikey-memlayer/<session>.jsonl
    if let Some(parent) = file_path.parent() {
        if let Some(dir_name) = parent.file_name() {
            return decode_project_path(&dir_name.to_string_lossy());
        }
    }
    "unknown".to_string()
}

/// Parse a single JSONL line into zero or more ParsedEntry items.
/// Returns empty vec for entries that should be skipped.
pub fn parse_jsonl_line(raw_line: &[u8], project_path: &str, machine_id: &str) -> Vec<ParsedEntry> {
    let v: Value = match serde_json::from_slice(raw_line) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    let entry_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");

    match entry_type {
        "user" => parse_user_entry(&v, raw_line, project_path, machine_id),
        "assistant" => parse_assistant_entry(&v, raw_line, project_path, machine_id),
        _ => vec![], // Skip progress, file-history-snapshot, etc.
    }
}

fn common_fields(v: &Value, project_path: &str, machine_id: &str) -> (String, Option<String>, Option<String>, Option<String>, Option<String>, String) {
    let session_id = v.get("sessionId").and_then(|s| s.as_str()).unwrap_or("").to_string();
    let slug = v.get("slug").and_then(|s| s.as_str()).map(String::from);
    let source_uuid = v.get("uuid").and_then(|s| s.as_str()).map(String::from);
    let parent_uuid = v.get("parentUuid").and_then(|s| s.as_str()).map(String::from);
    let cwd = v.get("cwd").and_then(|s| s.as_str()).map(String::from);
    let timestamp = v.get("timestamp").and_then(|s| s.as_str()).unwrap_or("").to_string();
    let _ = (project_path, machine_id); // used by caller
    (session_id, slug, source_uuid, parent_uuid, cwd, timestamp)
}

fn compute_hash(raw_line: &[u8], suffix: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw_line);
    if !suffix.is_empty() {
        hasher.update(suffix.as_bytes());
    }
    hex::encode(hasher.finalize())
}

fn parse_user_entry(v: &Value, raw_line: &[u8], project_path: &str, machine_id: &str) -> Vec<ParsedEntry> {
    // Skip meta messages
    if v.get("isMeta").and_then(|b| b.as_bool()).unwrap_or(false) {
        return vec![];
    }

    let content = v.get("message")
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");

    // Skip command/system messages
    if content.is_empty()
        || content.starts_with("<local-command-")
        || content.starts_with("<command-")
        || content.starts_with("<local-command-stdout>")
        || content.starts_with("<local-command-caveat>")
    {
        return vec![];
    }

    let (session_id, slug, source_uuid, parent_uuid, cwd, timestamp) =
        common_fields(v, project_path, machine_id);

    vec![ParsedEntry {
        payload_hash: compute_hash(raw_line, ""),
        session_id,
        message_type: "user".to_string(),
        content_type: "text".to_string(),
        raw_content: content.to_string(),
        timestamp,
        project_path: project_path.to_string(),
        client_machine_id: machine_id.to_string(),
        slug,
        source_uuid,
        parent_uuid,
        tool_name: None,
        cwd,
        git_branch: v.get("gitBranch").and_then(|s| s.as_str()).map(String::from),
    }]
}

fn parse_assistant_entry(v: &Value, raw_line: &[u8], project_path: &str, machine_id: &str) -> Vec<ParsedEntry> {
    let content_blocks = match v.get("message").and_then(|m| m.get("content")).and_then(|c| c.as_array()) {
        Some(arr) => arr,
        None => return vec![],
    };

    let (session_id, slug, source_uuid, parent_uuid, cwd, timestamp) =
        common_fields(v, project_path, machine_id);
    let git_branch = v.get("gitBranch").and_then(|s| s.as_str()).map(String::from);

    let mut entries = Vec::new();

    for (i, block) in content_blocks.iter().enumerate() {
        let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match block_type {
            "text" => {
                let text = block.get("text").and_then(|t| t.as_str()).unwrap_or("");
                if text.is_empty() {
                    continue;
                }
                entries.push(ParsedEntry {
                    payload_hash: compute_hash(raw_line, &format!(":text:{i}")),
                    session_id: session_id.clone(),
                    message_type: "assistant".to_string(),
                    content_type: "text".to_string(),
                    raw_content: text.to_string(),
                    timestamp: timestamp.clone(),
                    project_path: project_path.to_string(),
                    client_machine_id: machine_id.to_string(),
                    slug: slug.clone(),
                    source_uuid: source_uuid.clone(),
                    parent_uuid: parent_uuid.clone(),
                    tool_name: None,
                    cwd: cwd.clone(),
                    git_branch: git_branch.clone(),
                });
            }
            "tool_use" => {
                let name = block.get("name").and_then(|n| n.as_str()).unwrap_or("unknown");
                let input = block.get("input").map(|i| i.to_string()).unwrap_or_default();
                if input.is_empty() {
                    continue;
                }
                entries.push(ParsedEntry {
                    payload_hash: compute_hash(raw_line, &format!(":tool_use:{i}")),
                    session_id: session_id.clone(),
                    message_type: "assistant".to_string(),
                    content_type: "tool_use".to_string(),
                    raw_content: input,
                    timestamp: timestamp.clone(),
                    project_path: project_path.to_string(),
                    client_machine_id: machine_id.to_string(),
                    slug: slug.clone(),
                    source_uuid: source_uuid.clone(),
                    parent_uuid: parent_uuid.clone(),
                    tool_name: Some(name.to_string()),
                    cwd: cwd.clone(),
                    git_branch: git_branch.clone(),
                });
            }
            "tool_result" => {
                // tool_result content can be a string or array of blocks
                let text = if let Some(s) = block.get("content").and_then(|c| c.as_str()) {
                    s.to_string()
                } else if let Some(arr) = block.get("content").and_then(|c| c.as_array()) {
                    arr.iter()
                        .filter_map(|b| {
                            if b.get("type").and_then(|t| t.as_str()) == Some("text") {
                                b.get("text").and_then(|t| t.as_str()).map(String::from)
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    continue;
                };
                if text.is_empty() {
                    continue;
                }
                entries.push(ParsedEntry {
                    payload_hash: compute_hash(raw_line, &format!(":tool_result:{i}")),
                    session_id: session_id.clone(),
                    message_type: "assistant".to_string(),
                    content_type: "tool_result".to_string(),
                    raw_content: text,
                    timestamp: timestamp.clone(),
                    project_path: project_path.to_string(),
                    client_machine_id: machine_id.to_string(),
                    slug: slug.clone(),
                    source_uuid: source_uuid.clone(),
                    parent_uuid: parent_uuid.clone(),
                    tool_name: None,
                    cwd: cwd.clone(),
                    git_branch: git_branch.clone(),
                });
            }
            // Skip "thinking" and anything else
            _ => {}
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_project_path() {
        assert_eq!(decode_project_path("-home-mikey-memlayer"), "/home/mikey/memlayer");
        assert_eq!(decode_project_path("-home-mikey-projects-agentsmith"), "/home/mikey/projects/agentsmith");
    }

    #[test]
    fn test_skip_meta_user() {
        let line = br#"{"type":"user","isMeta":true,"sessionId":"abc","timestamp":"2024-01-01T00:00:00Z","message":{"role":"user","content":"<system>test</system>"}}"#;
        let entries = parse_jsonl_line(line, "/test", "machine");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_user_text() {
        let line = br#"{"type":"user","isMeta":false,"sessionId":"abc","uuid":"u1","timestamp":"2024-01-01T00:00:00Z","message":{"role":"user","content":"hello world"}}"#;
        let entries = parse_jsonl_line(line, "/test", "machine");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message_type, "user");
        assert_eq!(entries[0].raw_content, "hello world");
    }

    #[test]
    fn test_skip_progress() {
        let line = br#"{"type":"progress","sessionId":"abc","timestamp":"2024-01-01T00:00:00Z","data":{"type":"hook"}}"#;
        let entries = parse_jsonl_line(line, "/test", "machine");
        assert!(entries.is_empty());
    }
}
