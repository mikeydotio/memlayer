# Implementation Plan: Memlayer Brainframe Deployment Pipeline

**Created:** 2026-03-30
**Source:** IDEA.md (requirements), DESIGN.md (architecture), research/SUMMARY.md (findings)
**Current version:** v2.1.0

---

## Overview

Migrate memlayer server from Fly.io to brainframe with a validated deployment pipeline: push-triggered, smoke-tested, blue-green deployed. Six waves, each leaving the system in a testable state.

**Total files to create:** 13
**Total files to modify:** 2
**Estimated total effort:** 14-18 hours across waves

---

## Progress

### Wave 1 -- Core Deploy Scripts [NOT STARTED]

**Goal:** Three shell scripts that form the deployment logic, testable locally via dry-run flags.

**Resumption state after Wave 1:** All three scripts exist in `deploy/`, pass shellcheck, and can be invoked with `--dry-run` to trace their logic without touching Docker or Caddy. No infrastructure changes yet.

- [ ] Task 1.1: `deploy/brainframe-deploy.sh` -- Main orchestration script
  - **Files to create:** `/home/mikey/memlayer/deploy/brainframe-deploy.sh`
  - **Acceptance criteria:**
    1. Script is executable and passes `shellcheck deploy/brainframe-deploy.sh`
    2. Uses `flock /var/run/memlayer/deploy.lock` for exclusive execution
    3. Runs `git fetch origin && git reset --hard origin/main`
    4. Reads version from `VERSION` file, generates deploy-id as `<version>-<YYYYMMDDHHMMSS>`
    5. Builds Docker image tagged `memlayer-server:<version>-<deploy-id>` using `deploy/Dockerfile.cloud` with `--build-arg MEMLAYER_VERSION=<version>`
    6. Calls `deploy/smoke-test.sh <image-tag>` and aborts on non-zero exit
    7. Calls `deploy/blue-green-switch.sh <image-tag>` on smoke pass
    8. Runs a post-deploy production health probe: single entry ingest via `/api/ingest` then `/api/search` verification, with 30s timeout
    9. Sends Slack notification: reads webhook URL from `/opt/memlayer/slack-webhook.url`, sends JSON payload via `curl` with deploy-id, version, status (success/failure), duration, and commit SHA
    10. All output tee'd to `/var/log/memlayer/deploy-<deploy-id>.log`
    11. `--dry-run` flag prints each step it would execute without running Docker, git, or curl commands
    12. Sources `/opt/memlayer/production.env` for production secrets
    13. Logs total deploy duration (start/end timestamps)
    14. On any failure: sends Slack failure notification before exiting
  - **Depends on:** Nothing
  - **Effort:** 2-3 hours
  - **Reuse:** Output formatting functions from `scripts/lib.sh` (`info`, `success`, `warn`, `error`)

- [ ] Task 1.2: `deploy/smoke-test.sh` -- Ephemeral smoke test runner
  - **Files to create:** `/home/mikey/memlayer/deploy/smoke-test.sh`
  - **Acceptance criteria:**
    1. Script is executable and passes `shellcheck deploy/smoke-test.sh`
    2. Accepts image tag as first positional argument (e.g., `memlayer-server:2.1.0-20260330120000`)
    3. Generates unique project name: `memlayer-smoke-<timestamp>` for compose project isolation
    4. Generates runtime compose override file (tmpfile) that injects: the image tag, a randomized DB password (via `openssl rand -hex 16`), and `SMOKE_OPENAI_API_KEY`
    5. Starts ephemeral stack: `docker compose -p <project> -f deploy/docker-compose.smoke.yml -f <override> up -d`
    6. Waits for server health: polls `http://127.0.0.1:8499/health` every 2s, 60s timeout (reuses `wait_for_url` pattern from `scripts/lib.sh`)
    7. Seeds 3 test entries via `POST /api/ingest` with distinct content (payload pattern from `tests/e2e.sh` lines 44-80)
    8. Polls `/health` checking `embedding_progress.pending` every 30s, maximum 3 minutes, until pending reaches 0
    9. Verifies `POST /api/search` with query matching seed content returns `total >= 1`
    10. On failure: captures tarball to `/var/log/memlayer/failures/<deploy-id>-<timestamp>.tar.gz` containing: `docker compose logs` output, `pg_dump` of smoke DB, `/health` JSON snapshot, compose override file
    11. On failure: retains last 5 tarballs in `/var/log/memlayer/failures/`, deletes older ones (sorted by name)
    12. `trap cleanup EXIT` that runs `docker compose -p <project> -f ... down -v --remove-orphans` regardless of exit path
    13. Sources `/opt/memlayer/test.env` for `SMOKE_OPENAI_API_KEY`
    14. Validates `SMOKE_OPENAI_API_KEY` differs from production `OPENAI_API_KEY` (sources both env files and compares; aborts if identical)
    15. Exits 0 on pass, 1 on fail
    16. `--dry-run` flag traces steps without running containers
  - **Depends on:** Nothing (can be developed in parallel with 1.1)
  - **Effort:** 3-4 hours
  - **Reuse:** Ingest payload structure from `tests/e2e.sh` lines 44-80; embedding poll pattern from `tests/e2e-ollama.sh` lines 85-102; `wait_for_url` from `scripts/lib.sh` line 179

