pub fn format_status_json(
    health: &serde_json::Value,
    embeddings: &serde_json::Value,
    version: &serde_json::Value,
    daemon_error: Option<&str>,
) -> String {
    let mut output = serde_json::json!({
        "client_version": env!("CARGO_PKG_VERSION"),
        "server": version,
        "health": health,
        "embeddings": embeddings,
    });
    if let Some(err) = daemon_error {
        output["daemon_version_error"] = serde_json::Value::String(err.to_string());
    }
    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_status_text(
    health: &serde_json::Value,
    embeddings: &serde_json::Value,
    version: &serde_json::Value,
    daemon_error: Option<&str>,
) -> String {
    let mut sections = Vec::new();

    // Version info
    let client_ver = env!("CARGO_PKG_VERSION");
    let server_ver = version.get("server_version")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let schema_ver = version.get("schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let read_only = version.get("read_only")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    sections.push(format!(
        "## Version\n  Client: {client_ver}\n  Server: {server_ver}\n  Schema: {schema_ver}\n  Read-only: {read_only}"
    ));

    // Daemon version error
    if let Some(err) = daemon_error {
        sections.push(format!(
            "## WARNING: Daemon Version Error\n{err}"
        ));
    }

    // Health
    let health_json = serde_json::to_string_pretty(health).unwrap_or_default();
    sections.push(format!("## Health\n```json\n{health_json}\n```"));

    // Embeddings
    let embeddings_json = serde_json::to_string_pretty(embeddings).unwrap_or_default();
    sections.push(format!("## Embeddings\n```json\n{embeddings_json}\n```"));

    sections.join("\n\n")
}
