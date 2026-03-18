# Configuration

All configuration is via environment variables. No config files are required.

## Server Variables

Set these in your `.env` file (used by Docker Compose):

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_PASSWORD` | *(required)* | PostgreSQL database password |
| `MEMLAYER_AUTH_TOKEN` | *(required)* | Shared bearer token for API authentication |
| `MEMLAYER_BIND_ADDR` | `0.0.0.0` | IP address to bind the server port (use a Tailscale IP for secure remote access) |
| `OPENAI_API_KEY` | *(empty)* | Enables OpenAI vector embeddings for semantic search |
| `EMBEDDING_PROVIDER` | `openai` | Embedding provider: `openai` or `ollama` |
| `EMBEDDING_MODEL` | `text-embedding-3-small` | Embedding model name |
| `EMBEDDING_DIMENSIONS` | `1536` | Embedding vector dimensions |
| `OLLAMA_BASE_URL` | `http://host.docker.internal:11434` | Ollama API URL (when using Ollama embeddings) |
| `FILE_STORAGE_SOFT_LIMIT` | `0` (unlimited) | Soft limit in bytes for response file storage; background eviction starts here |
| `FILE_STORAGE_HARD_LIMIT` | `0` (unlimited) | Hard limit in bytes for response file storage; synchronous eviction on write |
| `RESPONSE_BUDGET_BYTES` | `200000` | Response size budget in bytes (200KB); responses exceeding this use file-based flow |
| `EVICTION_INTERVAL_SECS` | `60` | Seconds between background eviction checks |
| `INDEX_MODE` | `off` | Structural indexing for large files: `off`, `hybrid`, or `llm-only` |
| `INDEX_LLM_PROVIDER` | *(empty)* | LLM provider for indexing: `openai`, `anthropic`, or `ollama` |
| `INDEX_LLM_MODEL` | *(empty)* | LLM model name for indexing |
| `ANTHROPIC_API_KEY` | *(empty)* | API key for Anthropic-based indexing |
| `LOG_FORMAT` | `text` | Log output format: `text` or `json` |
| `LOG_LEVEL` | `INFO` | Server log level: `DEBUG`, `INFO`, `WARNING`, `ERROR` |

## Daemon Variables

Set these in the systemd/launchd service (handled automatically by `setup_client.sh`):

| Variable | Default | Description |
|----------|---------|-------------|
| `MEMLAYER_SERVER_URL` | `http://localhost:8420/api` | Server API URL (must include `/api`) |
| `MEMLAYER_AUTH_TOKEN` | *(required)* | Shared bearer token |
| `MEMLAYER_WATCH_PATH` | `~/.claude/projects` | Directory to watch for JSONL files |
| `MEMLAYER_BATCH_SIZE` | `50` | Number of entries per HTTP batch |
| `MEMLAYER_DATA_DIR` | `~/.local/share/memlayer` | Directory for cursor database and offline queue |
| `MEMLAYER_MACHINE_ID` | *(hostname)* | Machine identifier for multi-client tracking |
| `RUST_LOG` | `info` | Daemon log level (`debug`, `info`, `warn`, `error`) |

## CLI Variables

Set these as environment variables or in `~/.config/memlayer/env` (handled automatically by `setup_client.sh`):

| Variable | Default | Description |
|----------|---------|-------------|
| `MEMLAYER_SERVER_URL` | `http://localhost:8420/api` | Server API URL |
| `MEMLAYER_AUTH_TOKEN` | *(required)* | Shared bearer token |

The CLI reads `~/.config/memlayer/env` as a fallback when environment variables are not set.