- [ ] Task 1.3: `deploy/blue-green-switch.sh` -- Blue-green traffic switcher
  - **Files to create:** `/home/mikey/memlayer/deploy/blue-green-switch.sh`
  - **Acceptance criteria:**
    1. Script is executable and passes `shellcheck deploy/blue-green-switch.sh`
    2. Accepts image tag as first positional argument
    3. Reads current active color from `/opt/memlayer/.active-color` (defaults to `blue` if file missing or empty)
    4. Determines new color: if active is `blue` then new is `green`, and vice versa
    5. Exports `MEMLAYER_IMAGE=<image-tag>` for compose file interpolation
    6. Starts new color: `docker compose -f deploy/docker-compose.prod.yml up -d memlayer-<new-color>` with the image tag
    7. Health-gates new container: polls `http://127.0.0.1:<port>/health` with 60s timeout (blue=8430, green=8431)
    8. On health-gate pass: calls `deploy/caddy-switch.sh <new-color>` to switch upstream
    9. On Caddy switch success: stops old color container `docker compose -f deploy/docker-compose.prod.yml stop memlayer-<old-color>`
    10. Updates `/opt/memlayer/.active-color` to new color only after successful Caddy switch
    11. On health-gate failure: stops new container, does NOT update `.active-color`, exits 1
    12. On Caddy switch failure: stops new container, does NOT update `.active-color`, exits 1
    13. `--dry-run` flag traces logic without running Docker or Caddy commands
    14. Prints timing: how long the new container took to become healthy
  - **Depends on:** Nothing (can be developed in parallel with 1.1 and 1.2)
  - **Effort:** 1.5-2 hours
  - **Reuse:** `wait_for_url` pattern from `scripts/lib.sh`

---

### Wave 2 -- Docker Compose Files [NOT STARTED]

**Goal:** Production and smoke test compose files, testable with local Docker (no brainframe needed).

**Resumption state after Wave 2:** Compose files exist and validate with `docker compose config`. The smoke compose can be brought up locally to verify the ephemeral stack pattern. Production compose defines blue, green, and DB services with correct networking.

- [ ] Task 2.1: `deploy/docker-compose.prod.yml` -- Production blue/green stack
  - **Files to create:** `/home/mikey/memlayer/deploy/docker-compose.prod.yml`
  - **Acceptance criteria:**
    1. Defines three services: `memlayer-prod-db`, `memlayer-blue`, `memlayer-green`
    2. `memlayer-prod-db`:
       - Image: `pgvector/pgvector:pg16`
       - Container name: `memlayer-prod-db`
       - External volume: `memlayer-pgdata` (declared as `external: true` under volumes)
       - Mounts: `../db/migrations:/docker-entrypoint-initdb.d/migrations:ro` and `../db/init.sh:/docker-entrypoint-initdb.d/00-init.sh:ro`
       - Healthcheck: `pg_isready -U memlayer -d memlayer` with 5s interval, 3s timeout, 10 retries
       - `shm_size: 256mb`
       - Port: `127.0.0.1:5432:5432`
       - Environment: `POSTGRES_DB`, `POSTGRES_USER`, `POSTGRES_PASSWORD` from env
    3. `memlayer-blue`:
       - Image: `${MEMLAYER_IMAGE}` (set at runtime)
       - Container name: `memlayer-blue`
       - Port: `127.0.0.1:8430:8420`
       - `depends_on: memlayer-prod-db: condition: service_healthy`
       - `env_file: /opt/memlayer/production.env`
       - Mounts: `response-files:/data/response_files`, `../db/migrations:/app/migrations:ro`
       - `DATABASE_URL` pointing to `memlayer-prod-db`
       - Healthcheck: curl to `localhost:8420/health`
    4. `memlayer-green`: identical to blue except container name `memlayer-green` and port `8431:8420`
    5. All three services on `memlayer-net` network (declared as `external: true`)
    6. `response-files` volume declared (shared between blue and green)
    7. `docker compose -f deploy/docker-compose.prod.yml config` succeeds when `MEMLAYER_IMAGE`, `POSTGRES_PASSWORD`, and `POSTGRES_USER` env vars are set
    8. Blue and green services have `restart: unless-stopped`
  - **Depends on:** Nothing
  - **Effort:** 1-1.5 hours
  - **Reuse:** Service structure from `docker-compose.yml` (env vars, volumes, healthchecks); `Dockerfile.cloud` already exists at `deploy/Dockerfile.cloud`

