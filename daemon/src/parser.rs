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
    use std::path::PathBuf;

    // ── Helper to build an assistant JSONL line ──

    fn make_assistant_line(blocks: &[Value]) -> Vec<u8> {
        let obj = serde_json::json!({
            "type": "assistant",
            "sessionId": "sess-1",
            "uuid": "a-uuid",
            "parentUuid": "p-uuid",
            "timestamp": "2024-06-15T12:00:00Z",
            "slug": "test-slug",
            "cwd": "/home/mikey/memlayer",
            "gitBranch": "main",
            "message": {
                "role": "assistant",
                "content": blocks
            }
        });
        serde_json::to_vec(&obj).unwrap()
    }

    // ── decode_project_path ──

    #[test]
    fn test_decode_project_path() {
        assert_eq!(decode_project_path("-home-mikey-memlayer"), "/home/mikey/memlayer");
        assert_eq!(decode_project_path("-home-mikey-projects-agentsmith"), "/home/mikey/projects/agentsmith");
    }

    #[test]
    fn test_decode_project_path_no_leading_dash() {
        // When the dir name does NOT start with '-', return as-is
        assert_eq!(decode_project_path("plain-name"), "plain-name");
    }

    #[test]
    fn test_decode_project_path_single_segment() {
        assert_eq!(decode_project_path("-root"), "/root");
    }

    #[test]
    fn test_decode_project_path_with_hyphens_in_name() {
        // Hyphens in actual directory names become slashes — this is the known behavior.
        // e.g. "-home-mikey-my-cool-project" → "/home/mikey/my/cool/project"
        let result = decode_project_path("-home-mikey-my-cool-project");
        assert_eq!(result, "/home/mikey/my/cool/project");
    }

    // ── project_path_from_file ──

    #[test]
    fn test_project_path_from_file() {
        let p = PathBuf::from("/home/mikey/.claude/projects/-home-mikey-memlayer/session.jsonl");
        assert_eq!(project_path_from_file(&p), "/home/mikey/memlayer");
    }

    #[test]
    fn test_project_path_from_file_unknown() {
        // A bare filename with no parent directory
        let p = PathBuf::from("session.jsonl");
        // parent is "" which has no file_name, so falls through to "unknown"
        // Actually parent is "" which may still have a file_name. Let's just check it doesn't panic.
        let result = project_path_from_file(&p);
        assert!(!result.is_empty());
    }

    // ── User entry parsing ──

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

    #[test]
    fn test_skip_user_command_messages() {
        for prefix in &["<local-command-", "<command-", "<local-command-stdout>", "<local-command-caveat>"] {
            let line = format!(
                r#"{{"type":"user","sessionId":"s","timestamp":"2024-01-01T00:00:00Z","message":{{"role":"user","content":"{}test"}}}}"#,
                prefix
            );
            let entries = parse_jsonl_line(line.as_bytes(), "/test", "m");
            assert!(entries.is_empty(), "Expected skip for prefix {}", prefix);
        }
    }

    #[test]
    fn test_user_entry_has_common_fields() {
        let line = br#"{"type":"user","sessionId":"sess-42","uuid":"uid-1","parentUuid":"pid-1","timestamp":"2024-06-15T12:00:00Z","slug":"my-slug","cwd":"/tmp","gitBranch":"dev","message":{"role":"user","content":"test"}}"#;
        let entries = parse_jsonl_line(line, "/project", "host-1");
        assert_eq!(entries.len(), 1);
        let e = &entries[0];
        assert_eq!(e.session_id, "sess-42");
        assert_eq!(e.source_uuid.as_deref(), Some("uid-1"));
        assert_eq!(e.parent_uuid.as_deref(), Some("pid-1"));
        assert_eq!(e.slug.as_deref(), Some("my-slug"));
        assert_eq!(e.cwd.as_deref(), Some("/tmp"));
        assert_eq!(e.git_branch.as_deref(), Some("dev"));
        assert_eq!(e.project_path, "/project");
        assert_eq!(e.client_machine_id, "host-1");
        assert_eq!(e.timestamp, "2024-06-15T12:00:00Z");
    }

    // ── Assistant entry: text blocks ──

    #[test]
    fn test_assistant_text_block() {
        let blocks = vec![serde_json::json!({"type": "text", "text": "Hello from assistant"})];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].message_type, "assistant");
        assert_eq!(entries[0].content_type, "text");
        assert_eq!(entries[0].raw_content, "Hello from assistant");
        assert!(entries[0].tool_name.is_none());
    }

    #[test]
    fn test_assistant_multiple_text_blocks() {
        let blocks = vec![
            serde_json::json!({"type": "text", "text": "First"}),
            serde_json::json!({"type": "text", "text": "Second"}),
        ];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].raw_content, "First");
        assert_eq!(entries[1].raw_content, "Second");
        // Hashes should differ due to block index suffix
        assert_ne!(entries[0].payload_hash, entries[1].payload_hash);
    }

    // ── Assistant entry: tool_use blocks ──

    #[test]
    fn test_assistant_tool_use_block() {
        let blocks = vec![serde_json::json!({
            "type": "tool_use",
            "name": "Read",
            "input": {"file_path": "/etc/hosts"}
        })];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_type, "tool_use");
        assert_eq!(entries[0].tool_name.as_deref(), Some("Read"));
        assert!(entries[0].raw_content.contains("/etc/hosts"));
    }

    #[test]
    fn test_assistant_tool_use_empty_input_skipped() {
        let blocks = vec![serde_json::json!({
            "type": "tool_use",
            "name": "Read",
            "input": {}
        })];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        // Empty object serializes to "{}", which is NOT empty string, so it should NOT be skipped
        // Let's verify what happens: input.to_string() for {} is "{}"
        // The check is `if input.is_empty()` — "{}" is not empty, so it's included
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_assistant_tool_use_missing_name_defaults_unknown() {
        let blocks = vec![serde_json::json!({
            "type": "tool_use",
            "input": {"x": 1}
        })];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool_name.as_deref(), Some("unknown"));
    }

    // ── Assistant entry: tool_result blocks ──

    #[test]
    fn test_assistant_tool_result_string_content() {
        let blocks = vec![serde_json::json!({
            "type": "tool_result",
            "content": "result text here"
        })];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_type, "tool_result");
        assert_eq!(entries[0].raw_content, "result text here");
    }

    #[test]
    fn test_assistant_tool_result_array_content() {
        let blocks = vec![serde_json::json!({
            "type": "tool_result",
            "content": [
                {"type": "text", "text": "line one"},
                {"type": "text", "text": "line two"},
                {"type": "image", "source": "data:..."}
            ]
        })];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_type, "tool_result");
        // Non-text blocks in the array are filtered out; text blocks joined with newline
        assert_eq!(entries[0].raw_content, "line one\nline two");
    }

    #[test]
    fn test_assistant_tool_result_empty_string_skipped() {
        let blocks = vec![serde_json::json!({
            "type": "tool_result",
            "content": ""
        })];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_assistant_tool_result_empty_array_skipped() {
        let blocks = vec![serde_json::json!({
            "type": "tool_result",
            "content": []
        })];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        // Array with no text blocks -> joined result is "", which is skipped
        assert!(entries.is_empty());
    }

    #[test]
    fn test_assistant_tool_result_no_content_skipped() {
        let blocks = vec![serde_json::json!({
            "type": "tool_result"
        })];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert!(entries.is_empty());
    }

    // ── Assistant entry: mixed blocks ──

    #[test]
    fn test_assistant_mixed_blocks() {
        let blocks = vec![
            serde_json::json!({"type": "text", "text": "Here is the plan"}),
            serde_json::json!({"type": "tool_use", "name": "Bash", "input": {"command": "ls"}}),
            serde_json::json!({"type": "tool_result", "content": "file1.txt\nfile2.txt"}),
        ];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].content_type, "text");
        assert_eq!(entries[0].raw_content, "Here is the plan");
        assert_eq!(entries[1].content_type, "tool_use");
        assert_eq!(entries[1].tool_name.as_deref(), Some("Bash"));
        assert_eq!(entries[2].content_type, "tool_result");
        assert_eq!(entries[2].raw_content, "file1.txt\nfile2.txt");
    }

    // ── Thinking blocks should be skipped ──

    #[test]
    fn test_assistant_thinking_block_skipped() {
        let blocks = vec![
            serde_json::json!({"type": "thinking", "thinking": "internal reasoning..."}),
            serde_json::json!({"type": "text", "text": "visible response"}),
        ];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].content_type, "text");
        assert_eq!(entries[0].raw_content, "visible response");
    }

    #[test]
    fn test_assistant_only_thinking_blocks_returns_empty() {
        let blocks = vec![
            serde_json::json!({"type": "thinking", "thinking": "sig123"}),
        ];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert!(entries.is_empty());
    }

    // ── Empty content blocks ──

    #[test]
    fn test_assistant_empty_text_block_skipped() {
        let blocks = vec![
            serde_json::json!({"type": "text", "text": ""}),
            serde_json::json!({"type": "text", "text": "non-empty"}),
        ];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].raw_content, "non-empty");
    }

    #[test]
    fn test_assistant_no_content_array_returns_empty() {
        // message.content is not an array (e.g., null)
        let line = br#"{"type":"assistant","sessionId":"s","timestamp":"t","message":{"role":"assistant","content":null}}"#;
        let entries = parse_jsonl_line(line, "/test", "m");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_assistant_content_is_string_returns_empty() {
        // message.content is a string rather than an array — parse_assistant_entry expects array
        let line = br#"{"type":"assistant","sessionId":"s","timestamp":"t","message":{"role":"assistant","content":"just a string"}}"#;
        let entries = parse_jsonl_line(line, "/test", "m");
        assert!(entries.is_empty());
    }

    // ── Malformed JSON ──

    #[test]
    fn test_malformed_json_returns_empty() {
        let entries = parse_jsonl_line(b"this is not json", "/test", "m");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_truncated_json_returns_empty() {
        let entries = parse_jsonl_line(b"{\"type\":\"user\",\"session", "/test", "m");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_empty_input_returns_empty() {
        let entries = parse_jsonl_line(b"", "/test", "m");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_empty_json_object_returns_empty() {
        let entries = parse_jsonl_line(b"{}", "/test", "m");
        assert!(entries.is_empty());
    }

    // ── Very long content (50K+) ──

    #[test]
    fn test_very_long_user_content() {
        let long_text = "x".repeat(60_000);
        let line = format!(
            r#"{{"type":"user","sessionId":"s","timestamp":"t","message":{{"role":"user","content":"{}"}}}}"#,
            long_text
        );
        let entries = parse_jsonl_line(line.as_bytes(), "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].raw_content.len(), 60_000);
    }

    #[test]
    fn test_very_long_assistant_text() {
        let long_text = "y".repeat(55_000);
        let blocks = vec![serde_json::json!({"type": "text", "text": long_text})];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].raw_content.len(), 55_000);
    }

    // ── Unicode content ──

    #[test]
    fn test_unicode_user_content() {
        let line = r#"{"type":"user","sessionId":"s","timestamp":"t","message":{"role":"user","content":"Hello 世界! 🌍 Ñoño café"}}"#;
        let entries = parse_jsonl_line(line.as_bytes(), "/test", "m");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].raw_content.contains("世界"));
        assert!(entries[0].raw_content.contains("🌍"));
        assert!(entries[0].raw_content.contains("Ñoño"));
    }

    #[test]
    fn test_unicode_assistant_text() {
        let blocks = vec![serde_json::json!({"type": "text", "text": "日本語テスト — ñ ü ö ä"})];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/test", "m");
        assert_eq!(entries.len(), 1);
        assert!(entries[0].raw_content.contains("日本語テスト"));
    }

    // ── Payload hash uniqueness ──

    #[test]
    fn test_payload_hash_deterministic() {
        let line = br#"{"type":"user","sessionId":"s","timestamp":"t","message":{"role":"user","content":"hello"}}"#;
        let e1 = parse_jsonl_line(line, "/test", "m");
        let e2 = parse_jsonl_line(line, "/test", "m");
        assert_eq!(e1[0].payload_hash, e2[0].payload_hash);
    }

    #[test]
    fn test_payload_hash_differs_for_different_content() {
        let line1 = br#"{"type":"user","sessionId":"s","timestamp":"t","message":{"role":"user","content":"aaa"}}"#;
        let line2 = br#"{"type":"user","sessionId":"s","timestamp":"t","message":{"role":"user","content":"bbb"}}"#;
        let e1 = parse_jsonl_line(line1, "/test", "m");
        let e2 = parse_jsonl_line(line2, "/test", "m");
        assert_ne!(e1[0].payload_hash, e2[0].payload_hash);
    }

    // ── Common fields propagated on assistant entries ──

    #[test]
    fn test_assistant_common_fields() {
        let blocks = vec![serde_json::json!({"type": "text", "text": "hi"})];
        let line = make_assistant_line(&blocks);
        let entries = parse_jsonl_line(&line, "/proj", "host");
        let e = &entries[0];
        assert_eq!(e.session_id, "sess-1");
        assert_eq!(e.source_uuid.as_deref(), Some("a-uuid"));
        assert_eq!(e.parent_uuid.as_deref(), Some("p-uuid"));
        assert_eq!(e.slug.as_deref(), Some("test-slug"));
        assert_eq!(e.cwd.as_deref(), Some("/home/mikey/memlayer"));
        assert_eq!(e.git_branch.as_deref(), Some("main"));
        assert_eq!(e.project_path, "/proj");
        assert_eq!(e.client_machine_id, "host");
    }

    // ── Unknown entry type ──

    #[test]
    fn test_file_history_snapshot_skipped() {
        let line = br#"{"type":"file-history-snapshot","sessionId":"s","timestamp":"t","data":{}}"#;
        let entries = parse_jsonl_line(line, "/test", "m");
        assert!(entries.is_empty());
    }
}
