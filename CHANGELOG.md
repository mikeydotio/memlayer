# Changelog

All notable changes to this project will be documented in this file.

## [v2.0.0] — 2026-03-30

### Added

- Knowledge graph schema, LLM-based entity extraction pipeline, and multi-stage entity resolution (`db/migrations/011_knowledge_graph.sql`, `server/src/extraction/`) (2581faf)
- Graph API: entity CRUD, typed relationships, N-hop traversal, graph-augmented search (`server/src/routes/graph.py`, `db/migrations/012_graph_search_function.sql`) (0bccb05)
- Knowledge graph CLI commands: `memlayer entities`, `memlayer entity`, `memlayer graph stats` (`cli-rs/src/commands/`) (0013238)
- TUI Graph tab: entity browser with type-colored list, detail view with relationships, aliases, and mentions (0013238)
- Agent-directed graph tools and memory skill integration for entity/relationship management (65a116d)
- **TUI Graph tab: entity search/filter bar** with debounced fuzzy search via `/` key
- **TUI Graph tab: breadcrumb neighbor traversal** — Enter drills into entities, Esc navigates back through the chain
- **TUI Graph tab: pagination** — n/p keys to page through entities when list exceeds 50
- **TUI Graph tab: top entities** display in graph overview from stats data
- **TUI Search tab: graph-augmented search toggle** — on by default with `g` key toggle and visual ON/OFF indicator
- **TUI Search tab: related entities** shown in result detail pane and graph boost scores in result list
- **TUI Stats tab: contributors section** — lists source machines with entry counts, session counts, and last active timestamps
- **TUI Stats tab: database size** metric from PostgreSQL
- Server stats endpoint: contributor aggregation by `client_machine_id` and `pg_database_size()` query

### Fixed

- Detect pg_trgm extension schema for Supabase compatibility (ac610b4)

### Changed

- Graph tab rewritten with three input modes (filter, entity list, breadcrumb drill-down) and left/right split layout
- Search requests include `expand_graph` and `graph_weight` parameters when graph mode is enabled
- Stats response extended with `contributors` and `database_size_bytes` fields (backward compatible via serde defaults) _[manual]_

## [v1.7.0] — 2026-03-29

### Added