- [ ] Task 2.2: `deploy/docker-compose.smoke.yml` -- Ephemeral smoke test stack
  - **Files to create:** `/home/mikey/memlayer/deploy/docker-compose.smoke.yml`
  - **Acceptance criteria:**
    1. Defines two services: `smoke-db`, `smoke-server`
    2. `smoke-db`:
       - Image: `pgvector/pgvector:pg16`
       - tmpfs mount at `/var/lib/postgresql/data` for ephemeral storage
       - Command override: `postgres -c fsync=off -c full_page_writes=off` for speed
       - `POSTGRES_DB=memlayer_smoke`, `POSTGRES_USER=memlayer`, `POSTGRES_PASSWORD=${SMOKE_DB_PASSWORD}`
       - Mounts: `../db/migrations:/docker-entrypoint-initdb.d/migrations:ro` and `../db/init.sh:/docker-entrypoint-initdb.d/00-init.sh:ro`
       - Healthcheck: `pg_isready` with 2s interval, 2s timeout, 15 retries
       - `shm_size: 128mb`
    3. `smoke-server`:
       - Image: `${MEMLAYER_IMAGE}` (set at runtime by smoke-test.sh)
       - Port: `127.0.0.1:8499:8420`
       - `depends_on: smoke-db: condition: service_healthy`
       - Environment: `DATABASE_URL` pointing to `smoke-db`, `MEMLAYER_AUTH_TOKEN=${SMOKE_AUTH_TOKEN}`, `EMBEDDING_PROVIDER=openai`, `OPENAI_API_KEY=${SMOKE_OPENAI_API_KEY}`, `INDEX_MODE=off`, `EXTRACTION_MODE=off`, `LOG_LEVEL=DEBUG`
       - Mounts: `../db/migrations:/app/migrations:ro`
       - Healthcheck: curl to `localhost:8420/health`, 3s interval, 5s timeout, 10 retries, 10s start_period
       - No persistent volumes (everything ephemeral)
    4. Both on `smoke-net` network (created per-project, not external)
    5. No volumes declared as external (everything torn down with `down -v`)
    6. `docker compose -f deploy/docker-compose.smoke.yml config` succeeds when `MEMLAYER_IMAGE`, `SMOKE_DB_PASSWORD`, `SMOKE_AUTH_TOKEN`, `SMOKE_OPENAI_API_KEY` env vars are set
  - **Depends on:** Nothing
  - **Effort:** 1 hour
  - **Reuse:** Structure from `docker-compose.test.yml` (isolated network, healthchecks, test config)

---

### Wave 3 -- Caddy Configuration and Switching [NOT STARTED]

**Goal:** Caddy reverse proxy config and the script that atomically switches upstream between blue and green via the admin API.

**Resumption state after Wave 3:** Caddy config template exists. `caddy-switch.sh` can be tested locally against a running Caddy container with admin API exposed. Combined with Waves 1 and 2, the full deploy pipeline logic is code-complete (though not wired to systemd triggers yet).

- [ ] Task 3.1: `deploy/Caddyfile` -- Initial Caddy configuration template
  - **Files to create:** `/home/mikey/memlayer/deploy/Caddyfile`
  - **Acceptance criteria:**
    1. Top-level comment explaining this is a template; the actual config is managed via `caddy-switch.sh` POSTing JSON to the admin API
    2. Uses `tailscale` listener for the hostname (placeholder: `memlayer.{TAILNET_DOMAIN}`)
    3. Reverse proxy directive pointing to `localhost:8430` (blue) as default upstream
    4. `request_body` directive with `max_size 10MB`
    5. Admin API block: `admin 127.0.0.1:2019` with `origins 127.0.0.1`
    6. Access log and error log configured to `/var/log/caddy/`
    7. Comment noting the caddy-tailscale plugin is required: `xcaddy build --with github.com/tailscale/caddy-tailscale`
    8. File is valid Caddyfile syntax (verifiable with `caddy fmt` if available)
  - **Depends on:** Nothing
  - **Effort:** 0.5 hours

- [ ] Task 3.2: `deploy/caddy-switch.sh` -- Caddy admin API upstream switcher
  - **Files to create:** `/home/mikey/memlayer/deploy/caddy-switch.sh`
  - **Acceptance criteria:**
    1. Script is executable and passes `shellcheck deploy/caddy-switch.sh`
    2. Accepts target color as first positional argument (`blue` or `green`); exits 1 on invalid input
    3. Maps `blue` to port 8430, `green` to port 8431
    4. Reads tailnet domain from `/opt/memlayer/.tailnet-domain` file; exits 1 if file missing or empty
    5. Constructs full Caddy JSON config including:
       - Admin API on `127.0.0.1:2019` with origins restriction
       - App: http server with tailscale listener for `memlayer.<tailnet-domain>`
       - Route: reverse_proxy to `localhost:<target-port>`
       - Request body max size: 10MB
    6. POSTs the JSON config to `http://127.0.0.1:2019/load` via `curl -X POST -H "Content-Type: application/json"`
    7. Verifies the POST returns HTTP 200; exits 1 on any other status code, printing the response body
    8. `--dry-run` flag prints the JSON payload to stdout without POSTing
    9. `--status` flag GETs `http://127.0.0.1:2019/config/` and prints current upstream port/color
    10. JSON config is well-formed (can be validated with `python3 -m json.tool` in tests)
  - **Depends on:** Task 3.1 (must match the Caddy config structure)
  - **Effort:** 1-1.5 hours

