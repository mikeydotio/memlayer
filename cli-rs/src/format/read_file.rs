pub fn format_read_file_json(
    file_id: &str,
    start_line: usize,
    end_line: usize,
    content: &str,
) -> String {
    let output = serde_json::json!({
        "file_id": file_id,
        "start_line": start_line,
        "end_line": end_line,
        "content": content,
    });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_read_file_text(
    file_id: &str,
    start_line: usize,
    end_line: usize,
    content: &str,
) -> String {
    format!("Lines {start_line}-{end_line} of file {file_id}:\n\n{content}")
}
