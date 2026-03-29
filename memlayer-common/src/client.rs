use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};

use crate::api_types::*;
use crate::config::Config;

pub struct MemlayerClient {
    http: reqwest::Client,
    base_url: String,
    auth_token: String,
}

impl MemlayerClient {
    pub fn new(config: &Config) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        MemlayerClient {
            http,
            base_url: config.server_url.clone(),
            auth_token: config.auth_token.clone(),
        }
    }

    fn headers(&self) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !self.auth_token.is_empty() {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.auth_token)) {
                h.insert(AUTHORIZATION, val);
            }
        }
        h
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub fn auth_token(&self) -> &str {
        &self.auth_token
    }

    // ── Existing API methods (CLI parity) ───────────────────────────

    pub async fn search(&self, req: &SearchRequest) -> Result<SearchResponse, String> {
        let resp = self
            .http
            .post(format!("{}/search", self.base_url))
            .headers(self.headers())
            .json(req)
            .send()
            .await
            .map_err(|e| format!("Search request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Search failed: {status} {body}"));
        }

        resp.json::<SearchResponse>()
            .await
            .map_err(|e| format!("Failed to parse search response: {e}"))
    }

    pub async fn get_session_summary(
        &self,
        session_id: &str,
        limit: u32,
        types: Option<&[String]>,
    ) -> Result<SessionSummary, String> {
        let mut url = format!(
            "{}/sessions/{}/summary?limit={}",
            self.base_url, session_id, limit
        );
        if let Some(t) = types {
            if !t.is_empty() {
                url.push_str(&format!("&types={}", t.join(",")));
            }
        }

        let resp = self
            .http
            .get(&url)
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("Session summary request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Session summary failed: {status} {body}"));
        }

        resp.json::<SessionSummary>()
            .await
            .map_err(|e| format!("Failed to parse session summary: {e}"))
    }

    pub async fn download_file(&self, file_id: &str) -> Result<String, String> {
        let resp = self
            .http
            .get(format!("{}/files/{}", self.base_url, file_id))
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("File download request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("File download failed: {status} {body}"));
        }

        resp.text()
            .await
            .map_err(|e| format!("Failed to read file content: {e}"))
    }

    pub async fn get_health(&self) -> Result<serde_json::Value, String> {
        // Health endpoint is at /health, not /api/health
        let health_url = self.base_url.replace("/api", "") + "/health";
        let resp = self
            .http
            .get(&health_url)
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("Health request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err("unreachable".to_string());
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse health response: {e}"))
    }

    pub async fn get_embedding_status(&self) -> Result<serde_json::Value, String> {
        let resp = self
            .http
            .get(format!("{}/embeddings/status", self.base_url))
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("Embedding status request failed: {e}"))?;

        if !resp.status().is_success() {
            return Err("unreachable".to_string());
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse embedding status: {e}"))
    }

    // ── New API methods (dashboard) ─────────────────────────────────

    pub async fn get_projects(&self) -> Result<Vec<ProjectInfo>, String> {
        let resp = self
            .http
            .get(format!("{}/projects", self.base_url))
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("Projects request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Projects failed: {status} {body}"));
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse projects: {e}"))
    }

    pub async fn get_sessions(
        &self,
        project_path: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<SessionsPage, String> {
        let mut url = format!("{}/sessions?offset={}&limit={}", self.base_url, offset, limit);
        if let Some(p) = project_path {
            url.push_str(&format!(
                "&project_path={}",
                urlencoding::encode(p)
            ));
        }

        let resp = self
            .http
            .get(&url)
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("Sessions request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Sessions failed: {status} {body}"));
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse sessions: {e}"))
    }

    pub async fn get_session_entries(
        &self,
        session_id: &str,
        cursor: Option<i64>,
        limit: u32,
    ) -> Result<EntriesPage, String> {
        let mut url = format!(
            "{}/sessions/{}/entries?limit={}",
            self.base_url, session_id, limit
        );
        if let Some(c) = cursor {
            url.push_str(&format!("&cursor={}", c));
        }

        let resp = self
            .http
            .get(&url)
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("Session entries request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Session entries failed: {status} {body}"));
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse session entries: {e}"))
    }

    pub async fn get_stats(&self) -> Result<StatsResponse, String> {
        let resp = self
            .http
            .get(format!("{}/stats", self.base_url))
            .headers(self.headers())
            .send()
            .await
            .map_err(|e| format!("Stats request failed: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("Stats failed: {status} {body}"));
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse stats: {e}"))
    }
}

mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::with_capacity(s.len() * 3);
        for b in s.bytes() {
            match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(b as char);
                }
                _ => {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
        result
    }
}
