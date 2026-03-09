# Memlayer Roadmap

## v0.1.0 — Initial Release (shipped)

- [x] PostgreSQL 16 + pgvector schema with RRF hybrid search function
- [x] GIN (tsvector) + HNSW (cosine) + B-tree indexes
- [x] FastAPI server with ingest, search, and session summary endpoints
- [x] Async embedding worker (OpenAI / Ollama providers)
- [x] FTS-only fallback when no embedding API key is set
- [x] Rust daemon: JSONL tailing, parsing, HTTP batching
- [x] SQLite offline queue with drain-on-reconnect
- [x] Byte-offset cursor tracking for idempotent re-runs
- [x] SHA-256 payload_hash deduplication (UNIQUE constraint)
- [x] Bearer token auth on all API endpoints
- [x] TypeScript MCP server (`search_memory`, `get_session_summary`)
- [x] Claude skill for memory recall triggers
- [x] Docker Compose stack (db + server)

## v0.2.0 — Installer, Setup & Multi-Machine Support (shipped)

- [x] `setup_server.sh` — credential generation, Docker stack bootstrap
- [x] `setup_client.sh` — daemon install, service setup, MCP registration, CLAUDE.md injection
- [x] `install.sh` — one-liner curl installer for client-only setup
- [x] `uninstall.sh` — component-by-component teardown with confirmation prompts
- [x] Upgrade-safe re-run for all setup scripts
- [x] CI pipeline (build, test, release binaries)
- [x] Pre-built daemon binaries (Linux x86_64, aarch64, macOS)
- [x] Systemd / launchd service templates for daemon
- [x] Multi-client support (machine ID tracking)
- [x] Remote server URL configuration in client setup
- [x] Project-scoped search filters

## v0.3.0 — Large Response Support (shipped)

- [x] `response_files` table with LRU indexes and soft-delete tombstones
- [x] File storage service with write/read/eviction operations
- [x] Background LRU eviction worker (configurable soft/hard limits)
- [x] Content-type detection (markdown, code, JSON, text)
- [x] Heuristic structural indexing (heading trees, function signatures, key lists)
- [x] LLM-based indexing with provider abstraction (OpenAI, Anthropic, Ollama)
- [x] Configurable index modes: off, hybrid, llm-only
- [x] `LargeResponseRef` envelope on search and session summary responses
- [x] File download endpoint (`GET /api/files/{file_id}`)
- [x] MCP `read_memory_file` tool for line-range reads from cached files
- [x] Local file cache in MCP client (`~/.claude/memlayer/cache/`)
- [x] Raised inline truncation limits to 50,000 chars (fallback)
- [x] Docker volume for persistent response file storage
- [x] Spec housekeeping: PRD rename, 0.1.0 plan recreation, roadmap update

## v0.3.1 — Embeddings & Hardening (next)

### Embeddings
- [ ] OpenAI embedding configuration and provider setup
- [ ] Background backfill on server startup (entries with NULL embeddings)
- [ ] Idempotent backfill — resumes after restart, skips already-embedded entries
- [ ] Provider/model metadata columns on entries table
- [ ] Verified hybrid (FTS + vector) search end-to-end

### Hardening (audit fixes)
- [ ] Daemon: only advance cursor after confirmed send or durable queue
- [ ] Daemon: differentiate retriable (5xx) vs non-retriable (4xx) errors
- [ ] Server: migration tracking system (applied_migrations table)
- [ ] Server: upper bounds on `limit` parameters
- [ ] Server: await cancelled tasks before pool close on shutdown
- [ ] Server: add numpy as explicit dependency
- [ ] Server: fix JSON content detection (don't truncate before parse)
- [ ] Server: timing-safe auth token comparison
- [ ] MCP: HTTP request timeouts (abort after 30s)
- [ ] MCP: document `read_memory_file` in skill and README
- [ ] Docker: health check on server container
- [ ] Scripts: mask auth token in setup_server.sh summary
- [ ] Scripts: use EnvironmentFile= instead of Environment= for secrets

### Health & Diagnostics
- [ ] Enhanced `/health` endpoint with component status (db, embeddings, queue)

## v0.4.0 — Search Filters & Network Binding

- [ ] `MEMLAYER_BIND_ADDR` env var for Docker port binding (Tailscale support)
- [ ] Setup flow detects Tailscale and offers to bind to 100.x.y.z
- [ ] Date range filters on search (`after`, `before` parameters)
- [ ] Message type filters (`types` parameter: user/assistant/tool_use/tool_result)
- [ ] MCP tool schema updated with new filter parameters

## v0.5.0 — Reliability & Observability

- [ ] Daemon: graceful shutdown with SIGTERM handler and queue flush
- [ ] Daemon: configurable shutdown timeout (default 30s)
- [ ] Server: graceful shutdown — drain in-flight requests, await workers
- [ ] Ingestion metrics in server logs (entries/sec, queue depth, error rate)
- [ ] Structured JSON logging option for production
- [ ] Log level configuration via env var

## v1.0.0 — Stable Release

### Testing
- [ ] Daemon unit tests (cursor, queue, sender, parser)
- [ ] Server unit tests (routes, embeddings, file storage, indexing)
- [ ] MCP unit tests (api-client, file-cache)
- [ ] Integration tests (server + real PostgreSQL)
- [ ] End-to-end tests (daemon → server → search round-trip)

### Data Safety
- [ ] Backup command (`memlayer backup` → pg_dump + response files archive)
- [ ] Restore command (`memlayer restore` → pg_restore + file extraction)
- [ ] Conversation purge (`memlayer forget --session <id>` / `--project <path>`)
- [ ] Data integrity check (`memlayer verify` — orphaned embeddings, missing files)

### Operations
- [ ] Automatic schema migrations with tracking (ordered, idempotent, never re-run)
- [ ] Version compatibility — daemon sends version header, server rejects incompatible
- [ ] Consolidated `memlayer` CLI (setup, verify, backup, restore, forget, status)

### Documentation
- [ ] README rewrite with table of contents and deep links
- [ ] Troubleshooting guide (common issues, diagnostic steps)
- [ ] Upgrade guide (v0.x → v1.0 migration checklist)
- [ ] Target audience: experienced devs and vibe coders alike

## v1.1.0 — Cloud Deployment Option

- [ ] Supabase compatibility (PgBouncer / prepared statement handling, migration applicator)
- [ ] AWS deployment via ECS/Fargate
- [ ] `setup_server_cloud.sh` — guided cloud setup flow
- [ ] IaC templates (CDK or Terraform) for one-click provisioning
- [ ] Documentation for bring-your-own Supabase + AWS account

## v1.2.0 — Multi-Tenancy & Auth

- [ ] User accounts with GitHub OAuth
- [ ] Per-user API key generation and management
- [ ] Tenant ID on all tables with row-level security
- [ ] Rate limiting and usage metering
- [ ] Data export and deletion endpoints (GDPR)
- [ ] Background job queue replacing in-process embedding worker

## v1.3.0 — Public Launch

- [ ] Public website (landing page, docs, pricing)
- [ ] Stripe billing integration with subscription tiers
- [ ] Web dashboard (signup, API key management, usage stats)
- [ ] One-liner install with embedded user credentials
- [ ] Privacy policy and terms of service
- [ ] Status page

## Triage / Need Scoping

- [ ] Ollama local embedding support tested and documented
- [ ] HTTP Range support for file downloads
