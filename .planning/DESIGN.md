# Design: Memlayer Brainframe Deployment Pipeline

## Architecture Overview

```
 AGENTSMITH CONTAINER                        BRAINFRAME HOST
 ========================                    ====================================

 git push origin main
        |
 ~/.local/bin/git wrapper
        |
 ssh memlayer-deploy@brainframe ----------> touch /var/run/memlayer/signals/deploy
   (command-restricted Ed25519 key)                  |
                                             systemd .path unit (inotify)
                                                     |
                                             systemd .service unit (User=memlayer-deploy)
                                                     |
                                             /opt/memlayer/deploy/brainframe-deploy.sh
                                              |
                                              +-- flock deploy.lock
                                              +-- git pull
                                              +-- docker build (tagged with version+timestamp)
                                              +-- SMOKE TEST
                                              |    +-- ephemeral PG (tmpfs) + new server
                                              |    +-- seed 3 test entries via /api/ingest
                                              |    +-- poll /health embedding_progress every 30s
                                              |    +-- verify /api/search returns results
                                              |    +-- on fail: tarball artifact, cleanup, Slack alert
                                              |    +-- on pass: cleanup
                                              +-- BLUE-GREEN DEPLOY
                                              |    +-- start new color (blue or green)
                                              |    +-- health gate (60s timeout)
                                              |    +-- Caddy admin API: switch upstream
                                              |    +-- stop old color container
                                              +-- Slack notification (success/failure)

 DOCKER TOPOLOGY ON BRAINFRAME
 ====================================

        Tailscale (memlayer.<tailnet>.ts.net)
                 |
          [ Caddy ] (auto-HTTPS via tailscale plugin)
           admin API on 127.0.0.1:2019
          /                      \
 [memlayer-blue:8430]    [memlayer-green:8431]
          \                      /
           [  memlayer-prod-db  ]
           (bridge: memlayer-net)
           (volume: memlayer-pgdata, external)
```

## Components

### 1. Signal Mechanism
- Git wrapper at `~/.local/bin/git` (in container) extended with memlayer-specific block
- SSH to `memlayer-deploy@brainframe` with `ConnectTimeout=5`
- Key is Ed25519, command-restricted in `authorized_keys` to only run the signal script
- Signal failure is a warning, never blocks the git push
- Signal file: `/var/run/memlayer/signals/deploy`

### 2. systemd Units
- `.path` unit watches signal file via `PathChanged=`
- `.service` unit runs deploy script as `memlayer-deploy` user
- `ConditionPathExists=!/var/run/memlayer/deploy.lock` prevents overlap
- Catch-up timer (every 5 min): compares deployed commit vs `origin/main` to handle missed signals

### 3. Deploy Orchestration (`deploy/brainframe-deploy.sh`)
- `flock /var/run/memlayer/deploy.lock` for exclusive access
- `git fetch && git reset --hard origin/main`
- Build image tagged `memlayer-server:<version>-<deploy-id>`
- Call `deploy/smoke-test.sh` — exit 0 = pass, non-zero = fail
- Call `deploy/blue-green-switch.sh` — switches Caddy and stops old container
- Post-deploy production health probe (single ingest + verify)
- Slack notification via curl to webhook file
- All output tee'd to `/var/log/memlayer/deploy-<id>.log`

### 4. Smoke Test (`deploy/smoke-test.sh`)
- Unique project: `memlayer-smoke-<timestamp>`
- `deploy/docker-compose.smoke.yml`: PG on tmpfs (fsync=off), server on port 8499
- Compose override generated at runtime with built image tag + randomized credentials
- Uses `SMOKE_OPENAI_API_KEY` from test.env (separate from production)
- Seed 3 realistic entries via `/api/ingest`
- Poll `/health` `embedding_progress.pending` every 30s (max 3 min)
- Verify `/api/search` returns results
- On failure: save tarball (docker logs + pg_dump + health.json + compose file), then cleanup
- On success: cleanup immediately
- `trap cleanup EXIT` guarantees no orphaned containers
- Tarball retention: keep last 5, auto-delete older