---

### Wave 4 -- systemd Units and Signal Mechanism [NOT STARTED]

**Goal:** The trigger infrastructure: systemd path/service/timer units and the SSH signal target script. After this wave, the full automated pipeline is code-complete.

**Resumption state after Wave 4:** All systemd unit files exist in `deploy/systemd/`. The signal script exists. Combined with Waves 1-3, the entire pipeline is code-complete. What remains is installation (Wave 5) and data migration (Wave 6).

- [ ] Task 4.1: `deploy/systemd/memlayer-deploy.path` -- Signal watcher unit
  - **Files to create:** `/home/mikey/memlayer/deploy/systemd/memlayer-deploy.path`
  - **Acceptance criteria:**
    1. Valid systemd path unit (passes `systemd-analyze verify` if available)
    2. `[Path]` section: `PathChanged=/var/run/memlayer/signals/deploy`
    3. `[Unit]` section: `Description=Watch for memlayer deploy signals`
    4. `[Install]` section: `WantedBy=multi-user.target`
    5. Triggers `memlayer-deploy.service` (implicit from matching name)
  - **Depends on:** Nothing
  - **Effort:** 0.25 hours

- [ ] Task 4.2: `deploy/systemd/memlayer-deploy.service` -- Deploy runner unit
  - **Files to create:** `/home/mikey/memlayer/deploy/systemd/memlayer-deploy.service`
  - **Acceptance criteria:**
    1. Valid systemd service unit
    2. `Type=oneshot` (deploy runs to completion)
    3. `User=memlayer-deploy`
    4. `Group=memlayer-deploy`
    5. `ExecStart=/opt/memlayer/repo/deploy/brainframe-deploy.sh`
    6. `ConditionPathExists=!/var/run/memlayer/deploy.lock` to prevent concurrent deploys
    7. `After=network-online.target docker.service`
    8. `Wants=network-online.target`
    9. `StandardOutput=journal`
    10. `StandardError=journal`
    11. `Environment=PATH=/usr/local/bin:/usr/bin:/bin` (ensures Docker is findable)
    12. `WorkingDirectory=/opt/memlayer/repo`
    13. `TimeoutStartSec=600` (10 minutes max for full pipeline)
  - **Depends on:** Nothing
  - **Effort:** 0.25 hours

- [ ] Task 4.3: `deploy/systemd/memlayer-deploy-catchup.timer` and `memlayer-deploy-catchup.service` -- Missed-signal catch-up
  - **Files to create:** `/home/mikey/memlayer/deploy/systemd/memlayer-deploy-catchup.timer`, `/home/mikey/memlayer/deploy/systemd/memlayer-deploy-catchup.service`
  - **Acceptance criteria:**
    1. Timer: `OnCalendar=*:0/5` (every 5 minutes)
    2. Timer: `Persistent=true` (run missed intervals after boot)
    3. Timer: `[Install] WantedBy=timers.target`
    4. Service: `Type=oneshot`
    5. Service: `User=memlayer-deploy`
    6. Service: `WorkingDirectory=/opt/memlayer/repo`
    7. Service `ExecStart=` runs a comparison script that:
       a. Reads the current deployed commit from `/opt/memlayer/.deployed-commit` (file written by `brainframe-deploy.sh` after successful deploy)
       b. Runs `git fetch origin --quiet` then reads `git rev-parse origin/main`
       c. If they differ: `touch /var/run/memlayer/signals/deploy` (triggers the path unit)
       d. If they match: exits silently (exit 0)
    8. Service: `Environment=PATH=/usr/local/bin:/usr/bin:/bin`
    9. Both units pass `systemd-analyze verify` if available
  - **Depends on:** Nothing
  - **Effort:** 0.5 hours
  - **Note:** Task 1.1 (brainframe-deploy.sh) must write `/opt/memlayer/.deployed-commit` after successful deploy. Add this to Task 1.1 acceptance criteria.

- [ ] Task 4.4: `deploy/memlayer-signal.sh` -- SSH command-restricted signal target
  - **Files to create:** `/home/mikey/memlayer/deploy/memlayer-signal.sh`
  - **Acceptance criteria:**
    1. Script is executable and passes `shellcheck deploy/memlayer-signal.sh`
    2. Only action: `touch /var/run/memlayer/signals/deploy`
    3. Writes a timestamped log line to `/var/log/memlayer/signal.log` with format: `<ISO8601> signal received from ${SSH_CONNECTION%% *}` (source IP from SSH_CONNECTION env var)
    4. Exits 0 always (signal delivery is best-effort; errors logged but not propagated)
    5. No arguments accepted or processed
    6. Script is 15 lines or fewer (minimal attack surface)
  - **Depends on:** Nothing
  - **Effort:** 0.25 hours

---

### Wave 5 -- Setup Script, Git Wrapper, Notifications [NOT STARTED]

**Goal:** The one-time setup script that installs everything on brainframe, the git wrapper modification for agentsmith containers, and gitignore updates. After this wave, the pipeline can be installed and tested end-to-end.

