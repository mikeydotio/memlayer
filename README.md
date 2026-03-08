# memlayer

Persistent, searchable memory layer for Claude Code CLI sessions. Captures all conversations across projects and makes them retrievable via hybrid semantic + full-text search through MCP tools.

## Components

| Component | Language | Description |
|-----------|----------|-------------|
| `daemon/` | Rust | Tails JSONL logs, sends to server with offline queueing |
| `server/` | Python/FastAPI | Ingests entries, generates embeddings, serves search API |
| `db/` | PostgreSQL 16 + pgvector | HNSW vectors + GIN full-text + RRF hybrid search |
| `mcp/` | TypeScript | MCP server with `search_memory` and `get_session_summary` |
| `skill/` | Markdown | Claude skill for triggering memory recall |

## Quick Start

```bash
# 1. Configure
cp .env.example .env
# Edit .env with your POSTGRES_PASSWORD, MEMLAYER_AUTH_TOKEN, and optionally OPENAI_API_KEY

# 2. Start database + server
docker compose up -d

# 3. Build and run the daemon
cd daemon && cargo build --release
MEMLAYER_SERVER_URL="http://localhost:8420/api" \
MEMLAYER_AUTH_TOKEN="your-token" \
./target/release/claude-mem-daemon

# 4. Build and register MCP server
cd mcp && npm install && npx tsc
# Add to ~/.claude/settings.json mcpServers
```

## Search

Hybrid search combines PostgreSQL full-text search (BM25 ranking) with pgvector cosine similarity using Reciprocal Rank Fusion (RRF). Works in FTS-only mode if no embedding API key is configured.
