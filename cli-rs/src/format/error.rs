pub fn format_error(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}
