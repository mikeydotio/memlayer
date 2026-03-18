# Architecture

## Components

| Component | Language | Location | Description |
|-----------|----------|----------|-------------|
| **Daemon** | Rust | `daemon/` | Tails JSONL logs, parses entries, sends them to the server. Includes an offline SQLite queue for resilience when the server is unreachable. |
| **Server** | Python (FastAPI) | `server/` | API for ingestion, search, session summaries, and file downloads. Runs async embedding and LRU eviction workers in the background. |
| **Database** | PostgreSQL 16 + pgvector | `db/` | GIN (tsvector) and HNSW (cosine) indexes. RRF hybrid search implemented as a stored function for single-roundtrip execution. |
| **CLI** | TypeScript | `cli/` | `memlayer` binary with `search`, `session`, `read-file`, and `status` commands. Called by Claude Code via the Bash tool. |

## Data Flow

```
JSONL files --> Daemon --> POST /api/ingest --> Server --> PostgreSQL
                                                              |
Claude Code <-- `memlayer` CLI <-- POST /api/search ------------+
                               <-- GET /api/sessions/{id}/summary
                               <-- GET /api/files/{file_id}
```

## Key Design Decisions

- **Idempotent ingestion.** Every entry is hashed (SHA-256 of the raw JSONL line + block index). A `UNIQUE` constraint on `payload_hash` means re-running the daemon against already-ingested files produces zero duplicates.
- **Offline resilience.** If the server is unreachable, the daemon queues entries in a local SQLite database and drains the queue when connectivity returns.
- **FTS-first.** Full-text search works immediately on ingestion without any external API keys. Vector embeddings are optional and generated asynchronously when an embedding provider is configured.
- **Automatic migrations.** The server applies database migrations on startup using a tracking table (`applied_migrations`) to ensure each migration runs exactly once.
- **Graceful shutdown.** Both the daemon (30-second queue drain timeout) and server (in-flight request draining) handle SIGTERM cleanly.
- **Large response handling.** When search or session summary responses exceed a configurable threshold, they are offloaded to persistent file storage with a structural index for efficient line-range reads.

## Search Algorithm

Search uses **Reciprocal Rank Fusion (RRF)** to combine two retrieval methods:

1. **Full-Text Search** — PostgreSQL `tsvector` with `websearch_to_tsquery`, ranked by `ts_rank_cd`. Available immediately on ingestion.
2. **Vector Search** — pgvector HNSW index with cosine distance against query embedding. Available after async embedding generation.

The RRF formula merges both ranked lists:

```
score = 1/(k + fts_rank) + 1/(k + vector_rank)
```

where `k=60` (standard RRF constant). This is implemented as a PostgreSQL stored function (`hybrid_search`) for single-roundtrip execution.

**FTS-only mode:** When no `OPENAI_API_KEY` is set, search uses full-text search only. This still works well for keyword-based queries. You can add an embedding provider later without re-ingesting data — the server backfills embeddings for existing entries automatically.
