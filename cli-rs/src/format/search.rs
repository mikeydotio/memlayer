use memlayer_common::api_types::SearchResponse;

pub fn format_search_json(results: &SearchResponse) -> String {
    let json_results: Vec<serde_json::Value> = results
        .results
        .iter()
        .map(|r| {
            serde_json::json!({
                "session_id": r.session_id,
                "project_path": r.project_path,
                "created_at": r.created_at,
                "content_type": r.content_type,
                "tool_name": r.tool_name,
                "rrf_score": r.rrf_score,
                "content": r.raw_content,
            })
        })
        .collect();

    let large_response = results.large_response.as_ref().map(|lr| {
        serde_json::json!({
            "file_id": lr.file_id,
            "size_bytes": lr.size_bytes,
            "summary": lr.summary,
            "index": lr.index,
        })
    });

    let output = serde_json::json!({
        "total": results.total,
        "count": results.results.len(),
        "search_ms": results.search_ms.round() as i64,
        "results": json_results,
        "large_response": large_response,
    });

    serde_json::to_string_pretty(&output).unwrap_or_default()
}

pub fn format_search_text(results: &SearchResponse) -> String {
    if let Some(ref lr) = results.large_response {
        return [
            format!("Found {} results (response offloaded to file)", results.total),
            String::new(),
            format!("**File ID:** `{}`", lr.file_id),
            format!("**Size:** {} bytes", lr.size_bytes),
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

    if results.results.is_empty() {
        return "No matching memories found.".to_string();
    }

    let formatted: Vec<String> = results
        .results
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let header = format!("### Result {} (score: {:.3})", i + 1, r.rrf_score);
            let tool = r
                .tool_name
                .as_ref()
                .map(|t| format!(" ({t})"))
                .unwrap_or_default();
            let meta = format!(
                "**Session:** {} | **Project:** {} | **Date:** {} | **Type:** {}{}",
                r.session_id,
                r.project_path.as_deref().unwrap_or("unknown"),
                r.created_at,
                r.content_type,
                tool,
            );
            format!("{header}\n{meta}\n\n{}", r.raw_content)
        })
        .collect();

    format!(
        "Found {} results (showing top {}, search: {}ms):\n\n{}",
        results.total,
        results.results.len(),
        results.search_ms.round() as i64,
        formatted.join("\n\n---\n\n"),
    )
}
