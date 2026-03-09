# Memlayer

Claude Code Memory Layer — persistent, searchable conversation memory.

## Architecture

- **Daemon** (`daemon/`): Rust binary that tails `~/.claude/projects/**/*.jsonl` and sends parsed entries to the server
- **Server** (`server/`): Python/FastAPI API that ingests entries, generates embeddings, and serves hybrid search
- **Database**: PostgreSQL 16 + pgvector, managed via Docker Compose
- **MCP Server** (`mcp/`): TypeScript MCP server exposing `search_memory` and `get_session_summary` tools
- **Skill** (`skill/memory.md`): Claude skill that teaches when to use memory tools

## Development

```bash
# Start the stack
docker compose up -d

# Rebuild server after changes
docker compose up server --build -d

# Build daemon
cd daemon && cargo build --release

# Run daemon
MEMLAYER_SERVER_URL="http://localhost:8420/api" MEMLAYER_AUTH_TOKEN="..." ./daemon/target/release/claude-mem-daemon

# Build MCP server
cd mcp && npm install && npx tsc

# Run tests
cd daemon && cargo test
```

## Environment Variables

| Variable | Component | Description |
|----------|-----------|-------------|
| `MEMLAYER_SERVER_URL` | daemon, mcp | Server API URL (default: `http://localhost:8420/api`) |
| `MEMLAYER_AUTH_TOKEN` | daemon, mcp, server | Shared bearer token |
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
- `mcp/src/index.ts` — MCP tool definitions (`search_memory`, `get_session_summary`, `read_memory_file`)
- `mcp/src/file-cache.ts` — Local file cache for large response files