**Resumption state after Wave 5:** Running `deploy/setup-brainframe.sh` on brainframe creates the user, directories, installs systemd units, generates SSH keypair, and starts the watchers. The git wrapper in the container triggers deploys on push. The pipeline is fully operational but pointing at an empty database.

- [ ] Task 5.1: `deploy/setup-brainframe.sh` -- One-time brainframe setup
  - **Files to create:** `/home/mikey/memlayer/deploy/setup-brainframe.sh`
  - **Acceptance criteria:**
    1. Script is executable, passes `shellcheck`, requires root (`[[ $EUID -ne 0 ]] && exit 1`)
    2. Creates `memlayer-deploy` system user:
       - `useradd --system --shell /usr/sbin/nologin --home-dir /opt/memlayer memlayer-deploy`
       - Adds to `docker` group: `usermod -aG docker memlayer-deploy`
    3. Creates directory structure with correct ownership:
       - `/opt/memlayer/repo/` (memlayer-deploy:memlayer-deploy)
       - `/var/run/memlayer/signals/` (memlayer-deploy:memlayer-deploy)
       - `/var/log/memlayer/failures/` (memlayer-deploy:memlayer-deploy)
    4. Creates tmpfiles.d config for `/var/run/memlayer/` (survives reboot since `/var/run` is tmpfs):
       - Installs `/etc/tmpfiles.d/memlayer.conf` with `d /var/run/memlayer 0755 memlayer-deploy memlayer-deploy -` and subdirs
    5. Creates external Docker volume `memlayer-pgdata` if not exists: `docker volume create memlayer-pgdata`
    6. Creates external Docker network `memlayer-net` if not exists: `docker network create memlayer-net`
    7. Clones the memlayer repo into `/opt/memlayer/repo/` (or `git pull` if already cloned)
    8. Generates Ed25519 SSH keypair for `memlayer-deploy`:
       - `ssh-keygen -t ed25519 -f /opt/memlayer/.ssh/id_ed25519 -N "" -C "memlayer-deploy@brainframe"`
       - Sets up `/opt/memlayer/.ssh/authorized_keys` with `command="/opt/memlayer/repo/deploy/memlayer-signal.sh",no-port-forwarding,no-X11-forwarding,no-agent-forwarding,no-pty <pubkey>`
       - Sets permissions: `.ssh/` 0700, `authorized_keys` 0600, all owned by memlayer-deploy
    9. Installs systemd units: copies `deploy/systemd/*.path`, `*.service`, `*.timer` to `/etc/systemd/system/`
    10. Runs `systemctl daemon-reload`
    11. Enables and starts: `memlayer-deploy.path`, `memlayer-deploy-catchup.timer`
    12. Creates placeholder secret files with 0600 permissions (memlayer-deploy:memlayer-deploy):
        - `/opt/memlayer/production.env` (with commented template showing required vars)
        - `/opt/memlayer/test.env` (with commented template)
        - `/opt/memlayer/slack-webhook.url` (empty)
        - `/opt/memlayer/.tailnet-domain` (empty)
        - `/opt/memlayer/.active-color` (contains "blue")
        - `/opt/memlayer/.deployed-commit` (empty)
    13. Sets deploy scripts to root:root 0755: `chown root:root /opt/memlayer/repo/deploy/*.sh && chmod 755 /opt/memlayer/repo/deploy/*.sh`
    14. Prints post-setup instructions:
        a. SSH public key to copy to agentsmith containers
        b. Secrets to fill in (production.env, test.env, slack-webhook.url, .tailnet-domain)
        c. Caddy installation steps (xcaddy build with tailscale plugin)
        d. DNS/Tailscale hostname configuration
    15. Idempotent: all create operations check before acting (`id -u memlayer-deploy`, `docker volume ls`, etc.)
    16. Sources `scripts/lib.sh` for output formatting
  - **Depends on:** Tasks 4.1-4.4 (needs systemd unit files to exist for installation)
  - **Effort:** 2-3 hours

- [ ] Task 5.2: Git wrapper signal snippet
  - **Files to create:** `/home/mikey/memlayer/deploy/git-wrapper-snippet.sh`
  - **Acceptance criteria:**
    1. Contains a bash code block (with clear `# --- BEGIN memlayer deploy signal ---` / `# --- END ---` markers) to be inserted into `~/.local/bin/git` in agentsmith containers
    2. Detection logic: checks if the git command is `push`, the remote URL contains `memlayer`, and the target ref is `main`
    3. On match: runs `ssh -o ConnectTimeout=5 -o StrictHostKeyChecking=accept-new -i <keypath> memlayer-deploy@<brainframe-host> true` (the command= restriction in authorized_keys handles the rest)
    4. SSH failure outputs a warning to stderr: `"[memlayer] deploy signal failed (non-blocking)"` and does NOT affect the exit code of the git push
    5. The snippet itself passes `shellcheck` (tested standalone)
    6. Includes a header comment documenting: where to insert, what key file to use, what brainframe host to configure
    7. Uses `<BRAINFRAME_HOST>` and `<KEY_PATH>` as placeholders with instructions to replace
  - **Depends on:** Task 5.1 (needs to know SSH key path convention)
  - **Effort:** 0.5 hours

