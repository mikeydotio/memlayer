use chrono::{DateTime, Utc};
use memlayer_common::api_types::RecentEntriesPage;

pub fn format_recent_json(page: &RecentEntriesPage) -> String {
    let json_entries: Vec<serde_json::Value> = page
        .entries
        .iter()
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "session_id": e.session_id,
                "message_type": e.message_type,
                "content_type": e.content_type,
                "content_preview": e.content_preview,
                "tool_name": e.tool_name,
                "created_at": e.created_at,
                "project_path": e.project_path,
                "slug": e.slug,
            })
        })
        .collect();

    let output = serde_json::json!({
        "total": page.total,
        "count": page.entries.len(),
        "machine_id": page.machine_id,
        "entries": json_entries,
    });

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_recent_text(page: &RecentEntriesPage) -> String {
    if page.entries.is_empty() {
        return "No recent entries found.".to_string();
    }

    let host_label = match &page.machine_id {
        Some(m) => format!(" (host: {})", m),
        None => " (all hosts)".to_string(),
    };

    let rows: Vec<String> = page
        .entries
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let age = relative_time(&e.created_at);
            let type_label = if e.content_type != e.message_type {
                format!("{}/{}", e.message_type, e.content_type)
            } else {
                e.message_type.clone()
            };
            let short_session = if e.session_id.len() > 8 {
                &e.session_id[..8]
            } else {
                &e.session_id
            };
            let project = e
                .project_path
                .as_deref()
                .and_then(|p| p.rsplit('/').next())
                .unwrap_or("?");
            let preview = if e.content_preview.len() > 120 {
                format!("{}...", &e.content_preview[..120])
            } else {
                e.content_preview.clone()
            };
            // Replace newlines in preview for single-line display
            let preview = preview.replace('\n', " ");
            format!(
                "  {}. [{}] {}  (session: {} / {})\n     {}",
                i + 1,
                type_label,
                age,
                short_session,
                project,
                preview,
            )
        })
        .collect();

    format!(
        "Recent entries ({} of {}){}\n\n{}",
        page.entries.len(),
        page.total,
        host_label,
        rows.join("\n"),
    )
}

fn relative_time(iso: &str) -> String {
    let Ok(dt) = iso.parse::<DateTime<Utc>>() else {
        return iso.to_string();
    };
    let now = Utc::now();
    let delta = now.signed_duration_since(dt);

    let secs = delta.num_seconds();
    if secs < 0 {
        return "just now".to_string();
    }

    let mins = delta.num_minutes();
    let hours = delta.num_hours();
    let days = delta.num_days();

    if mins < 1 {
        "just now".to_string()
    } else if mins < 60 {
        format!("{}m ago", mins)
    } else if hours < 24 {
        format!("{}h ago", hours)
    } else if days < 30 {
        format!("{}d ago", days)
    } else {
        format!("{}mo ago", days / 30)
    }
}
