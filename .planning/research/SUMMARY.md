# Research Summary

## Key Findings That Shape the Design

1. **Signal mechanism must be remote**: The container and brainframe are separate machines on Tailscale. The shared-volume pattern from deploy-web won't work. SSH + touch is simplest; the existing git wrapper at `~/.local/bin/git` can be extended to also signal memlayer deploys.

2. **Existing E2E infra is directly reusable**: `docker-compose.test.yml`, `tests/e2e.sh`, `wait_for_url()`, and the embedding polling pattern from `e2e-ollama.sh` can be adapted for the smoke test with minimal changes.

3. **Migrator handles blue-green schema transitions**: Read-only mode for schema-ahead situations means the old server degrades gracefully during the blue-green window. This is safer than typical Docker Compose blue-green.

4. **Caddy + tailscale plugin is the recommended proxy**: Admin API enables programmatic blue-green switching, and the tailscale/caddy-tailscale plugin provides automatic HTTPS on the tailnet.

5. **Embedding status already exposed**: `/health` endpoint has `embedding_progress.pending` — the smoke test can poll this every 30s until pending == 0 for the test entries.

## What Already Exists (reuse, don't rebuild)

| Component | Location | Reuse Strategy |
|-----------|----------|---------------|
| Test compose file | `docker-compose.test.yml` | Base for smoke test stack |
| E2E smoke tests | `tests/e2e.sh` | Extract ingest+search subset |
| Embedding poll | `tests/e2e-ollama.sh` lines 85-102 | Adapt polling loop |
| wait_for_url | `scripts/lib.sh` | Use directly |
| Health endpoint | `server/src/main.py` /health | Poll for status + embedding progress |
| Git signal pattern | `~/.local/bin/git` | Extend for memlayer |
| systemd service template | `scripts/memlayer-daemon.service.template` | Pattern for deploy service |
| .env.example | repo root | Template for brainframe production .env |

## What Must Be Built

1. **Deploy signal**: Extend git wrapper + add systemd path/service units on brainframe
2. **Deploy orchestration script**: checkout → smoke test → blue-green switch
3. **Smoke test script**: ephemeral stack → seed → ingest → poll embeddings → search → verify
4. **Failure artifact**: tarball creation on smoke test failure
5. **Blue-green compose files**: separate blue/green overrides sharing DB
6. **Caddy config**: reverse proxy with admin API switching
7. **Slack notification**: curl to webhook on success/failure
8. **Production .env**: secrets for brainframe deployment