- [ ] Task 5.3: `.gitignore` updates for brainframe deployment files
  - **Files to modify:** `/home/mikey/memlayer/.gitignore`
  - **Acceptance criteria:**
    1. Adds a `# Brainframe deployment secrets (not in repo, live on brainframe)` comment block
    2. Adds patterns: `production.env`, `test.env`, `.active-color`, `.tailnet-domain`, `slack-webhook.url`, `.deployed-commit`
    3. Existing `.gitignore` entries are unchanged
    4. New entries are appended at the end, separated by a blank line
  - **Depends on:** Nothing
  - **Effort:** 0.1 hours

---

### Wave 6 -- Data Migration and Go-Live [NOT STARTED]

**Goal:** Migrate existing data from Supabase to brainframe's production PG, verify the migration, and switch production traffic. After this wave, memlayer is fully operational on brainframe.

**Resumption state after Wave 6:** Production memlayer runs on brainframe, accessible via Tailnet. All historical data migrated from Supabase. Fly.io can be decommissioned.

- [ ] Task 6.1: `deploy/migrate-from-supabase.sh` -- Data migration script
  - **Files to create:** `/home/mikey/memlayer/deploy/migrate-from-supabase.sh`
  - **Acceptance criteria:**
    1. Script is executable and passes `shellcheck deploy/migrate-from-supabase.sh`
    2. Accepts Supabase connection string as `--source` argument (or reads from `SUPABASE_DATABASE_URL` env var)
    3. Runs `pg_dump` against Supabase with flags: `--data-only --no-owner --no-acl --no-comments` (schema managed by memlayer migrations, not imported from Supabase)
    4. Uses `PGPASSWORD` env var for credential passing (not CLI argument, not visible in `ps`)
    5. Validates dump file: non-empty, contains `COPY` statements for expected tables (`entries`, `sessions`, `entities`, `relationships`, `embeddings`)
    6. Creates a pre-migration backup of brainframe production DB: `pg_dump` to `/opt/memlayer/backups/pre-migration-<timestamp>.sql.gz`
    7. Restores into brainframe production PG via `psql` with `--single-transaction`
    8. Runs count verification: queries `SELECT count(*) FROM entries` on both source and destination, prints comparison
    9. Prints summary: tables migrated, row counts per table, warnings
    10. `--dry-run` flag: performs dump and validation but does not restore
    11. `--skip-backup` flag: skips the pre-migration backup step (for empty target DBs)
    12. Exits non-zero if source and destination row counts do not match
  - **Depends on:** Task 2.1 (production DB must be definable/runnable)
  - **Effort:** 1.5-2 hours

- [ ] Task 6.2: `deploy/go-live-checklist.md` -- Go-live verification procedure
  - **Files to create:** `/home/mikey/memlayer/deploy/go-live-checklist.md`
  - **Acceptance criteria:**
    1. Ordered checklist with verification commands for each step:
       a. Pre-flight: brainframe setup complete (systemd units active, secrets filled, Caddy running)
       b. Pre-flight: production DB running, migrations applied, blue server healthy
       c. Run smoke test manually: `deploy/smoke-test.sh memlayer-server:<current-version>`
       d. Run data migration: `deploy/migrate-from-supabase.sh --source <conn-string>`
       e. Verify migrated data: `memlayer search "test query" --server http://brainframe:8430`
       f. Verify entry counts match Supabase
       g. Trigger first blue-green deploy: `touch /var/run/memlayer/signals/deploy`
       h. Verify Tailnet accessibility: `curl https://memlayer.<tailnet>/health` from agentsmith container
       i. Update daemon configs: set `MEMLAYER_SERVER_URL` to brainframe Tailnet URL in all containers
       j. Verify daemon ingests: check server logs for new entries
       k. Monitor for 30 minutes: watch health endpoint, check Slack for alerts
       l. Decommission Fly.io: `flyctl apps destroy memlayer`
    2. Each step includes a rollback procedure
    3. Documents the "point of no return" (after daemon configs are switched)
    4. Includes estimated duration per step
    5. Includes prerequisite checks (Docker, Caddy, Tailscale, network connectivity)
  - **Depends on:** All previous waves
  - **Effort:** 0.5 hours

---

## Deviations Log

| Task | Original Plan | Actual | Reason |
|------|--------------|--------|--------|

---

## Requirement Traceability

