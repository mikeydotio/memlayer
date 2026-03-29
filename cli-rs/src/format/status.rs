pub fn format_status_json(health: &serde_json::Value, embeddings: &serde_json::Value) -> String {
    let output = serde_json::json!({
        "health": health,
        "embeddings": embeddings,
    });
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_status_text(health: &serde_json::Value, embeddings: &serde_json::Value) -> String {
    let health_json = serde_json::to_string_pretty(health).unwrap_or_default();
    let embeddings_json = serde_json::to_string_pretty(embeddings).unwrap_or_default();
    format!("## Health\n```json\n{health_json}\n```\n\n## Embeddings\n```json\n{embeddings_json}\n```")
}
