use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Config {
    pub server_url: String,
    pub auth_token: String,
    pub cache_dir: PathBuf,
}

impl Config {
    /// Load config from env vars, falling back to `~/.config/memlayer/env` dotenv file.
    /// Env vars take precedence. Default server URL: `http://localhost:8420/api`.
    pub fn load() -> Self {
        let mut server_url = std::env::var("MEMLAYER_SERVER_URL").unwrap_or_default();
        let mut auth_token = std::env::var("MEMLAYER_AUTH_TOKEN").unwrap_or_default();

        if server_url.is_empty() || auth_token.is_empty() {
            if let Some(home) = dirs::home_dir() {
                let env_file = home.join(".config/memlayer/env");

                // Warn if config file has overly permissive permissions
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(&env_file) {
                        let mode = meta.permissions().mode();
                        if mode & 0o077 != 0 {
                            eprintln!(
                                "warning: {} is accessible by other users (mode {:o}). \
                                 Run: chmod 600 {}",
                                env_file.display(),
                                mode & 0o777,
                                env_file.display(),
                            );
                        }
                    }
                }

                if let Ok(contents) = std::fs::read_to_string(&env_file) {
                    for line in contents.lines() {
                        let trimmed = line.trim();
                        if trimmed.is_empty() || trimmed.starts_with('#') {
                            continue;
                        }
                        if let Some(eq) = trimmed.find('=') {
                            let key = &trimmed[..eq];
                            let val = &trimmed[eq + 1..];
                            if key == "MEMLAYER_SERVER_URL" && server_url.is_empty() {
                                server_url = val.to_string();
                            }
                            if key == "MEMLAYER_AUTH_TOKEN" && auth_token.is_empty() {
                                auth_token = val.to_string();
                            }
                        }
                    }
                }
            }
        }

        if server_url.is_empty() {
            server_url = "http://localhost:8420/api".to_string();
        }

        let cache_dir = std::env::var("MEMLAYER_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("/tmp"))
                    .join(".claude/memlayer/cache")
            });

        Config {
            server_url,
            auth_token,
            cache_dir,
        }
    }
}