| # | Requirement (from IDEA.md) | Task(s) | Status |
|---|---------------------------|---------|--------|
| R1 | Signal mechanism: git push from container triggers brainframe | 4.1, 4.4, 5.2 | pending |
| R2 | Brainframe listener: receives signal, checks out code, runs pipeline | 1.1, 4.1, 4.2 | pending |
| R3 | Ephemeral test stack: fresh PG + server per dry-run, torn down after | 1.2, 2.2 | pending |
| R4 | Realistic test data: seed with representative sessions/entries | 1.2 | pending |
| R5 | Full smoke test: health -> ingest -> poll embeddings -> search verify | 1.2 | pending |
| R6 | Embedding status polling: check /health every 30s until complete | 1.2 | pending |
| R7 | Failure artifact: timestamped tarball of DB, config, logs on failure | 1.2 | pending |
| R8 | Blue-green production deploy: new container, health check, switch, remove old | 1.3, 2.1, 3.2 | pending |
| R9 | Slack notification: post deploy result to webhook | 1.1 | pending |
| R10 | Separate test OpenAI API key for smoke test | 1.2, 2.2, 5.1 | pending |
| R11 | Data migration from Fly.io/Supabase | 6.1 | pending |
| R12 | Production stack: docker-compose based, PG + server on brainframe | 2.1 | pending |
| R13 | Catch-up timer for missed signals (every 5 min) | 4.3 | pending |
| R14 | Caddy reverse proxy with auto-HTTPS via tailscale plugin | 3.1, 3.2 | pending |
| R15 | One-time brainframe setup automation | 5.1 | pending |
| R16 | Deploy scripts owned by root, not modifiable by deploy user | 5.1 | pending |
| R17 | SSH key command-restricted + no-port-forwarding + no-pty | 4.4, 5.1 | pending |
| R18 | No orphaned containers -- cleanup on any exit path | 1.2 | pending |
| R19 | flock-based deploy locking | 1.1 | pending |
| R20 | Post-deploy production health probe | 1.1 | pending |
| R21 | Deploy log per deployment | 1.1 | pending |
| R22 | Tarball retention: keep last 5 | 1.2 | pending |

---

## Risk Register

| # | Risk | Likelihood | Impact | Mitigation | Source |
|---|------|-----------|--------|------------|--------|
| RK1 | Smoke test OpenAI key accidentally same as production key | Medium | High | smoke-test.sh validates keys differ before starting; separate env files with 0600 perms | Security review |
| RK2 | Blue-green switch fails mid-way: new container up but Caddy not switched | Low | Medium | blue-green-switch.sh rolls back: stops new container on Caddy failure; active-color file not updated until success | Devil's advocate |
| RK3 | Smoke test embedding polling times out (OpenAI latency spikes) | Medium | Low | 3-minute timeout; tarball captures state; deploy can be re-triggered; not a production outage | Design review |
| RK4 | systemd path unit misses inotify signal | Low | Low | Catch-up timer every 5 minutes compares deployed commit vs origin/main HEAD | Design doc |
| RK5 | SSH signal fails (network partition, key rotation) | Medium | Low | Signal failure never blocks push; catch-up timer compensates within 5 minutes | Design doc |
| RK6 | Caddy admin API exposed beyond localhost | Low | High | Bound to 127.0.0.1:2019 with origins restriction; setup script validates; no external port mapping | Security review |
| RK7 | flock file descriptor held by zombie process | Very Low | Medium | flock auto-releases on process exit (kernel guarantee); systemd TimeoutStartSec kills stuck deploys | Operational |
| RK8 | Supabase pg_dump schema incompatible with local migrations | Medium | High | --data-only flag (no schema in dump); pre-migration backup; dry-run mode; row count verification | Devil's advocate |
| RK9 | Docker image build fails (registry down, OOM, disk full) | Low | Low | Deploy aborts cleanly before smoke test; Slack failure notification; manual re-trigger | Operational |
| RK10 | Failure tarball disk usage grows unbounded | Low | Low | Keep-last-5 policy enforced by smoke-test.sh; dedicated `/var/log/memlayer/failures/` directory | Design doc |
| RK11 | memlayer-deploy user modifies deploy scripts | Low | High | Scripts owned by root:root 0755; deploy user has nologin shell; only docker group membership | Security review |
| RK12 | DB schema migration during blue-green window causes query errors | Medium | Medium | Server migrator supports read-only mode for schema-ahead; window is brief (seconds); for breaking changes, design doc says stop old first | Research findings |
| RK13 | Caddy tailscale plugin not available or incompatible | Low | High | Setup script checks for caddy binary with tailscale module; documents xcaddy build command; can fall back to plain HTTPS with manual cert | Research |
| RK14 | /var/run/memlayer lost on reboot (tmpfs) | Medium | Low | tmpfiles.d config recreates directory structure on boot; setup script installs the config | Operational |

---

## Cross-Task Dependencies (Visual)

```
Wave 1 (parallel):     1.1  1.2  1.3
                         \    |    /
Wave 2 (parallel):      2.1  2.2
                          |    |
Wave 3 (sequential):    3.1 -> 3.2
                               |
Wave 4 (parallel):     4.1  4.2  4.3  4.4
                         \    |    |    /
Wave 5:                5.1 (needs W4) -> 5.2
                        |
                       5.3 (independent)
                        |
Wave 6:               6.1 (needs 2.1) -> 6.2 (needs all)
```

---

## Reuse Map

