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

## v0.2.0 — Installer & Setup Scripts (shipped)

- [x] `setup_server.sh` — credential generation, Docker stack bootstrap
- [x] `setup_client.sh` — daemon install, service setup, MCP registration, CLAUDE.md injection
- [x] `install.sh` — one-liner curl installer for client-only setup
- [x] `uninstall.sh` — component-by-component teardown with confirmation prompts
- [x] Upgrade-safe re-run for all setup scripts

## v0.3.0 — Embeddings & Semantic Search

- [ ] OpenAI API key configuration flow
- [ ] Backfill embeddings for existing entries
- [ ] Verified hybrid (FTS + vector) search end-to-end
- [ ] Ollama local embedding support tested and documented

## v0.4.0 — Multi-Machine Deployment

- [ ] Systemd user service for daemon
- [ ] Tailscale-bound server (listen on 100.x.y.z only)
- [ ] Multi-client support (machine ID tracking)
- [ ] Remote server URL configuration in client setup

## v0.5.0 — Search Quality & Filtering

- [ ] Project-scoped search filters
- [ ] Date range filters
- [ ] Message type filters (user / assistant / tool_use / tool_result)
- [ ] Result ranking tuning (RRF k-constant, weight adjustments)
- [ ] Search result snippets with highlighted matches

## v0.6.0 — Observability & Reliability

- [ ] Health check endpoint with component status (db, embeddings)
- [ ] Ingestion metrics (entries/sec, queue depth, error rate)
- [ ] Daemon heartbeat reporting
- [ ] Log rotation and retention policy
- [ ] Graceful shutdown with queue flush

## v1.0.0 — Stable Release

- [ ] Comprehensive test suite (daemon, server, MCP)
- [ ] CI pipeline (build, test, release binaries)
- [ ] Pre-built daemon binaries (Linux x86_64, aarch64, macOS)
- [ ] Versioned database migrations
- [ ] Published npm package for MCP server
- [ ] Full documentation site
