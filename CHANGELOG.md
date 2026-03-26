# Changelog

All notable changes to this project will be documented in this file.

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
