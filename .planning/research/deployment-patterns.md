# Domain Research: Deployment Patterns

## Signal Mechanism

The shared-volume signal pattern from deploy-web will NOT work for remote containers. The container and brainframe are separate machines on Tailscale.

**Recommended: SSH + touch signal file**
- Container runs: `ssh brainframe "touch /var/run/memlayer/signals/deploy"`
- brainframe has a systemd `.path` unit watching that file
- Simplest approach, fewest moving parts

**Alternative: HTTP webhook via adnanh/webhook or Tailscale Serve**

systemd path units use inotify (`PathChanged=`) to trigger a `.service` unit. Known limitation: does not fire retroactively if signal written while unit was down. Mitigation: make deploy script idempotent.

## Blue-Green Docker Compose

Architecture:
```
    Tailscale network
         |
    [Caddy or nginx]
    /            \
[memlayer-blue]  [memlayer-green]
  port 8430         port 8431
    \            /
    [memlayer-db]
    (shared, external network)
```

- Database runs independently on an external Docker network
- Blue and green each get their own server container on different ports
- Proxy switches between them
- Both share the same database

Caddy is the strongest fit: admin API for programmatic switching, `tailscale/caddy-tailscale` plugin for auto-HTTPS on tailnet, atomic config changes.

Memlayer's migrator handles the hard part: read-only mode when schema is ahead, so old server degrades gracefully during transition.

## Ephemeral Test Environments

Project already has: `docker-compose.test.yml`, `tests/e2e.sh`, `wait_for_url()` utility.

Best practices:
- Unique project name per run: `docker compose -p "memlayer-smoke-$(date +%s)"`
- tmpfs for Postgres: `fsync=off`, `synchronous_commit=off` for ~2s startup
- `trap cleanup EXIT` for guaranteed teardown
- Seed via ingest API (existing e2e pattern)

## Embedding Status Polling

`/health` returns `embedding_progress.pending` count. Existing pattern in `e2e-ollama.sh`: fixed-interval poll (3-5s) with timeout. For smoke tests, seed 2-3 entries and poll until `pending == 0`.
