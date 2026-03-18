# Development

## Common Commands

```bash
# Start the full stack (database + server)
docker compose up -d

# Rebuild the server after code changes
docker compose up server --build -d

# Build daemon from source
cd daemon && cargo build --release

# Run daemon tests
cd daemon && cargo test

# Build CLI
cd cli && npm install && npx tsc

# Watch server logs
docker compose logs -f server

# Connect to the database
docker compose exec db psql -U memlayer -d memlayer

# Check entry count
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*) FROM memory_entries"

# Check embedding coverage
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*) AS total, COUNT(embedding) AS embedded FROM memory_entries"
```

## Project Structure

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
  cli/                   TypeScript CLI binary
    src/
      cli.ts             CLI entrypoint (search, session, read-file, status)
      cli-formatters.ts  Output formatting (JSON and text modes)
      api-client.ts      HTTP client for the memlayer API
      file-cache.ts      Local file cache for large response files
      format.ts          Large response notice formatter
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

## API Endpoints

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

All `/api` endpoints require `Authorization: Bearer <token>`. Migration endpoints accept either the admin token or a time-limited migration key.