- `memlayer recent` command — list recent sessions by last activity without a search query; supports `--limit`, `--project`, `--format` flags (`cli-rs/src/commands/recent.rs`, `cli-rs/src/format/recent.rs`) (#37)
- `--all-types` flag on `search` and `session` commands — includes all content types (tool_use, tool_result) when explicitly requested (#39)
- `--full` flag on `search` command — returns full untruncated content from the server (#38)
- Server-side content truncation for search results — `raw_content` truncated to 200 chars by default with `content_truncated` and `content_length` metadata fields (#38)
- Search strategy guidance and common mistakes section in memory skill documentation (#40)

### Changed

- `search` and `session` commands now default to `--types user,assistant`, filtering out tool_use/tool_result entries; use `--all-types` to restore previous behavior (#39)
- Search results JSON output now includes `content_truncated` and `content_length` fields (#38)
- Search results text output shows `[...truncated, N chars total]` indicator for truncated content (#38)
- Memory skill SKILL.md rewritten with expanded command reference, search heuristics, and temporal cue handling (#40)
- `memlayer.claudemd.template` updated with `memlayer recent` command and default behavior notes (#40) _[manual]_

## [v1.6.0] — 2026-03-29

### Added

- Rust CLI binary (`memlayer`) replacing the TypeScript CLI — all existing commands preserved (`search`, `session`, `read-file`, `status`) (`cli-rs/`)
- Interactive TUI dashboard (`memlayer dashboard`) with tab-based layout: Browse, Search, Live, Stats (`cli-rs/src/tui/`)
- Browse tab: Projects → Sessions → Entries drill-down hierarchy
- Search tab: live-updating search with debounce, results + detail split pane
- Live tab: real-time SSE stream of ingested entries with filter bar and auto-scroll
- Stats tab: entry/session/project counts, embedding progress, activity sparklines
- Shared Rust crate (`memlayer-common/`) for config, API types, HTTP client, and file cache
- Cargo workspace unifying `daemon`, `cli-rs`, and `memlayer-common`
- Server SSE endpoint (`GET /api/stream/entries`) for live entry broadcasting
- Server browse endpoints (`GET /api/projects`, `GET /api/sessions`, `GET /api/sessions/{id}/entries`) with cursor-based pagination
- Server stats endpoint (`GET /api/stats`) with 30s TTL cache
- In-process EventBus (`server/src/event_bus.py`) for pub/sub SSE broadcasting
- Ingest endpoint now broadcasts new entries to connected SSE clients
- Database migration `010_browse_index.sql` — composite index for cursor-based pagination

### Changed

- Daemon now builds as a workspace member (shared dependency versions)
- `setup_client.sh` builds Rust CLI instead of TypeScript CLI; Node.js no longer required
- `CLAUDE.md` updated to reflect new architecture and CLI commands _[manual]_

## [v1.5.4] — 2026-03-26

### Fixed

- Retry health check with longer timeout for Fly.io cold starts (`8e4faf7`) _[manual]_

### Changed

- Fall back to crontab `@reboot` when systemctl is unavailable (`8e4faf7`)

## [v1.5.3] — 2026-03-26

### Fixed

- Guard all systemctl calls behind availability check for containers and minimal installs (`a9d5211`) _[manual]_

## [v1.5.2] — 2026-03-26

### Changed

- All server setup scripts emit a single pre-filled client install one-liner (`d5282b9`) _[manual]_
- Client setup summary now reports warnings with re-run command and config file paths (`d5282b9`)

### Fixed

- Fix box borders and post-setup guidance (`1df7012`)
- Fix client setup summary box alignment (`1a45f2d`)
- Fix infinite loop when setup_client.sh is run via pipe (`a95f582`)
- Fix fly.toml http_service.checks array syntax (`39e2893`)

### Added

- Cloud-hosted setup wizard for Supabase + Fly.io (`8d74073`, `f90d697`)
- Cloud setup documentation and website page (`17e32c8`, `cfbb43e`)
- Auto-detect vector extension schema for Supabase compatibility (`04f7486`)
- Clear error message when pgvector extension is missing (`4e1e595`)
- Make cloud setup idempotent with billing warning and error recovery (`c02e497`)

## [v1.5.1] — 2026-03-26

### Changed

- Rename claude-mem-* to memlayer-* across the repo (`410c438`) _[manual]_

## [v1.5.0] — 2026-03-26

### Changed

- Replace MCP server with CLI binary (`659e8b8`) _[manual]_

### Fixed

- Fix CI: use uv sync instead of uv pip install --system (`e7a7c63`)

### Added

- Comprehensive E2E edge case tests (`9e12297`)
- CI pipeline and migration bug fixes (`ee7b500`)

## [v1.4.0] — 2026-03-09

### Added

- Server migration and handoff between instances (`548ca79`) _[manual]_

## [v1.3.0] — 2026-03-09

### Added

- Public website and launch at memlayer.io (`032541b`, `27e6cc3`) _[manual]_

## [v1.2.0] — 2026-03-09

### Added

- Server web UI dashboard (`ce02248`) _[manual]_
- DigitalOcean guided cloud setup script (`796c8f5`)
- Generic VPS setup documentation (`3a88102`)

## [v1.1.0] — 2026-03-09

### Added

- Batch INSERT for improved ingest performance (`28331cb`) _[manual]_
- Embedding progress tracking (`28331cb`)
- Line-range endpoint (`28331cb`)
- Ollama integration test with Docker Compose (`91e45ba`)

## [v1.0.1] — 2026-03-09

### Added

- E2E integration test with isolated Docker Compose stack (`9b3a3e3`) _[manual]_
- MCP unit tests: file-cache and api-client (`431c241`)
- Server-side response size analytics (`6843b17`)
- MCP file cache eviction: 50MB soft / 100MB hard, FIFO (`d470d6a`)
- 200KB response budget with file-based overflow (`4ed88d3`)
- Non-interactive container boot script (`993e03a`)

### Fixed

- Debounce map memory leak: prune stale entries every 60s (`4ed88d3`)

## [v1.0.0] — 2026-03-09

### Changed

- Stable release (`a91a4b9`) _[manual]_

### Fixed

- pgvector data format: pass numpy arrays instead of raw bytes (`ffd1032`)

## [v0.5.0] — 2026-03-09

### Changed

- Reliability and observability improvements (`96e61d2`) _[manual]_

## [v0.4.0] — 2026-03-09

### Added

- Search filters and network binding configuration (`65c8d08`) _[manual]_

## [v0.3.1] — 2026-03-09

### Added

- Embeddings and hardening (`c6afa88`) _[manual]_

## [v0.3.0] — 2026-03-08

### Added

- Large response support with file storage and intelligent indexing (`f0c0fd1`) _[manual]_

## [v0.2.0] — 2026-03-08

### Added

- Installer scripts and CI workflow (`20c0386`) _[manual]_

## [v0.1.0] — 2026-03-08

### Added

- Initial release: Claude Code Memory Layer (`58019c4`) _[manual]_
