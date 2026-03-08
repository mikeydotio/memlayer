use std::path::PathBuf;

#[derive(Debug, Clone)]
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
