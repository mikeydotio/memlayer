# Memlayer

Claude Code Memory Layer — persistent, searchable conversation memory with knowledge graph.

## Architecture

- **Daemon** (`daemon/`): Rust binary that tails `~/.claude/projects/**/*.jsonl` and sends parsed entries to the server
- **Server** (`server/`): Python/FastAPI API that ingests entries, generates embeddings, extracts entities/relationships, and serves hybrid + graph-augmented search
- **Database**: PostgreSQL 16 + pgvector + pg_trgm, managed via Docker Compose
- **CLI** (`cli-rs/`): Rust CLI binary (`memlayer`) for searching, recalling conversations, browsing entities, and interactive TUI dashboard
- **Shared** (`memlayer-common/`): Shared Rust crate for config, API types, HTTP client, and file cache
- **Plugin** (`plugin/`): Claude Code plugin with recall, health, and graph skills plus read-augmentation hook
- **Knowledge Graph** (`server/src/extraction/`): LLM-based entity extraction, multi-stage entity resolution, and typed relationship tracking (supports, contradicts, supersedes, depends_on, etc.)

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
| `EXTRACTION_MODE` | server | Knowledge graph extraction: `off` (default), `auto`, `on` |
| `EXTRACTION_LLM_PROVIDER` | server | LLM for entity extraction: `openai`, `anthropic`, `ollama` |
| `EXTRACTION_LLM_MODEL` | server | Model name for extraction (e.g. `gpt-4o-mini`, `claude-haiku-4-5-20251001`) |
| `MEMLAYER_STRICT_VERSION_CHECK` | server | Hard-reject incompatible client major versions (`true`/`false`, default `false`) |
| `MEMLAYER_MIGRATION_DRY_RUN` | server | Validate migrations without applying (`true`/`false`) |
| `MEMLAYER_BACKUP_DIR` | server | Directory for pre-migration pg_dump backups (default `/data/backups`) |
| `MIN_CLIENT_VERSION` | server | Minimum client version required (e.g. `2.1.0`); clients below this are rejected with 426 |
| `NOTIFICATION_EMAIL` | server | Email address for incompatible client alerts (empty = disabled) |
| `NOTIFICATION_SMTP_HOST` | server | SMTP server for notifications |
| `NOTIFICATION_SMTP_PORT` | server | SMTP port (default 587) |
| `NOTIFICATION_SMTP_USER` | server | SMTP username |
| `NOTIFICATION_SMTP_PASSWORD` | server | SMTP password |

## Key Files

- `db/migrations/` — SQL schema, indexes, RRF hybrid search function, knowledge graph tables
- `server/src/main.py` — Server entrypoint, lifespan, auth + version negotiation middleware
- `server/src/migrator.py` — Safe migration system (transactions, pre-flight checks, backups, dry-run, fingerprinting)
- `server/src/version.py` — Version parsing, compatibility checking, feature registry
- `server/src/notifications.py` — Email notifications for incompatible clients
- `server/src/routes/ingest.py` — Ingestion endpoint
- `server/src/routes/search.py` — Hybrid search + graph-augmented search + session summary endpoints
- `server/src/routes/version.py` — Version info and update manifest endpoints
- `server/src/routes/graph.py` — Knowledge graph API (entity CRUD, relationships, traversal, stats)
- `server/src/routes/files.py` — File download endpoint for large responses
- `server/src/embeddings.py` — OpenAI/Ollama embedding providers
- `server/src/extraction/` — Knowledge graph extraction pipeline (LLM-based entity/relationship extraction, entity resolution)
- `server/src/file_storage.py` — Response file storage + LRU eviction
- `server/src/eviction.py` — Background eviction worker
- `server/src/indexing/` — Content detection, heuristic + LLM indexing
- `daemon/src/parser.rs` — JSONL parsing and content extraction
- `daemon/src/watcher.rs` — File watcher with cursor tracking
- `daemon/src/sender.rs` — Batch sender with version negotiation, read-only detection, offline queue
- `daemon/src/updater.rs` — Self-update mechanism (download, verify, archive, replace)
- `cli-rs/src/main.rs` — CLI entrypoint (`memlayer search`, `memlayer session`, `memlayer recent`, `memlayer entities`, `memlayer entity`, `memlayer graph`, `memlayer read-file`, `memlayer status`, `memlayer update`, `memlayer rollback`, `memlayer dashboard`)
- `cli-rs/src/tui/` — TUI dashboard (ratatui-based, tab layout: Browse, Search, Live, Stats, Graph)
- `memlayer-common/src/client.rs` — Shared HTTP client with version headers and server info parsing
- `memlayer-common/src/api_types.rs` — Shared API types including version negotiation types
- `memlayer-common/src/config.rs` — Shared config loading (env + dotenv file)
- `memlayer-common/src/file_cache.rs` — Local file cache for large response files
- `scripts/sync-version.sh` — Synchronize VERSION file across all components
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
