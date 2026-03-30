# Memlayer

> Persistent, searchable memory with knowledge graph for Claude Code conversations.

Memlayer gives Claude Code perfect recall across sessions. Every conversation is automatically indexed and searchable — Claude can remember what you worked on, how you solved problems, and what decisions were made. A knowledge graph layer extracts entities and typed relationships from your conversations, connecting ideas across sessions and surfacing patterns you'd otherwise miss.

Start a Claude Code session and ask *"Do you remember how we fixed the SSH tunnel issue last week?"* — Claude searches your indexed conversation history and returns the exact session where you debugged it together. Ask *"What decisions have we superseded?"* — Claude browses the knowledge graph to find decisions that were later replaced.

## How It Works

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
   |  Code       |    via `memlayer` CLI
   |  session    |    (Rust, Bash tool)
   +-------------+
                       4. Claude recalls
```

1. **Watch** — The daemon tails Claude Code's JSONL logs in real time.
2. **Parse** — Each line is parsed into structured entries and batched to the server over HTTP.
3. **Store & Index** — The server stores entries in PostgreSQL with full-text search vectors. Embeddings are generated asynchronously for semantic search. Search uses Reciprocal Rank Fusion (RRF) to combine both methods. Optionally, an LLM extraction pipeline identifies entities (concepts, decisions, bugs, patterns, tools) and typed relationships (supports, contradicts, supersedes, depends_on) to build a knowledge graph.
4. **Recall** — Claude calls the `memlayer` CLI to query the server. `memlayer search` finds entries via hybrid search; `--expand-graph` surfaces connected entries through the knowledge graph. `memlayer entities` and `memlayer entity` browse extracted concepts and their relationships.

## Quick Start: Self-Hosted

Two commands. The server sets up the database and API; the client installs the daemon and connects Claude Code.

**Server** (needs Docker):

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
./setup_server.sh
```

**Client** (on each machine, or the same one):

```bash
./setup_client.sh
```

## Quick Start: Cloud-Hosted

Deploy to Supabase (database) + Fly.io (server) — no Docker or VPS needed:

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
./setup_cloud_hosted.sh
```

## One-Liner Install

Client-only — downloads everything automatically:

```bash
curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash
```

**Test it.** Start a new Claude Code session and ask:

```
Do you remember what we worked on last week?
```

## Documentation

### Installation & Deployment

- [Getting Started (Self-Hosted)](docs/getting-started.md) — Full server + client setup
- [Cloud Setup (Supabase + Fly.io)](docs/cloud-setup.md) — Fully cloud-hosted deployment
- [Generic VPS Setup](docs/vps-setup.md) — Deploy on any Linux VPS with Docker

### Reference

- [Architecture](docs/architecture.md) — Components, data flow, search algorithm
- [Configuration](docs/configuration.md) — All environment variables
- [CLI Reference](docs/cli-reference.md) — `memlayer-server` admin commands
- [CLI Tools & Search](docs/cli-tools.md) — `memlayer` search CLI and search filters

### Operations

- [Server Migration](docs/migration.md) — Zero-downtime server migration
- [Upgrading](docs/upgrading.md) — Version upgrade guides
- [Troubleshooting](docs/troubleshooting.md) — Common issues and fixes
- [Uninstalling](docs/uninstalling.md) — Clean removal

### Contributing

- [Development](docs/development.md) — Dev setup, project structure, API endpoints

## License

MIT
