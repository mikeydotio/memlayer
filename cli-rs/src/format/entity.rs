use memlayer_common::api_types::EntityDetail;

pub fn format_entity_json(detail: &EntityDetail) -> String {
    serde_json::to_string_pretty(detail).unwrap_or_else(|_| "{}".to_string())
}

pub fn format_entity_text(detail: &EntityDetail, _show_neighbors: bool) -> String {
    let e = &detail.entity;
    let mut lines = vec![
        format!("Entity #{}: {}", e.id, e.canonical_name),
        format!("  Type:       {}", e.entity_type),
        format!("  Status:     {}", e.status),
        format!("  Confidence: {:.2}", e.confidence),
        format!("  Mentions:   {}", e.mention_count),
        format!("  First seen: {}", e.first_seen_at),
        format!("  Last seen:  {}", e.last_seen_at),
    ];

    if let Some(ref desc) = e.description {
        lines.push(format!("  Description: {desc}"));
    }
    if let Some(ref project) = e.project_path {
        lines.push(format!("  Project:    {project}"));
    }

    if !detail.aliases.is_empty() {
        lines.push(String::new());
        lines.push("  Aliases:".to_string());
        for a in &detail.aliases {
            lines.push(format!("    - {}", a.alias));
        }
    }

    if !detail.relationships.is_empty() {
        lines.push(String::new());
        lines.push("  Relationships:".to_string());
        for r in &detail.relationships {
            let arrow = if r.direction == "outgoing" { "-->" } else { "<--" };
            let validity = if r.valid_until.is_some() { " [expired]" } else { "" };
            lines.push(format!(
                "    {} [{}] {} ({:.0}%){validity}",
                arrow, r.relationship_type, r.related_entity.canonical_name,
                r.confidence * 100.0,
            ));
        }
    }

    if !detail.mentions.is_empty() {
        lines.push(String::new());
        lines.push(format!("  Recent mentions ({}):", detail.mentions.len()));
        for m in detail.mentions.iter().take(10) {
            let text = m.mention_text.as_deref().unwrap_or("(no text)");
            lines.push(format!("    [{}] {} - {}", m.session_id, m.created_at, text));
        }
    }

    lines.join("\n")
}