### 5. Blue-Green (`deploy/blue-green-switch.sh`)
- State file: `/opt/memlayer/.active-color` (blue or green)
- Blue always on port 8430, green on 8431
- Shared DB via `memlayer-net` bridge network
- Shared response-files volume
- New color starts, health-gated (60s), Caddy switches, old color stops
- For breaking schema changes: stop old server first, accept brief downtime
- `deploy/docker-compose.prod.yml` defines both services + shared DB

### 6. Caddy
- Reverse proxy with `tailscale/caddy-tailscale` plugin for auto-HTTPS
- Admin API on `127.0.0.1:2019` (localhost only, origins restricted)
- `deploy/caddy-switch.sh` POSTs full config to `/load` endpoint (atomic swap)
- `request_body { max_size 10MB }` for defense-in-depth

### 7. Data Migration
- pg_dump directly from Supabase (bypasses crash-looping Fly.io)
- Restore into brainframe's production PG
- One-time operation during initial setup

## Security Model

### Users and Permissions
```
memlayer-deploy (nologin shell)
  - Docker group membership
  - Owns: /opt/memlayer/repo/, signal directory
  - SSH key: command-restricted to signal script only

Files:
  /opt/memlayer/production.env    # 0600 memlayer-deploy:memlayer-deploy
  /opt/memlayer/test.env          # 0600 memlayer-deploy:memlayer-deploy
  /opt/memlayer/slack-webhook.url # 0600 memlayer-deploy:memlayer-deploy
  deploy scripts                  # 0755 root:root (not writable by deploy user)
```

### Key Mitigations
- SSH key command-restricted + no-port-forwarding + no-pty
- Deploy scripts owned by root, not modifiable by deploy user
- Test and production env files validated to have different keys
- Caddy admin API localhost-only with origins restriction
- pg_dump uses PGPASSWORD env var, not CLI argument

## File Layout on Brainframe

```
/opt/memlayer/
    repo/                               # Git checkout
        deploy/
            brainframe-deploy.sh        # Main orchestration
            smoke-test.sh               # Smoke test runner
            blue-green-switch.sh        # Traffic switcher
            caddy-switch.sh             # Caddy admin API caller
            docker-compose.prod.yml     # Production blue/green stack
            docker-compose.smoke.yml    # Ephemeral smoke test stack
            systemd/                    # Unit files (installed to /etc/systemd/system/)
            setup-brainframe.sh         # One-time setup script
    production.env                      # Production secrets
    test.env                            # Smoke test secrets
    slack-webhook.url                   # Slack webhook URL
    .active-color                       # "blue" or "green"
    .tailnet-domain                     # Tailnet suffix for Caddy hostname

/var/run/memlayer/
    signals/deploy                      # Signal file
    deploy.lock                         # flock target

/var/log/memlayer/
    deploy-*.log                        # Per-deploy logs
    failures/*.tar.gz                   # Failure tarballs (keep 5)
```

## Files to Create

| File | Purpose |
|------|---------|
| `deploy/brainframe-deploy.sh` | Main orchestration |
| `deploy/smoke-test.sh` | Ephemeral smoke test |
| `deploy/blue-green-switch.sh` | Blue-green traffic switch |
| `deploy/caddy-switch.sh` | Caddy upstream switcher |
| `deploy/docker-compose.prod.yml` | Production stack (DB + blue + green) |
| `deploy/docker-compose.smoke.yml` | Smoke test stack |
| `deploy/systemd/memlayer-deploy.path` | Signal watcher |
| `deploy/systemd/memlayer-deploy.service` | Deploy runner |
| `deploy/systemd/memlayer-deploy-catchup.timer` | Missed-signal catch-up |
| `deploy/systemd/memlayer-deploy-catchup.service` | Catch-up check |
| `deploy/setup-brainframe.sh` | One-time setup (user, dirs, Caddy, systemd) |
| `deploy/Caddyfile` | Initial Caddy config template |
| `deploy/memlayer-signal.sh` | Command-restricted SSH target script |

## Files to Modify

| File | Change |
|------|--------|
| `~/.local/bin/git` (in container) | Add memlayer SSH signal block |
| `.gitignore` | Add production.env, test.env, .active-color, .tailnet-domain |
