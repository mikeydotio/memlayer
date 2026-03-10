# Memlayer

> Persistent, searchable memory for Claude Code conversations.

Memlayer gives Claude Code perfect recall across sessions. Every conversation is automatically indexed and searchable -- so Claude can remember what you worked on, how you solved problems, and what decisions were made.

Start a Claude Code session and ask *"Do you remember how we fixed the SSH tunnel issue last week?"* -- Claude searches your indexed conversation history and returns the exact session where you debugged it together.

## Table of Contents

- [How It Works](#how-it-works)
- [Quick Start](#quick-start)
- [Installation](#installation)
  - [Server Setup](#server-setup)
  - [Client Setup](#client-setup)
- [Architecture](#architecture)
- [Configuration](#configuration)
- [CLI Reference](#cli-reference)
  - [memlayer status](#memlayer-status)
  - [memlayer backup](#memlayer-backup)
  - [memlayer restore](#memlayer-restore)
  - [memlayer forget](#memlayer-forget)
  - [memlayer verify](#memlayer-verify)
  - [memlayer migrate](#memlayer-migrate)
- [MCP Tools](#mcp-tools)
  - [search_memory](#search_memory)
  - [get_session_summary](#get_session_summary)
  - [read_memory_file](#read_memory_file)
- [Search Filters](#search-filters)
- [Upgrading](#upgrading)
- [Troubleshooting](#troubleshooting)
- [Development](#development)
- [License](#license)

## How It Works

Memlayer runs four components that work together to capture, store, and retrieve your Claude Code conversations:

```
  Your Claude Code sessions             Memlayer server
  ~/.claude/projects/**/*.jsonl          (FastAPI + PostgreSQL)

         |                                      |
         v                                      v
   +-------------+      HTTP/JSON      +----------------------+
   |   daemon    | ------------------> |   PostgreSQL 16      |
   |   (Rust)    |    batch ingest     |   + pgvector         |
   |             |                     |                      |
   |  1. Watches |                     |  3. Stores entries   |
   |  2. Parses  |                     |     FTS + vectors    |
   +-------------+                     +----------+-----------+
                                                  |
   +-------------+     search API                 |
   |  Claude     | <------------------------------+
   |  Code       |    via MCP server
   |  session    |    (TypeScript, stdio)
   +-------------+
                       4. Claude recalls
```

**Step 1 -- Watch.** The daemon watches Claude Code's JSONL conversation log files in real time. Every message you send, every response Claude gives, every tool call and result is detected as it happens.

**Step 2 -- Parse.** Each JSONL line is parsed into structured entries: user messages, assistant responses, tool calls, and tool results. Entries are batched and sent to the server over HTTP.

**Step 3 -- Store.** The server stores entries in PostgreSQL with full-text search vectors generated on insert. Optionally, embeddings are generated asynchronously for semantic search. Search uses Reciprocal Rank Fusion (RRF) to combine both retrieval methods.

**Step 4 -- Recall.** When Claude needs to remember something, it calls the MCP tools (`search_memory`, `get_session_summary`) which query the server and return relevant conversation history directly into the current session.

## Quick Start

Two commands get you running. The server command sets up the database and API; the client command installs the daemon and connects Claude Code to your memory.

**On the server machine** (needs Docker):

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
./setup_server.sh
```

The script generates credentials, starts Docker containers, and prints an auth token. Save this token -- you will need it for client setup.

**On each client machine** (or the same machine):

```bash
./setup_client.sh
```

The script installs the daemon binary, sets up a background service (systemd on Linux, launchd on macOS), registers MCP tools with Claude Code, and adds memory instructions to your `CLAUDE.md`.

**One-liner install** (client only -- downloads everything automatically):

```bash
curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash
```

**Test it.** Start a new Claude Code session and ask:

```
Do you remember what we worked on last week?
```

Claude should call `search_memory` and return results from your conversation history.

## Installation

### Prerequisites

| Component | Requirements |
|-----------|-------------|
| Server | Docker and Docker Compose v2 |
| Client | Node.js 18+, npm |
| Optional | Rust toolchain (for building daemon from source) |
| Optional | Claude CLI (`npm install -g @anthropic-ai/claude-code`) |

### Server Setup

The server runs as two Docker containers: PostgreSQL 16 with pgvector for storage, and a FastAPI application for the API.

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
./setup_server.sh
```

The setup script walks you through five steps:

1. **Prerequisites** -- Checks that Docker and Docker Compose v2 are installed.
2. **Configuration** -- Generates a database password and auth token (or lets you provide your own). Optionally configures OpenAI for vector embeddings.
3. **Docker startup** -- Runs `docker compose up -d` to start the database and server containers.
4. **Health check** -- Waits up to 60 seconds for the server to respond at `http://localhost:8420/health`.
5. **Summary** -- Displays the server URL and auth token.

If you have Tailscale installed, the script will detect it and offer to bind the server to your Tailscale IP for secure remote access.

**Re-running is safe.** The script detects existing installations and asks whether to keep or regenerate your configuration.

<details>
<summary>Manual server setup (without the script)</summary>

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
cp .env.example .env
```

Edit `.env` with your credentials:

```bash
# Required
POSTGRES_PASSWORD=your-secure-database-password
MEMLAYER_AUTH_TOKEN=your-secret-auth-token

# Optional -- enables vector embeddings for semantic search
# Without this, search uses full-text search only (still works well)
OPENAI_API_KEY=sk-...
```

Start the containers:

```bash
docker compose up -d
```

Verify the server is running:

```bash
curl http://localhost:8420/health
# {"status":"ok","components":{"database":"ok","embeddings":"disabled (FTS-only)"}}
```

</details>

### Client Setup

The client consists of a Rust daemon (pre-built binaries available) and a TypeScript MCP server that connects to Claude Code.

```bash
./setup_client.sh
```

The setup script walks you through seven steps:

1. **Prerequisites** -- Checks for Node.js 18+ and optionally Rust/Cargo.
2. **Daemon installation** -- Downloads a pre-built binary or builds from source. Installs to `~/.local/bin/claude-mem-daemon`.
3. **Server connection** -- Auto-detects a local server or prompts for the server URL.
4. **Authentication** -- Prompts for the auth token from server setup (or detects an existing one).
5. **Background service** -- Installs a systemd service (Linux) or launchd agent (macOS) so the daemon starts automatically.
6. **MCP tools** -- Builds the TypeScript MCP server and registers it with Claude Code via `claude mcp add`.
7. **CLAUDE.md** -- Adds memory instructions to `~/.claude/CLAUDE.md` so Claude knows when and how to use the memory tools.

You can also pass arguments directly to skip the interactive prompts:

```bash
./setup_client.sh --server-url http://your-server:8420/api --auth-token your-token
```

<details>
<summary>Manual client setup (without the script)</summary>

**Build the daemon:**

```bash
cd daemon
cargo build --release
cp target/release/claude-mem-daemon ~/.local/bin/
```

**Run the daemon:**

```bash
MEMLAYER_SERVER_URL="http://localhost:8420/api" \
MEMLAYER_AUTH_TOKEN="your-secret-token" \
~/.local/bin/claude-mem-daemon
```

On first run, the daemon scans all existing JSONL files and ingests them. Subsequent runs pick up from where they left off using byte-offset cursors stored in `~/.local/share/memlayer/cursors.db`.

**Set up the daemon as a background service (Linux with systemd):**

```bash
# Create env file for secrets
mkdir -p ~/.config/memlayer
cat > ~/.config/memlayer/env << 'EOF'
MEMLAYER_AUTH_TOKEN=your-secret-token
MEMLAYER_SERVER_URL=http://localhost:8420/api
EOF
chmod 600 ~/.config/memlayer/env

# Create service file
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/claude-mem-daemon.service << 'EOF'
[Unit]
Description=Claude Memory Layer Daemon
After=network-online.target

[Service]
Type=simple
ExecStart=%h/.local/bin/claude-mem-daemon
EnvironmentFile=%h/.config/memlayer/env
Environment=RUST_LOG=info
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now claude-mem-daemon
```

**Build and register the MCP server:**

```bash
cd mcp
npm install
npx tsc

claude mcp add claude-memory --scope user \
  -e MEMLAYER_SERVER_URL=http://localhost:8420/api \
  -e MEMLAYER_AUTH_TOKEN=your-secret-token \
  -- node /absolute/path/to/memlayer/mcp/dist/index.js
```

Verify the MCP server is connected:

```bash
claude mcp list
# claude-memory: node .../mcp/dist/index.js
```

**Add memory instructions to CLAUDE.md:**

Add this to your global `~/.claude/CLAUDE.md`:

```markdown
## Memory (Cross-Session Recall)

The `claude-memory` MCP server provides tools to search past conversations:
- `search_memory` -- hybrid search across all past Claude Code conversations
- `get_session_summary` -- full chronological session history

Use `search_memory` when the user references past work, asks about prior
decisions, or encounters a problem that may have been solved before.
Use keyword-rich queries for best results.
```

</details>

## Architecture

Memlayer is composed of four independently deployable components:

| Component | Language | Location | Description |
|-----------|----------|----------|-------------|
| **Daemon** | Rust | `daemon/` | File watcher that tails JSONL logs, parses entries, and sends them to the server. Includes an offline SQLite queue for resilience when the server is unreachable. |
| **Server** | Python (FastAPI) | `server/` | API for ingestion, search, session summaries, and file downloads. Runs async embedding and LRU eviction workers in the background. |
| **Database** | PostgreSQL 16 + pgvector | `db/` | Schema with GIN (tsvector) and HNSW (cosine) indexes. RRF hybrid search implemented as a stored function for single-roundtrip execution. |
| **MCP Server** | TypeScript | `mcp/` | Exposes `search_memory`, `get_session_summary`, and `read_memory_file` tools to Claude Code via the Model Context Protocol (stdio transport). |

**Data flow:**

```
JSONL files --> Daemon --> POST /api/ingest --> Server --> PostgreSQL
                                                              |
Claude Code <-- MCP (stdio) <-- POST /api/search -------------+
                            <-- GET /api/sessions/{id}/summary
                            <-- GET /api/files/{file_id}
```

**Key design decisions:**

- **Idempotent ingestion.** Every entry is hashed (SHA-256 of the raw JSONL line + block index). A `UNIQUE` constraint on `payload_hash` means re-running the daemon against already-ingested files produces zero duplicates.
- **Offline resilience.** If the server is unreachable, the daemon queues entries in a local SQLite database and drains the queue when connectivity returns.
- **FTS-first.** Full-text search works immediately on ingestion without any external API keys. Vector embeddings are optional and generated asynchronously when an embedding provider is configured.
- **Automatic migrations.** The server applies database migrations on startup using a tracking table (`applied_migrations`) to ensure each migration runs exactly once.
- **Graceful shutdown.** Both the daemon (30-second queue drain timeout) and server (in-flight request draining) handle SIGTERM cleanly.
- **Large response handling.** When search or session summary responses exceed a configurable character threshold, they are offloaded to persistent file storage with a structural index for efficient line-range reads.

### Search Algorithm

Search uses **Reciprocal Rank Fusion (RRF)** to combine two retrieval methods:

1. **Full-Text Search** -- PostgreSQL `tsvector` with `websearch_to_tsquery`, ranked by `ts_rank_cd`. Available immediately on ingestion.
2. **Vector Search** -- pgvector HNSW index with cosine distance against query embedding. Available after async embedding generation.

The RRF formula merges both ranked lists:

```
score = 1/(k + fts_rank) + 1/(k + vector_rank)
```

where `k=60` (standard RRF constant). This is implemented as a PostgreSQL stored function (`hybrid_search`) for single-roundtrip execution.

**FTS-only mode:** When no `OPENAI_API_KEY` is set, search uses full-text search only. This still works well for keyword-based queries. You can add an embedding provider later without re-ingesting any data -- the server backfills embeddings for existing entries automatically.

## Configuration

All configuration is via environment variables. No config files are required.

### Server Variables

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

### Daemon Variables

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

### MCP Server Variables

Set these via `claude mcp add -e` (handled automatically by `setup_client.sh`):

| Variable | Default | Description |
|----------|---------|-------------|
| `MEMLAYER_SERVER_URL` | `http://localhost:8420/api` | Server API URL |
| `MEMLAYER_AUTH_TOKEN` | *(required)* | Shared bearer token |

## CLI Reference

The `memlayer` CLI provides commands for managing your memory database. All commands connect to the server specified by `MEMLAYER_SERVER_URL` and authenticate with `MEMLAYER_AUTH_TOKEN`.

### memlayer status

Show the current state of all memlayer components.

```bash
memlayer status
```

Example output:

```
Memlayer Status
===============
Server:       http://localhost:8420 (healthy)
Database:     ok (6,243 entries, 847 sessions)
Embeddings:   openai (text-embedding-3-small), 6,100 embedded
Daemon:       running (systemd), PID 12345
MCP:          registered (claude-memory)
File cache:   ~/.claude/memlayer/cache/ (12 files, 2.4 MB)
```

### memlayer backup

Create a backup of the entire memory database and response files.

```bash
# Backup to the default location (~/.local/share/memlayer/backups/)
memlayer backup

# Backup to a specific file
memlayer backup --output /path/to/backup.tar.gz
```

This runs `pg_dump` against the database and archives response files into a single compressed tarball. The backup includes:
- All database tables (entries, sessions, migrations)
- All response files from the Docker volume
- Metadata (timestamp, entry count, server version)

### memlayer restore

Restore a backup to the database and response file storage.

```bash
memlayer restore /path/to/backup.tar.gz
```

The restore process:
1. Verifies the backup archive integrity
2. Stops the server containers
3. Restores the database from the `pg_dump`
4. Extracts response files to the Docker volume
5. Restarts the server containers

### memlayer forget

Permanently delete conversations from the database.

```bash
# Forget a specific session
memlayer forget --session 550e8400-e29b-41d4-a716-446655440000

# Forget all conversations for a project
memlayer forget --project /home/user/my-project

# Forget everything before a date
memlayer forget --before 2025-01-01
```

The command asks for confirmation before deleting. Pass `--confirm` to skip the prompt (useful in scripts).

### memlayer verify

Check database integrity and report any issues.

```bash
memlayer verify
```

Example output:

```
Memlayer Verify
===============
Entries:              6,243 (0 orphaned)
Sessions:             847
Embeddings:           6,100 / 6,243 (97.7%)
Response files:       DB: 24, Disk: 24 (0 missing, 0 orphaned)
Payload hashes:       all unique
Migration tracking:   8 applied, 0 pending
Status:               OK
```

Checks performed:
- Orphaned entries (no matching session)
- Missing embeddings (entries with NULL embedding vectors)
- File storage consistency (database records vs. files on disk)
- Duplicate payload hashes
- Pending database migrations

### memlayer migrate

Move your memlayer instance to a new server with zero data loss. The migration transfers all conversation entries, embeddings, and response files from the source to the destination, then redirects the daemon automatically.

```bash
# On the source server — generate a migration key
memlayer migrate

# On the destination server — set up and pull data from source
./setup_server.sh --migrate
```

The migration uses a state machine (INITIATED → KEY_EXCHANGED → REDIRECTING → DRAINING → TRANSFERRING → VERIFYING → COMPLETE) with Ed25519-signed HTTP 449 redirects to hand off the daemon mid-flight. The daemon queues entries locally during the transition — no data is ever discarded.

See [docs/migration.md](docs/migration.md) for the full guide, including failure recovery and manual steps.

## MCP Tools

Memlayer exposes three tools to Claude Code via the [Model Context Protocol](https://modelcontextprotocol.io/). These are what Claude calls when it needs to recall past conversations.

### search_memory

Search across all past Claude Code conversations using hybrid semantic + full-text search. Returns relevant conversation excerpts ranked by relevance.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | yes | Natural language search query describing what you are looking for |
| `session_id` | string (UUID) | no | Filter to a specific session |
| `project_path` | string | no | Filter to a specific project (e.g., `/home/user/my-project`) |
| `limit` | number (1--50) | no | Maximum results to return (default: 10) |
| `after` | string (ISO 8601) | no | Only return entries after this timestamp (e.g., `2025-01-01T00:00:00Z`) |
| `before` | string (ISO 8601) | no | Only return entries before this timestamp |
| `types` | string array | no | Filter by entry type: `user`, `assistant`, `tool_use`, `tool_result` |

**How Claude uses it:**

```
User: "How did we fix the database connection pooling issue?"
Claude calls: search_memory(query="database connection pooling fix")
```

Results include the session ID, project path, date, content type, relevance score, and the raw conversation content for each match.

### get_session_summary

Retrieve the full chronological conversation history for a specific Claude Code session. Use after `search_memory` returns interesting results to get the complete context of that session.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `session_id` | string (UUID) | yes | The session ID to retrieve |
| `limit` | number (1--500) | no | Maximum entries to return (default: 200) |
| `types` | string array | no | Filter by entry type: `user`, `assistant`, `tool_use`, `tool_result` |

### read_memory_file

Read a specific line range from a large response that was offloaded to file storage. When `search_memory` or `get_session_summary` returns a response larger than the configured threshold, the full response is stored as a file and a structural index is returned instead. Use that index to identify which line ranges contain the content you need, then call this tool to read those ranges.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `file_id` | string (UUID) | yes | The file ID from the `large_response.file_id` field |
| `start_line` | number | yes | Start line number (1-indexed, inclusive) |
| `end_line` | number | yes | End line number (1-indexed, inclusive) |

**Typical workflow:**

1. Call `search_memory` with a query to find relevant conversations.
2. If a result looks relevant, call `get_session_summary` with its `session_id` for full context.
3. If the response includes a `large_response` reference with a structural index, use `read_memory_file` with the `file_id` and line ranges from the index to read specific sections.
4. Present findings with session dates and project context.

## Search Filters

Search supports several filters that can be combined in any combination.

### Date Filters

Restrict results to a time window using ISO 8601 timestamps:

```
# Everything after a date
search_memory(query="deployment script", after="2025-06-01T00:00:00Z")

# Everything before a date
search_memory(query="auth bug", before="2025-03-15T00:00:00Z")

# A specific date range
search_memory(query="refactor", after="2025-01-01T00:00:00Z", before="2025-02-01T00:00:00Z")
```

### Type Filters

Filter by the kind of conversation entry:

| Type | What it captures |
|------|-----------------|
| `user` | Messages you sent to Claude |
| `assistant` | Claude's text responses |
| `tool_use` | Tool calls Claude made (file reads, edits, bash commands) |
| `tool_result` | Results returned from those tool calls |

```
# Only human and assistant messages (skip tool noise)
search_memory(query="database schema", types=["user", "assistant"])

# Only tool results (find error outputs, command results)
search_memory(query="npm install error", types=["tool_result"])
```

### Project Filter

Restrict to conversations about a specific project:

```
search_memory(query="API design", project_path="/home/user/my-api")
```

The `project_path` must match exactly as it appears in the database. You can find project paths in search results.

### Session Filter

Look within a single known session:

```
search_memory(query="the fix we applied", session_id="550e8400-e29b-41d4-a716-446655440000")
```

### Combining Filters

All filters can be used together:

```
search_memory(
  query="migration fix",
  project_path="/home/user/my-api",
  after="2025-06-01T00:00:00Z",
  types=["user", "assistant"],
  limit=5
)
```

## Upgrading

### From v0.x to v1.0.0

If you have an existing memlayer installation, upgrading is straightforward. Your conversation data is preserved -- nothing is lost during the upgrade.

**Server:**

```bash
cd memlayer
git pull
./setup_server.sh
```

The setup script detects your existing `.env` and Docker containers. It will offer to rebuild with the latest code. Database migrations are applied automatically on server startup -- no manual SQL is required.

**Client (installed via `install.sh`):**

```bash
curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash
```

**Client (installed via git clone):**

```bash
cd memlayer
git pull
./setup_client.sh
```

Re-running the setup scripts is safe and idempotent. They detect existing installations and offer to update each component individually.

### What changed in v1.4.0

- **Server migration** -- Move your memlayer instance to a new host with zero downtime and zero data loss. See [docs/migration.md](docs/migration.md).
- **Ed25519-signed 449 redirects** -- The daemon automatically follows server redirects during migration, queuing entries locally to guarantee no data is lost.
- **Migration auth** -- Time-limited migration keys with SHA-256 hashing, automatic TTL extension after handshake, and stale cleanup.
- **Transfer worker** -- Background pull-based transfer of entries, embeddings, and response files from source to destination.
- **Daemon credential provisioning** -- After migration completes, the daemon automatically obtains new credentials from the destination server.

### What changed in v1.0.0

- **Automatic schema migrations** with tracking -- no manual SQL, no repeated migrations
- **Version compatibility** checks between daemon and server
- **`memlayer` CLI** with `status`, `backup`, `restore`, `verify`, and `forget` commands
- **Date range and message type filters** on search and session summary
- **Graceful shutdown** with queue flushing (daemon) and connection draining (server)
- **Structured JSON logging** option for production deployments
- **Enhanced health endpoint** with component-level status (database, embeddings)
- **Comprehensive test suite** -- unit, integration, and end-to-end tests

## Troubleshooting

### Server not starting

**Symptom:** `docker compose up -d` runs but `curl http://localhost:8420/health` fails or times out.

**Steps:**

```bash
# Check container status
docker compose ps

# Check server logs for errors
docker compose logs server

# Check if the database is healthy
docker compose logs db
```

Common causes:
- **Port 8420 already in use.** Check with `lsof -i :8420` or `ss -tlnp | grep 8420`. Stop the conflicting process or change the port.
- **Database not ready.** The server waits for the `db` container's health check, but if the database is slow to initialize on first run, the server may time out. Restart: `docker compose restart server`.
- **Missing `.env` file.** Copy `.env.example` to `.env` and fill in at least `POSTGRES_PASSWORD` and `MEMLAYER_AUTH_TOKEN`.

### Daemon not connecting to server

**Symptom:** Daemon starts but logs show repeated connection errors or timeouts.

**Steps:**

```bash
# Check daemon status (Linux)
systemctl --user status claude-mem-daemon
journalctl --user -u claude-mem-daemon -f

# Check daemon status (macOS)
launchctl list | grep memlayer
cat ~/Library/Logs/claude-mem-daemon.log
```

Common causes:
- **Wrong `MEMLAYER_SERVER_URL`.** The URL must include `/api` at the end (e.g., `http://localhost:8420/api`, not `http://localhost:8420`).
- **Server on a different machine.** Replace `localhost` with the server's actual IP address or Tailscale hostname.
- **Firewall blocking port 8420.** Ensure the port is open on the server machine.
- **Auth token mismatch.** Check `~/.config/memlayer/env` on the client and `.env` on the server -- tokens must match exactly.

The daemon retries automatically with exponential backoff (up to 5 minutes between attempts) and queues entries locally in SQLite until the server is reachable. No data is lost while the server is down.

### MCP tools not showing up in Claude Code

**Symptom:** Claude does not call `search_memory` when you reference past conversations. It may say it does not have access to memory tools.

**Steps:**

```bash
# Check MCP registration
claude mcp list

# If claude-memory is missing, re-register:
cd memlayer/mcp && npm install && npx tsc
claude mcp add claude-memory --scope user \
  -e MEMLAYER_SERVER_URL=http://localhost:8420/api \
  -e MEMLAYER_AUTH_TOKEN=your-token \
  -- node /absolute/path/to/memlayer/mcp/dist/index.js
```

Common causes:
- **MCP server not registered.** Run `setup_client.sh` again, or register manually with the commands above.
- **MCP server build failed.** Check for TypeScript errors: `cd mcp && npx tsc`.
- **Node.js not in PATH.** The MCP server requires Node.js 18+. Verify with `node --version`.
- **Missing CLAUDE.md instructions.** Claude needs to be told when to use the memory tools. Check `~/.claude/CLAUDE.md` for a "Memory (Cross-Session Recall)" section. If it is missing, re-run `setup_client.sh` or add the section manually (see the [Client Setup](#client-setup) section).

### Search returning no results

**Symptom:** `search_memory` returns "No matching memories found" for queries you know should match.

**Steps:**

```bash
# Check how many entries are in the database
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*) FROM memory_entries"

# Check the time range of ingested data
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*), MIN(created_at), MAX(created_at) FROM memory_entries"

# Check if the daemon is running
systemctl --user status claude-mem-daemon
```

Common causes:
- **Daemon is not running.** Start it: `systemctl --user start claude-mem-daemon`.
- **Initial ingestion still in progress.** On first run, the daemon processes all existing JSONL files, which can take several minutes for a large history. Check daemon logs for progress.
- **Query too specific.** Full-text search works best with keywords. Try shorter, simpler queries instead of full sentences.
- **Wrong project filter.** Remove the `project_path` filter to search across all projects.

### Embeddings not generating

**Symptom:** Search works but only uses full-text search. The `/health` endpoint shows `"embeddings": "disabled (FTS-only)"`.

**Steps:**

```bash
# Check the health endpoint
curl http://localhost:8420/health | python3 -m json.tool

# Check for embedding errors in server logs
docker compose logs server | grep -i embed
```

Common causes:
- **No `OPENAI_API_KEY` set.** Add it to your `.env` file and restart: `docker compose restart server`.
- **Invalid API key.** Test it directly: `curl https://api.openai.com/v1/models -H "Authorization: Bearer sk-..."`.
- **Using Ollama but it is not running.** If `EMBEDDING_PROVIDER=ollama`, ensure Ollama is accessible at the `OLLAMA_BASE_URL` and has an embedding model pulled.

Note: full-text search works well on its own. Embeddings add semantic understanding (finding "auth problem" when you searched for "login bug") but are not required for memlayer to be useful.

### "Invalid auth token" errors

**Symptom:** API requests return HTTP 401 "Invalid or missing auth token."

**Steps:**

```bash
# Check the token on the server
grep MEMLAYER_AUTH_TOKEN /path/to/memlayer/.env

# Check the token on the client (daemon)
cat ~/.config/memlayer/env

# Test the token manually
curl -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":1}' \
  http://localhost:8420/api/search
```

Common causes:
- **Token mismatch.** The token must be identical on both the server (`.env`) and client (`~/.config/memlayer/env`).
- **Token regenerated on server but not updated on client.** Re-run `setup_client.sh` or manually update `~/.config/memlayer/env` and restart the daemon.
- **Extra whitespace or newline in the token.** Check for trailing newlines in your env files.

### Database connection issues

**Symptom:** Server logs show "connection refused" or "could not connect to server" errors for PostgreSQL.

**Steps:**

```bash
# Check database container status
docker compose ps db

# Check database logs
docker compose logs db

# Check available disk space
df -h
```

Common causes:
- **Database container not running.** Start it: `docker compose up -d db`. Wait for the health check to pass before starting the server.
- **Database still initializing.** On first startup, PostgreSQL runs all migrations. Wait a moment and check `docker compose logs db` for "database system is ready to accept connections."
- **Disk full.** PostgreSQL refuses to start without free disk space. Free up space and restart: `docker compose restart db`.
- **Corrupted data volume.** As a last resort (WARNING: this deletes all indexed data): `docker compose down -v && docker compose up -d`.

### Backup and restore issues

**Symptom:** `memlayer backup` or `memlayer restore` fails with an error.

**Steps:**

```bash
# Ensure Docker containers are running (required for backup/restore)
docker compose ps

# Check write permissions on the backup destination
ls -la ~/.local/share/memlayer/backups/

# For restore, verify the backup file is valid
tar -tzf /path/to/backup.tar.gz | head
```

Common causes:
- **Docker containers not running.** Both backup and restore need the database container.
- **Insufficient disk space** for the backup file.
- **Backup file corrupted or truncated.** Re-create the backup from the source.

### Large response files not loading

**Symptom:** `read_memory_file` returns "File not found" errors for a file ID that was recently returned by search.

**Steps:**

```bash
# Check if the file exists on the server
docker compose exec server ls /data/response_files/

# Check eviction settings
grep FILE_STORAGE .env
```

Common causes:
- **File was evicted.** If `FILE_STORAGE_SOFT_LIMIT` or `FILE_STORAGE_HARD_LIMIT` is set, older files are deleted when storage limits are exceeded. Increase the limits or set them to `0` for unlimited storage.
- **Client file cache is stale.** Clear it: `rm -rf ~/.claude/memlayer/cache/*`.

### Slow initial ingestion

**Symptom:** On first startup, the daemon takes a long time and uses noticeable CPU.

This is expected behavior. The daemon scans all existing JSONL files in `~/.claude/projects/` on first run. If you have months of Claude Code history, initial ingestion can take several minutes. Subsequent runs are fast because the daemon tracks byte-offset cursors and only processes new data.

Monitor progress:

```bash
# Linux
journalctl --user -u claude-mem-daemon -f

# macOS
tail -f ~/Library/Logs/claude-mem-daemon.log
```

## Development

```bash
# Start the full stack (database + server)
docker compose up -d

# Rebuild the server after code changes
docker compose up server --build -d

# Build daemon from source
cd daemon && cargo build --release

# Run daemon tests
cd daemon && cargo test

# Build MCP server
cd mcp && npm install && npx tsc

# Watch server logs in real time
docker compose logs -f server

# Connect to the database directly
docker compose exec db psql -U memlayer -d memlayer

# Check entry count
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*) FROM memory_entries"

# Check embedding coverage
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*) AS total, COUNT(embedding) AS embedded FROM memory_entries"
```

### Project Structure

```
memlayer/
  daemon/                Rust daemon
    src/
      main.rs            Entry point, signal handling, graceful shutdown
      config.rs          Environment variable configuration
      parser.rs          JSONL parsing and content extraction
      watcher.rs         File watcher with byte-offset cursor tracking
      sender.rs          HTTP batch sender with retry and backoff
      migration.rs       Server migration: 449 handling, Ed25519, credential provisioning
      queue.rs           SQLite offline queue
      cursor.rs          Cursor persistence for idempotent re-runs
  server/                Python FastAPI server
    src/
      main.py            App lifecycle, auth middleware, health endpoint
      config.py          Pydantic settings from environment variables
      db.py              asyncpg connection pool management
      models.py          Pydantic request/response models
      embeddings.py      OpenAI/Ollama embedding providers and background worker
      file_storage.py    Response file storage with LRU eviction
      eviction.py        Background eviction worker
      migration_state.py Migration state machine, key management, Ed25519 signing
      routes/
        ingest.py        POST /api/ingest (with migration 449 redirect)
        search.py        POST /api/search + GET /api/sessions/{id}/summary
        files.py         GET /api/files/{file_id}
        migration.py     Migration API endpoints (initiate, transfer, receive)
      indexing/          Structural indexing for large responses
  mcp/                   TypeScript MCP server
    src/
      index.ts           Tool definitions (search_memory, get_session_summary, read_memory_file)
      api-client.ts      HTTP client for the memlayer API
      file-cache.ts      Local file cache for large response files
      format.ts          Response formatting helpers
  db/
    migrations/          SQL migrations (applied in order on server startup)
    init.sh              Docker entrypoint init script
  scripts/               Setup script templates and shared shell library
  skill/                 Claude skill definition for memory recall
  docker-compose.yml     Database + server Docker stack
  setup_server.sh        Interactive server setup
  setup_client.sh        Interactive client setup
  install.sh             One-liner curl installer (client only)
  uninstall.sh           Component-by-component uninstaller
```

### API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/health` | No | Health check with component status (database, embeddings) |
| `POST` | `/api/ingest` | Yes | Receive a batch of parsed entries from the daemon (max 200) |
| `POST` | `/api/search` | Yes | Hybrid search with optional filters (session, project, date, type) |
| `GET` | `/api/sessions/{id}/summary` | Yes | Chronological conversation entries for a session |
| `GET` | `/api/files/{file_id}` | Yes | Download a large response file |
| `POST` | `/api/migration/initiate` | Admin | Generate migration key and Ed25519 keypair |
| `GET` | `/api/migration/status` | Admin or migration key | Migration state and transfer progress |
| `POST` | `/api/migration/cancel` | Admin | Cancel an active migration |
| `POST` | `/api/migration/verify-destination` | Migration key | Validate key and negotiate embeddings |
| `POST` | `/api/migration/start-redirect` | Migration key | Begin 449 redirect of ingest requests |
| `GET` | `/api/migration/stream/config` | Migration key | Export server config for transfer |
| `GET` | `/api/migration/stream/entries` | Migration key | Paginated entry export |
| `GET` | `/api/migration/stream/files` | Migration key | Response file export |
| `POST` | `/api/migration/receive/handshake` | Admin | Accept migration on destination |
| `POST` | `/api/migration/receive/entries` | Admin | Receive migrated entries |
| `POST` | `/api/migration/receive/files` | Admin | Receive migrated files |
| `POST` | `/api/migration/receive/complete` | Admin | Verify counts and complete migration |
| `GET` | `/api/migration/client-provision` | Migration key | Provision daemon credentials post-migration |

All `/api` endpoints require `Authorization: Bearer <token>`. Migration endpoints accept either the admin token or a time-limited migration key (noted in the Auth column).

## Uninstalling

```bash
# From a repo clone
./uninstall.sh

# From an install.sh installation
~/.memlayer/uninstall.sh
```

The uninstaller walks through each component and asks before removing anything:

1. **Background service** -- Stops and removes the systemd service or launchd agent
2. **Daemon binary** -- Removes `~/.local/bin/claude-mem-daemon`
3. **MCP registration** -- Removes the `claude-memory` entry from Claude Code
4. **CLAUDE.md section** -- Removes the memory instructions from `~/.claude/CLAUDE.md`
5. **Local data** -- Removes cursor database and offline queue (`~/.local/share/memlayer/`)
6. **Server and database** -- Stops Docker containers and removes volumes (with data loss warning)
7. **Repository clone** -- Removes `~/.memlayer/` (only for `install.sh` installations)

## License

MIT
