# Server Migration Guide

## Overview

Memlayer server migration transfers all memory entries, response files, session metadata, and embeddings from one server instance to another. The process is designed for **zero data loss**: daemons queue entries locally during the transfer window and automatically redirect to the new server once migration completes. No manual reconfiguration of clients is required.

## When to Migrate

- Moving to new hardware or a different cloud provider
- Scaling up (larger VM, more storage, faster database)
- Changing network topology (e.g., moving behind Tailscale)
- Disaster recovery from a backup to a fresh server

## Prerequisites

- **Source server** is running and healthy (`/health` returns OK)
- **Destination server** is set up and running (via `setup_server.sh` or Docker Compose)
- Both servers can reach each other over the network (source must be reachable from destination)
- Both servers should run the same memlayer version to avoid schema mismatches
- If both servers use the same embedding provider and model, embeddings transfer directly; otherwise the destination regenerates them

## Migration via Web UI

1. Open the source server's dashboard (e.g., `http://source-server:8420`)
2. Navigate to the **Migration** tab
3. Click **Start Migration** — the server generates an Ed25519 keypair and displays a one-time migration key
4. Copy the migration key
5. On the **destination** server, run `setup_server.sh` and select option **2) Migrate from existing server** when prompted
6. Paste the source server URL and migration key
7. The dashboard shows real-time progress (entries transferred, files transferred, current state)
8. When the state shows **COMPLETE**, migration is finished

To cancel at any point, click **Cancel Migration** on the source server's Migration tab.

## Migration via CLI

### On the source server

```bash
memlayer migrate
```

This initiates the migration, generates a migration key, and begins polling for status. The key is displayed in a box — copy it. You can press Ctrl+C to stop polling; the migration continues on the server.

### On the destination server

```bash
./setup_server.sh
```

When prompted for installation type, select **2) Migrate from existing server**, then enter:
- The source server URL (e.g., `http://old-server:8420/api`)
- The migration key from the source

The script starts Docker services, waits for the server to become healthy, initiates the handshake with the source, and polls until the transfer completes.

### Cancelling via CLI

There is no `--cancel` flag. To cancel an active migration, use the API directly:

```bash
curl -X POST http://localhost:8420/api/migration/cancel \
  -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'
```

Or use the Cancel button in the web UI.

## What Happens During Migration

The migration follows a state machine with these stages:

1. **INITIATED** — Source generates an Ed25519 keypair and a one-time migration key (expires in 1 hour). The key is hashed (SHA-256) before storage; only the plaintext is shown to the admin.

2. **KEY_EXCHANGED** — Destination calls `/migration/verify-destination` on the source with the migration key, its own URL, and its embedding configuration. Source validates the key, compares embedding settings (provider, model, dimensions), records whether embeddings are compatible, counts total entries and files, and extends the key TTL to 24 hours.

3. **REDIRECTING** — Source starts returning HTTP 449 on the `/ingest` endpoint. The 449 response includes the destination URL and an Ed25519 signature. Daemons verify the signature against the public key they received earlier, then queue entries locally in their SQLite offline queue and begin sending new entries to the destination.

4. **TRANSFERRING** — Destination pulls entries and files from the source in paginated batches:
   - Entries: batches of 200, ordered by ID, with optional embeddings (base64-encoded float32 arrays) if embedding config matches
   - Files: batches of 10, including file content and metadata
   - Progress checkpoints are saved after each batch (entry ID watermark), enabling resume on failure
   - Duplicate entries are skipped via `ON CONFLICT (payload_hash) DO NOTHING`
   - Entries without transferred embeddings are enqueued for embedding generation on the destination

5. **VERIFYING** — Destination compares actual entry and file counts against expected totals from the source.

6. **COMPLETE** — Migration is done. Daemons that received the 449 redirect drain their queued entries to the destination, then call `/migration/client-provision` to get permanent credentials (server URL + auth token). The daemon automatically rewrites its config files:
   - `~/.config/memlayer/env` (systemd environment)
   - `~/.claude/settings.json` (MCP server configuration)

## Failure Recovery

### Source goes down during transfer

The destination retries HTTP requests with exponential backoff (up to 8 retries, max 60s backoff). Meanwhile, daemons queue entries locally in their SQLite offline queue — entries are never lost. Once the source comes back, the transfer resumes from the last checkpoint.

### Destination goes down during transfer

Restart the destination server. The transfer worker resumes from the last saved entry ID checkpoint (`last_transferred_entry_id`). No entries are lost because the source still has all data.

### Migration key expires

The initial key TTL is 1 hour. After a successful handshake (KEY_EXCHANGED), the TTL extends to 24 hours. If the key expires before handshake:

1. Cancel the migration on the source (web UI or API)
2. Re-initiate with `memlayer migrate` or the web UI to get a fresh key

### Transfer fails (FAILED state)

The migration transitions to FAILED with an error message. Check the server logs (`docker compose logs server`) for details. Cancel the failed migration and re-initiate:

```bash
# Cancel the failed migration
curl -X POST http://localhost:8420/api/migration/cancel \
  -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{}'

# Re-initiate
memlayer migrate
```

### Daemon cannot reach either server

The daemon's SQLite offline queue holds entries indefinitely. When connectivity returns, entries drain automatically. No data is lost.

## Verifying Completion

After migration completes:

1. **Check entry counts** — Compare source and destination:
   ```bash
   # On source
   curl -s http://source:8420/health | python3 -m json.tool

   # On destination
   curl -s http://destination:8420/health | python3 -m json.tool
   ```

2. **Test search** — Run a search query against the destination to confirm entries are indexed:
   ```bash
   curl -s "http://destination:8420/api/search?q=test&limit=5" \
     -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" | python3 -m json.tool
   ```

3. **Verify daemon config** — Check that daemon config files point to the new server:
   ```bash
   grep MEMLAYER_SERVER_URL ~/.config/memlayer/env
   ```

4. **Check migration status** — Confirm the migration shows COMPLETE:
   ```bash
   curl -s http://destination:8420/api/migration/status \
     -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" | python3 -m json.tool
   ```

## Decommissioning the Old Server

Once migration is COMPLETE and you have verified the destination:

1. Confirm all daemons have drained their queues and are sending to the new server
2. Stop the old server:
   ```bash
   docker compose down
   ```
3. Optionally take a final backup before removing the old server's data:
   ```bash
   memlayer backup
   ```
4. The old server's database volume can be removed once you are confident the new server is operating correctly
