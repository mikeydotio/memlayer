# Domain Research: Existing Infrastructure

## Signal Pattern (from git wrapper at ~/.local/bin/git)

The agentsmith host already has a git wrapper that intercepts `git push`:
- Checks remote URL and branch
- On match, touches `/var/run/coderig/signals/auto-update`
- systemd path unit triggers deploy service

Can be extended for memlayer: detect "memlayer" in remote URL, touch `/var/run/memlayer/signals/deploy`.

## Docker Compose Stack

- `docker-compose.yml`: db (pgvector:pg16) + server (built from server/Dockerfile)
- `docker-compose.test.yml`: isolated test stack (port 8421, separate volumes/network)
- `docker-compose.migration-test.yml`: dual server+DB pairs
- `.env.example` provides template; `.env` is gitignored

## Dockerfiles

- `server/Dockerfile`: context is `./server`, uses `uv pip install --system`, CMD `uvicorn`
- `deploy/Dockerfile.cloud`: context is repo root, uses `uv sync --no-cache`, CMD `uv run uvicorn`
- Both now have non-root `app` user and `UV_CACHE_DIR=/tmp/uv-cache`

## Health Endpoint

`/health` (unauthenticated) returns:
- `status`: ok/degraded
- `components.database`: ok/error
- `components.embeddings`: ok/disabled
- `embedding_progress`: total_entries, embedded, pending, queue_depth, provider, model, enabled
- `read_only`: whether schema is ahead of server

## Migration System

- `server/src/migrator.py`: transactional migrations, read-only mode, backup, dry-run, fingerprinting
- `server/src/routes/migration.py`: server-to-server data migration (state machine, Ed25519 signed redirects)
- 14 migration files in `db/migrations/`

## Existing E2E Tests

- `tests/e2e.sh`: ingest → search → session round-trip
- `tests/e2e-ollama.sh`: embedding polling pattern (3s interval, 60s timeout)
- `scripts/lib.sh`: `wait_for_url()` utility
- CI uses `docker compose -f docker-compose.test.yml up -d --build --wait` + `down -v`

## Key Config Locations

- `~/.config/memlayer/env`: daemon auth token + server URL
- `~/.config/memlayer/cloud.env`: cloud deployment config
- `.env` at repo root: docker-compose secrets (POSTGRES_PASSWORD, MEMLAYER_AUTH_TOKEN, API keys)
