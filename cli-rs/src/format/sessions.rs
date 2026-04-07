use chrono::{DateTime, Utc};
use memlayer_common::api_types::SessionsPage;

pub fn format_sessions_json(page: &SessionsPage) -> String {
    let json_sessions: Vec<serde_json::Value> = page
        .sessions
        .iter()
        .map(|s| {
            serde_json::json!({
                "session_id": s.session_id,
                "slug": s.slug,
                "created_at": s.created_at,
                "last_seen_at": s.last_seen_at,
                "entry_count": s.entry_count,
            })
        })
        .collect();

    let output = serde_json::json!({
        "total": page.total,
        "count": page.sessions.len(),
        "sessions": json_sessions,
    });

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_sessions_text(page: &SessionsPage) -> String {
    if page.sessions.is_empty() {
        return "No sessions found.".to_string();
    }

    let rows: Vec<String> = page
        .sessions
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let slug = s.slug.as_deref().unwrap_or("unnamed");
            let short_id = if s.session_id.len() > 8 {
                &s.session_id[..8]
            } else {
                &s.session_id
            };
            let age = relative_time(&s.last_seen_at);
            format!(
                "  {}. {}  {}  ({}, {} entries)",
                i + 1,
                short_id,
                slug,
                age,
                s.entry_count,
            )
        })
        .collect();

    format!(
        "Recent sessions ({} of {}):\n\n{}",
        page.sessions.len(),
        page.total,
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