| Existing File | Used In | What To Extract |
|--------------|---------|-----------------|
| `tests/e2e.sh` lines 44-80 | Task 1.2 | Ingest payload structure: 3 entries with session_id, timestamps, realistic content |
| `tests/e2e-ollama.sh` lines 85-102 | Task 1.2 | Embedding status polling loop: poll, check count, sleep, timeout pattern |
| `scripts/lib.sh` lines 179-194 | Tasks 1.1, 1.2, 1.3 | `wait_for_url()` function for health-gate polling |
| `scripts/lib.sh` lines 6-26 | All deploy scripts | `info()`, `success()`, `warn()`, `error()`, `step()` formatting |
| `scripts/lib.sh` lines 75-78 | Task 1.2 | `generate_secret()` for randomized smoke DB password |
| `docker-compose.test.yml` | Task 2.2 | Compose structure: isolated network, PG healthcheck, test-mode env vars |
| `docker-compose.yml` | Task 2.1 | Production PG config, server env vars, volume mounts, healthchecks |
| `deploy/Dockerfile.cloud` | All (image builds) | Existing Dockerfile for server; used as-is for `docker build` |
| `scripts/memlayer-daemon.service.template` | Tasks 4.1-4.3 | systemd unit structure patterns |

---

## Key Design Decisions to Preserve

1. **Atomic Caddy switch:** Always POST full JSON config to `/load` endpoint. Never PATCH individual routes. Prevents partial config states.

2. **flock, not PID files:** `flock /var/run/memlayer/deploy.lock` uses kernel file descriptor locking. Auto-released on process exit, even on SIGKILL. No stale lockfile cleanup needed.

3. **Image tag convention:** `memlayer-server:<version>-<deploy-id>` where deploy-id is `YYYYMMDDHHMMSS`. Multiple images coexist for rollback. Old images pruned manually.

4. **Smoke test isolation:** Separate compose project name, separate network, separate ports (8499), separate DB password, separate OpenAI key. Smoke test cannot affect production.

5. **Signal is best-effort:** Git wrapper SSH call has 5-second timeout. Failure is stderr warning, never affects push exit code. Catch-up timer provides eventual consistency within 5 minutes.

6. **Scripts root-owned:** Deploy scripts at `/opt/memlayer/repo/deploy/` are root:root 0755. The `memlayer-deploy` user can read and execute them but not modify them. This prevents a compromised deploy user from injecting code into the pipeline.

7. **tmpfiles.d for /var/run:** Since `/var/run` is tmpfs on modern Linux, the `/etc/tmpfiles.d/memlayer.conf` ensures directories are recreated on every boot.

---

## Testing Strategy Per Wave

| Wave | Validation Method |
|------|-------------------|
| 1 | `shellcheck deploy/brainframe-deploy.sh deploy/smoke-test.sh deploy/blue-green-switch.sh` passes; `--dry-run` on each script produces expected trace output; manual review of control flow |
| 2 | `MEMLAYER_IMAGE=test POSTGRES_PASSWORD=test POSTGRES_USER=memlayer docker compose -f deploy/docker-compose.prod.yml config` validates; same for smoke compose with its env vars; optionally `docker compose -f deploy/docker-compose.smoke.yml up` locally |
| 3 | `shellcheck deploy/caddy-switch.sh` passes; `--dry-run` prints valid JSON (verify with `python3 -m json.tool`); optionally test against a local Caddy container: `docker run -d -p 2019:2019 caddy:latest` |
| 4 | `systemd-analyze verify deploy/systemd/*.service deploy/systemd/*.path deploy/systemd/*.timer` if available; `shellcheck deploy/memlayer-signal.sh` passes; manual review of unit dependencies |
| 5 | Read through setup script for correctness; test on a disposable VM or container (`docker run -it ubuntu:22.04 bash`); verify idempotency by running twice; `shellcheck deploy/setup-brainframe.sh deploy/git-wrapper-snippet.sh` |
| 6 | `--dry-run` migration against Supabase (dump but no restore); verify dump file structure; test restore against a fresh local PG; verify row counts; checklist review |

---

## Resumption State

**Current state:** Planning complete. No implementation started. All six waves are NOT STARTED.

**To resume:**
1. Read this PLAN.md
2. Find the first wave marked IN PROGRESS or NOT STARTED
3. Within that wave, find the first unchecked task
4. Complete the task, verify acceptance criteria
5. Mark task complete with date: `- [x] Task N.M: ... -- completed YYYY-MM-DD`
6. After completing all tasks in a wave, update wave status and resumption state
7. Record any deviations in the Deviations Log
8. Update Requirement Traceability status column

**Key files for context when resuming:**
- Design: `/home/mikey/memlayer/.planning/DESIGN.md`
- Existing compose: `/home/mikey/memlayer/docker-compose.yml` and `/home/mikey/memlayer/docker-compose.test.yml`
- Existing e2e tests: `/home/mikey/memlayer/tests/e2e.sh` and `/home/mikey/memlayer/tests/e2e-ollama.sh`
- Shared lib: `/home/mikey/memlayer/scripts/lib.sh`
- Server Dockerfile: `/home/mikey/memlayer/deploy/Dockerfile.cloud`
- Server config: `/home/mikey/memlayer/server/src/config.py`
