# memlayer

Persistent, searchable memory for [Claude Code](https://docs.anthropic.com/en/docs/claude-code) CLI sessions. Captures all conversations across every project and makes them retrievable via hybrid search through MCP tools — giving Claude cross-session recall.

```
"Do you remember how we fixed the SSH tunnel issue last week?"
```

Claude searches 6,000+ indexed conversation entries and returns the exact session where you debugged it together.

## How It Works

```
~/.claude/projects/**/*.jsonl          memlayer server (FastAPI)
         │                                    │
         ▼                                    ▼
   ┌───────────┐    HTTP/JSON    ┌──────────────────────┐
   │  daemon    │───────────────▶│  PostgreSQL 16        │
   │  (Rust)    │                │  + pgvector           │
   │  tails     │                │  FTS (GIN tsvector)   │
   │  parses    │                │  Vec (HNSW cosine)    │
   │  queues    │                │  RRF hybrid search    │
   └───────────┘                └──────────┬───────────┘
                                           │
   ┌───────────┐    search API             │
   │  Claude    │◀─────────────────────────┘
   │  Code      │   via MCP server
   │  session   │   (TypeScript, stdio)
   └───────────┘
```

**Daemon** tails Claude Code's JSONL conversation logs in real-time, parses user messages, assistant responses, tool calls, and tool results, then streams them to the server. If the server is unreachable, entries are queued locally in SQLite and drained when connectivity returns.

**Server** receives entries, stores them in PostgreSQL with full-text search vectors generated on insert, and asynchronously generates embeddings (OpenAI or Ollama). Search uses Reciprocal Rank Fusion (RRF) to combine FTS and vector results in a single stored function.

**MCP Server** exposes `search_memory` and `get_session_summary` tools to Claude Code via the Model Context Protocol. Works in FTS-only mode without an embedding API key.

## Prerequisites

- Docker and Docker Compose
- Rust 1.70+ (for the daemon)
- Node.js 18+ (for the MCP server)

## Installation

### 1. Clone and configure

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
cp .env.example .env
```

Edit `.env`:

```bash
# Required — pick a password and a shared auth token
POSTGRES_PASSWORD=your-db-password
MEMLAYER_AUTH_TOKEN=your-secret-token

# Optional — enables vector embeddings for semantic search
# Without this, search uses full-text search only (still works well)
OPENAI_API_KEY=sk-...
```

### 2. Start the database and server

```bash
docker compose up -d
```

Verify:

```bash
curl http://localhost:8420/health
# {"status":"ok"}
```

### 3. Build and run the daemon

```bash
cd daemon
cargo build --release
```

Run it:

```bash
MEMLAYER_SERVER_URL="http://localhost:8420/api" \
MEMLAYER_AUTH_TOKEN="your-secret-token" \
./target/release/claude-mem-daemon
```

On first run, the daemon scans all existing JSONL files and ingests them. Subsequent runs pick up from where they left off (byte-offset cursors stored in `~/.local/share/memlayer/cursors.db`).

To run as a background service (Linux with systemd):

```bash
mkdir -p ~/.config/systemd/user

cat > ~/.config/systemd/user/claude-mem-daemon.service << 'EOF'
[Unit]
Description=Claude Memory Layer Daemon
After=network-online.target

[Service]
Type=simple
ExecStart=%h/memlayer/daemon/target/release/claude-mem-daemon
Restart=always
RestartSec=10
Environment=MEMLAYER_SERVER_URL=http://localhost:8420/api
Environment=MEMLAYER_AUTH_TOKEN=your-secret-token
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now claude-mem-daemon
```

### 4. Build and register the MCP server

```bash
cd mcp
npm install
npx tsc
```

Register with Claude Code:

```bash
claude mcp add claude-memory --scope user \
  -e MEMLAYER_SERVER_URL=http://localhost:8420/api \
  -e MEMLAYER_AUTH_TOKEN=your-secret-token \
  -- node /absolute/path/to/memlayer/mcp/dist/index.js
```

Verify it's connected:

```bash
claude mcp list
# claude-memory: node .../mcp/dist/index.js - ✓ Connected
```

### 5. Add memory instructions to CLAUDE.md

Add this to your global `~/.claude/CLAUDE.md` so Claude knows to use the memory tools:

```markdown
## Memory (Cross-Session Recall)

The `claude-memory` MCP server provides tools to search past conversations:
- `search_memory` — hybrid search across all past Claude Code conversations
- `get_session_summary` — full chronological session history

Use `search_memory` when the user references past work, asks about prior
decisions, or encounters a problem that may have been solved before.
Use keyword-rich queries for best results.
```

### 6. Test it

Start a new Claude Code session and ask:

```
Do you remember what we worked on last week?
```

Claude should call `search_memory` and return relevant results from your conversation history.

## Architecture

| Component | Language | Description |
|-----------|----------|-------------|
| `daemon/` | Rust | File watcher + JSONL parser + HTTP sender with offline SQLite queue |
| `server/` | Python/FastAPI | Ingestion, async embedding worker, hybrid search API |
| `db/` | PostgreSQL 16 + pgvector | Schema, HNSW/GIN indexes, RRF stored function |
| `mcp/` | TypeScript | MCP server exposing search + summary tools via stdio |
| `skill/` | Markdown | Claude skill with memory recall trigger instructions |

## Configuration

All configuration is via environment variables:

| Variable | Used By | Default | Description |
|----------|---------|---------|-------------|
| `MEMLAYER_SERVER_URL` | daemon, mcp | `http://localhost:8420/api` | Server API URL |
| `MEMLAYER_AUTH_TOKEN` | daemon, mcp, server | *(required)* | Shared bearer token |
| `POSTGRES_PASSWORD` | docker-compose | *(required)* | Database password |
| `OPENAI_API_KEY` | server | *(optional)* | Enables vector embeddings |
| `EMBEDDING_PROVIDER` | server | `openai` | `openai` or `ollama` |
| `EMBEDDING_MODEL` | server | `text-embedding-3-small` | Embedding model name |
| `EMBEDDING_DIMENSIONS` | server | `1536` | Embedding vector size |
| `OLLAMA_BASE_URL` | server | `http://localhost:11434` | Ollama API URL |
| `MEMLAYER_WATCH_PATH` | daemon | `~/.claude/projects` | Directory to watch |
| `MEMLAYER_BATCH_SIZE` | daemon | `50` | Entries per HTTP batch |

## Search

Search uses **Reciprocal Rank Fusion (RRF)** to combine two retrieval methods:

1. **Full-Text Search** — PostgreSQL `tsvector` with `websearch_to_tsquery`, ranked by `ts_rank_cd`. Available immediately on ingestion.
2. **Vector Search** — pgvector HNSW index with cosine distance against query embedding. Available after async embedding generation.

The RRF formula merges both ranked lists:

```
score = 1/(k + fts_rank) + 1/(k + vector_rank)
```

where `k=60` (standard RRF constant). This is implemented as a PostgreSQL stored function (`hybrid_search`) for single-roundtrip execution.

**FTS-only mode**: When no `OPENAI_API_KEY` is set, search falls back to full-text search only. This still works well for keyword-based queries.

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/ingest` | Receive batch of parsed entries from daemon |
| `POST` | `/api/search` | Hybrid search (query, optional session/project filters) |
| `GET` | `/api/sessions/{id}/summary` | Chronological entries for a session |
| `GET` | `/health` | Health check |

All `/api` endpoints require `Authorization: Bearer <token>`.

## MCP Tools

### `search_memory`

Search across all past conversations.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | yes | Natural language search query |
| `session_id` | string (UUID) | no | Filter to specific session |
| `project_path` | string | no | Filter to specific project |
| `limit` | number (1-50) | no | Max results (default: 10) |

### `get_session_summary`

Get full conversation history for a session.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `session_id` | string (UUID) | yes | Session to retrieve |
| `limit` | number (1-500) | no | Max entries (default: 200) |

## Development

```bash
# Rebuild server after code changes
docker compose up server --build -d

# Run daemon tests
cd daemon && cargo test

# Watch server logs
docker compose logs -f server

# Connect to database
docker compose exec db psql -U memlayer -d memlayer

# Check entry count
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*) FROM memory_entries"
```

## Idempotency

Every conversation entry is hashed (SHA-256 of the raw JSONL line + block index) and stored with a `UNIQUE` constraint on `payload_hash`. Re-running the daemon against already-ingested files produces zero errors and zero duplicates — the database silently skips them.

## License

MIT
