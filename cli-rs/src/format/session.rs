use memlayer_common::api_types::SessionSummary;

pub fn format_session_json(summary: &SessionSummary) -> String {
    serde_json::to_string_pretty(summary).unwrap_or_default()
}

pub fn format_session_text(summary: &SessionSummary) -> String {
    if let Some(ref lr) = summary.large_response {
        let header = format!(
            "## Session: {}\n**Project:** {}\n**Messages:** {}",
            summary.session_id,
            summary.project_path.as_deref().unwrap_or("unknown"),
            summary.message_count,
        );
        return [
            header,
            String::new(),
            format!("Response offloaded to file ({} bytes).", lr.size_bytes),
            String::new(),
            format!("**File ID:** `{}`", lr.file_id),
            String::new(),
            "**Summary:**".to_string(),
            lr.summary.clone(),
            String::new(),
            "**Structural Index:**".to_string(),
            lr.index.clone(),
            String::new(),
            format!(
                "Use `memlayer read-file {} --start 1 --end 50` to read sections.",
                lr.file_id
            ),
        ]
        .join("\n");
    }

    if summary.messages.is_empty() {
        return format!("No data found for session {}.", summary.session_id);
    }

    let header = format!(
        "## Session: {}\n**Project:** {}\n**Slug:** {}\n**Started:** {}\n**Messages:** {}",
        summary.session_id,
        summary.project_path.as_deref().unwrap_or("unknown"),
        summary.slug.as_deref().unwrap_or("none"),
        summary.created_at,
        summary.message_count,
    );

    let messages: Vec<String> = summary
        .messages
        .iter()
        .map(|m| {
            let role = if m.message_type == "user" {
                "Human"
            } else {
                "Assistant"
            };
            let type_tag = if m.content_type != "text" {
                format!(" [{}]", m.content_type)
            } else {
                String::new()
            };
            let tool_tag = m
                .tool_name
                .as_ref()
                .map(|t| format!(" ({t})"))
                .unwrap_or_default();
            format!(
                "**[{role}{type_tag}{tool_tag}]** ({})\n{}",
                m.created_at, m.raw_content
            )
        })
        .collect();

    format!("{header}\n\n{}", messages.join("\n\n"))
}
