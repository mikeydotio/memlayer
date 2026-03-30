use memlayer_common::api_types::EntitiesPage;

pub fn format_entities_json(page: &EntitiesPage) -> String {
    serde_json::to_string_pretty(page).unwrap_or_else(|_| "{}".to_string())
}

pub fn format_entities_text(page: &EntitiesPage) -> String {
    if page.entities.is_empty() {
        return "No entities found.".to_string();
    }

    let mut lines = vec![format!("Entities ({} total):\n", page.total)];
    for e in &page.entities {
        let desc = e.description.as_deref().unwrap_or("");
        let desc_short = if desc.len() > 60 { &desc[..60] } else { desc };
        lines.push(format!(
            "  #{:<5} [{}] {} (mentions: {}) {}",
            e.id, e.entity_type, e.canonical_name, e.mention_count,
            if desc_short.is_empty() { String::new() } else { format!("- {desc_short}") }
        ));
    }
    lines.join("\n")
}
