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

## v0.3.0 — Large Response Support

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

## v0.3.1 — Embeddings & Semantic Search

- [ ] AI provider selection and configuration flow (OpenAI, Anthropic, Gemini)
- [ ] Backfill utility for embedding existing entries
- [ ] Verified hybrid (FTS + vector) search end-to-end
- [ ] Enhanced health check with component status (db, embeddings)

## v0.4.0 — Search & Network

- [ ] Tailscale-bound server (listen on 100.x.y.z only)
- [ ] Date range filters
- [ ] Message type filters (user / assistant / tool_use / tool_result)

## v0.5.0 — Reliability

- [ ] Graceful shutdown with queue flush
- [ ] Daemon heartbeat reporting
- [ ] Ingestion metrics (entries/sec, queue depth, error rate)
- [ ] Log rotation and retention policy

## v1.0.0 — Stable Release

- [ ] Comprehensive test suite (daemon, server, MCP)
- [ ] Comprehensive README with troubleshooting guide

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
