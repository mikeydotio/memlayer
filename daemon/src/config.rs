use std::path::PathBuf;

#[derive(Clone)]
pub struct Config {
    pub server_url: String,
    pub auth_token: String,
    pub watch_path: PathBuf,
    pub data_dir: PathBuf,
    pub machine_id: String,
    pub batch_size: usize,
    pub flush_interval_secs: u64,
    pub max_retry_delay_secs: u64,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("server_url", &self.server_url)
            .field("auth_token", &"[REDACTED]")
            .field("watch_path", &self.watch_path)
            .field("data_dir", &self.data_dir)
            .field("machine_id", &self.machine_id)
            .field("batch_size", &self.batch_size)
            .field("flush_interval_secs", &self.flush_interval_secs)
            .field("max_retry_delay_secs", &self.max_retry_delay_secs)
            .finish()
    }
}

impl Config {
    pub fn from_env() -> Self {
        let home = dirs::home_dir().expect("Cannot determine home directory");

        let watch_path = std::env::var("MEMLAYER_WATCH_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".claude/projects"));

        let data_dir = std::env::var("MEMLAYER_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::data_local_dir()
                    .unwrap_or_else(|| home.join(".local/share"))
                    .join("memlayer")
            });

        let machine_id = std::env::var("MEMLAYER_MACHINE_ID")
            .unwrap_or_else(|_| hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "unknown".to_string()));

        Config {
            server_url: std::env::var("MEMLAYER_SERVER_URL")
                .unwrap_or_else(|_| "http://localhost:8420/api".to_string()),
            auth_token: std::env::var("MEMLAYER_AUTH_TOKEN").unwrap_or_default(),
            watch_path,
            data_dir,
            machine_id,
            batch_size: std::env::var("MEMLAYER_BATCH_SIZE")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
            flush_interval_secs: 5,
            max_retry_delay_secs: 300,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Env var mutation is inherently racy in multi-threaded tests (Rust 2024
    // correctly marks set_var/remove_var as unsafe). To avoid flakiness we
    // consolidate all env-mutating config tests into a SINGLE test function
    // that runs sequentially within itself.  Non-mutating tests are separate.

    #[test]
    fn test_constant_fields() {
        // These fields are hard-coded, not from env, so always deterministic.
        let cfg = Config::from_env();
        assert_eq!(cfg.flush_interval_secs, 5);
        assert_eq!(cfg.max_retry_delay_secs, 300);
    }

    #[test]
    fn test_machine_id_non_empty() {
        // Even without MEMLAYER_MACHINE_ID, falls back to hostname.
        let cfg = Config::from_env();
        assert!(!cfg.machine_id.is_empty());
    }

    #[test]
    fn test_debug_redacts_auth_token() {
        // Build a Config manually to avoid env mutation.
        let cfg = Config {
            server_url: "http://localhost:8420/api".to_string(),
            auth_token: "super-secret-token".to_string(),
            watch_path: PathBuf::from("/tmp"),
            data_dir: PathBuf::from("/tmp"),
            machine_id: "test".to_string(),
            batch_size: 50,
            flush_interval_secs: 5,
            max_retry_delay_secs: 300,
        };
        let debug_output = format!("{:?}", cfg);
        assert!(!debug_output.contains("super-secret-token"), "auth token leaked in debug output");
        assert!(debug_output.contains("[REDACTED]"), "expected [REDACTED] in debug output");
    }

    #[test]
    fn test_config_clone() {
        let cfg = Config {
            server_url: "http://example.com/api".to_string(),
            auth_token: "tok".to_string(),
            watch_path: PathBuf::from("/watch"),
            data_dir: PathBuf::from("/data"),
            machine_id: "host-1".to_string(),
            batch_size: 25,
            flush_interval_secs: 5,
            max_retry_delay_secs: 300,
        };
        let cloned = cfg.clone();
        assert_eq!(cfg.server_url, cloned.server_url);
        assert_eq!(cfg.auth_token, cloned.auth_token);
        assert_eq!(cfg.watch_path, cloned.watch_path);
        assert_eq!(cfg.data_dir, cloned.data_dir);
        assert_eq!(cfg.machine_id, cloned.machine_id);
        assert_eq!(cfg.batch_size, cloned.batch_size);
    }

    /// All env-var-mutating config tests combined in a single test to avoid
    /// race conditions when cargo runs tests in parallel threads.
    #[test]
    fn test_env_var_overrides() {
        // SAFETY: Only this one test mutates MEMLAYER_* env vars, and the
        // operations within are sequential.
        unsafe {
            // ── defaults (clear everything first) ──
            std::env::remove_var("MEMLAYER_SERVER_URL");
            std::env::remove_var("MEMLAYER_AUTH_TOKEN");
            std::env::remove_var("MEMLAYER_WATCH_PATH");
            std::env::remove_var("MEMLAYER_DATA_DIR");
            std::env::remove_var("MEMLAYER_MACHINE_ID");
            std::env::remove_var("MEMLAYER_BATCH_SIZE");

            let cfg = Config::from_env();
            assert_eq!(cfg.server_url, "http://localhost:8420/api");
            assert_eq!(cfg.auth_token, "");
            assert!(cfg.watch_path.to_string_lossy().ends_with(".claude/projects"));
            assert!(cfg.data_dir.to_string_lossy().contains("memlayer"));
            assert_eq!(cfg.batch_size, 50);

            // ── custom server URL ──
            std::env::set_var("MEMLAYER_SERVER_URL", "http://custom:9999/api");
            let cfg = Config::from_env();
            assert_eq!(cfg.server_url, "http://custom:9999/api");
            std::env::remove_var("MEMLAYER_SERVER_URL");

            // ── custom auth token ──
            std::env::set_var("MEMLAYER_AUTH_TOKEN", "secret-token-123");
            let cfg = Config::from_env();
            assert_eq!(cfg.auth_token, "secret-token-123");
            std::env::remove_var("MEMLAYER_AUTH_TOKEN");

            // ── custom batch size ──
            std::env::set_var("MEMLAYER_BATCH_SIZE", "100");
            let cfg = Config::from_env();
            assert_eq!(cfg.batch_size, 100);
            std::env::remove_var("MEMLAYER_BATCH_SIZE");

            // ── invalid batch size falls back to default ──
            std::env::set_var("MEMLAYER_BATCH_SIZE", "not-a-number");
            let cfg = Config::from_env();
            assert_eq!(cfg.batch_size, 50);
            std::env::remove_var("MEMLAYER_BATCH_SIZE");

            // ── custom watch path ──
            std::env::set_var("MEMLAYER_WATCH_PATH", "/custom/watch");
            let cfg = Config::from_env();
            assert_eq!(cfg.watch_path, PathBuf::from("/custom/watch"));
            std::env::remove_var("MEMLAYER_WATCH_PATH");

            // ── custom data dir ──
            std::env::set_var("MEMLAYER_DATA_DIR", "/custom/data");
            let cfg = Config::from_env();
            assert_eq!(cfg.data_dir, PathBuf::from("/custom/data"));
            std::env::remove_var("MEMLAYER_DATA_DIR");

            // ── custom machine id ──
            std::env::set_var("MEMLAYER_MACHINE_ID", "my-custom-host");
            let cfg = Config::from_env();
            assert_eq!(cfg.machine_id, "my-custom-host");
            std::env::remove_var("MEMLAYER_MACHINE_ID");
        }
    }
}
