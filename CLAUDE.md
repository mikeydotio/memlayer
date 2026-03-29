# Memlayer

Claude Code Memory Layer — persistent, searchable conversation memory.

## Architecture

- **Daemon** (`daemon/`): Rust binary that tails `~/.claude/projects/**/*.jsonl` and sends parsed entries to the server
- **Server** (`server/`): Python/FastAPI API that ingests entries, generates embeddings, and serves hybrid search
- **Database**: PostgreSQL 16 + pgvector, managed via Docker Compose
- **CLI** (`cli-rs/`): Rust CLI binary (`memlayer`) for searching, recalling conversations, and interactive TUI dashboard
- **Shared** (`memlayer-common/`): Shared Rust crate for config, API types, HTTP client, and file cache
- **Plugin** (`plugin/`): Claude Code plugin with memory skill and read-augmentation hook
- **Skill** (`skill/memory.md`): Claude skill that teaches when to use the CLI

## Development

```bash
# Start the stack
docker compose up -d

# Rebuild server after changes
docker compose up server --build -d

# Build everything (workspace)
cargo build --workspace --release

# Build just the CLI
cargo build -p memlayer-cli --release

# Build just the daemon
cargo build -p memlayer-daemon --release

# Run daemon
MEMLAYER_SERVER_URL="http://localhost:8420/api" MEMLAYER_AUTH_TOKEN="..." ./target/release/memlayer-daemon

# Run CLI
./target/release/memlayer search "query"
./target/release/memlayer dashboard

# Run tests
cargo test --workspace
```

## Environment Variables

| Variable | Component | Description |
|----------|-----------|-------------|
| `MEMLAYER_SERVER_URL` | daemon, cli | Server API URL (default: `http://localhost:8420/api`) |
| `MEMLAYER_AUTH_TOKEN` | daemon, cli, server | Shared bearer token |
| `OPENAI_API_KEY` | server | For embedding generation |
| `EMBEDDING_PROVIDER` | server | `openai` or `ollama` |
| `POSTGRES_PASSWORD` | docker-compose | Database password |
| `INDEX_MODE` | server | Indexing mode: `off`, `hybrid`, `llm-only` |
| `INDEX_LLM_PROVIDER` | server | LLM for indexing: `openai`, `anthropic`, `ollama` |
| `ANTHROPIC_API_KEY` | server | For Anthropic-based indexing |
| `RESPONSE_BUDGET_BYTES` | server | Response size budget (bytes, default 200000); over-budget responses use file-based flow |
| `FILE_STORAGE_SOFT_LIMIT` | server | Soft limit for response files (bytes, 0=unlimited) |
| `FILE_STORAGE_HARD_LIMIT` | server | Hard limit for response files (bytes, 0=unlimited) |

## Key Files

- `db/migrations/` — SQL schema, indexes, RRF hybrid search function
- `server/src/routes/ingest.py` — Ingestion endpoint
- `server/src/routes/search.py` — Hybrid search + session summary endpoints + large response detection
- `server/src/routes/files.py` — File download endpoint for large responses
- `server/src/embeddings.py` — OpenAI/Ollama embedding providers
- `server/src/file_storage.py` — Response file storage + LRU eviction
- `server/src/eviction.py` — Background eviction worker
- `server/src/indexing/` — Content detection, heuristic + LLM indexing
- `daemon/src/parser.rs` — JSONL parsing and content extraction
- `daemon/src/watcher.rs` — File watcher with cursor tracking
- `cli-rs/src/main.rs` — CLI entrypoint (`memlayer search`, `memlayer session`, `memlayer read-file`, `memlayer status`, `memlayer dashboard`)
- `cli-rs/src/tui/` — TUI dashboard (ratatui-based, tab layout: Browse, Search, Live, Stats)
- `memlayer-common/src/client.rs` — Shared HTTP client for the memlayer API
- `memlayer-common/src/config.rs` — Shared config loading (env + dotenv file)
- `memlayer-common/src/file_cache.rs` — Local file cache for large response files
- `server/src/event_bus.py` — In-process pub/sub for SSE broadcasting
- `server/src/routes/stream.py` — SSE endpoint for live entry streaming
- `server/src/routes/browse.py` — Browse endpoints (projects, sessions, entries)
- `server/src/routes/stats.py` — Aggregate statistics endpoint

<!-- semver:start -->
## Versioning

This project uses semantic versioning managed by the `/semver` skill.

- **Current version**: See `VERSION` file
- **Changelog**: See `CHANGELOG.md`
- **Config**: `.semver/config.yaml`
- **Bump**: `/semver bump <major|minor|patch>`
- **Check**: `/semver current`
<!-- semver:end -->
