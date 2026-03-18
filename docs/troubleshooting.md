# Troubleshooting

## Server not starting

**Symptom:** `docker compose up -d` runs but `curl http://localhost:8420/health` fails.

```bash
docker compose ps
docker compose logs server
docker compose logs db
```

**Causes:**
- **Port 8420 in use.** Check with `lsof -i :8420` or `ss -tlnp | grep 8420`.
- **Database not ready.** Restart: `docker compose restart server`.
- **Missing `.env` file.** Copy `.env.example` to `.env` and fill in `POSTGRES_PASSWORD` and `MEMLAYER_AUTH_TOKEN`.

## Daemon not connecting to server

**Symptom:** Daemon starts but logs show repeated connection errors.

```bash
# Linux
systemctl --user status claude-mem-daemon
journalctl --user -u claude-mem-daemon -f

# macOS
launchctl list | grep memlayer
cat ~/Library/Logs/claude-mem-daemon.log
```

**Causes:**
- **Wrong `MEMLAYER_SERVER_URL`.** Must include `/api` (e.g., `http://localhost:8420/api`).
- **Server on a different machine.** Replace `localhost` with the actual IP or Tailscale hostname.
- **Firewall blocking port 8420.**
- **Auth token mismatch.** Compare `~/.config/memlayer/env` on the client with `.env` on the server.

The daemon retries with exponential backoff (up to 5 minutes) and queues entries locally in SQLite. No data is lost while the server is down.

## CLI not working

**Symptom:** Claude doesn't call `memlayer search` when you reference past conversations.

```bash
# Check CLI is installed
which memlayer
memlayer status

# If missing, rebuild and reinstall:
cd memlayer/cli && npm install && npx tsc
ln -sf "$(pwd)/dist/cli.js" ~/.local/bin/memlayer
```

**Causes:**
- **CLI not installed.** Run `setup_client.sh` again or install manually.
- **CLI build failed.** Check for TypeScript errors: `cd cli && npx tsc`.
- **`~/.local/bin` not in PATH.** Add `export PATH="$HOME/.local/bin:$PATH"` to your shell profile.
- **Node.js not in PATH.** Verify with `node --version` (requires 18+).
- **Missing CLAUDE.md instructions.** Check `~/.claude/CLAUDE.md` for a "Memory (Cross-Session Recall)" section. Re-run `setup_client.sh` if missing.

## Search returning no results

**Symptom:** `memlayer search` returns "No matching memories found" for queries you know should match.

```bash
# Check entry count
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*) FROM memory_entries"

# Check time range
docker compose exec db psql -U memlayer -d memlayer \
  -c "SELECT COUNT(*), MIN(created_at), MAX(created_at) FROM memory_entries"

# Check daemon status
systemctl --user status claude-mem-daemon
```

**Causes:**
- **Daemon not running.** Start it: `systemctl --user start claude-mem-daemon`.
- **Initial ingestion still in progress.** Check daemon logs for progress.
- **Query too specific.** Try shorter, simpler queries with keywords.
- **Wrong project filter.** Remove `project_path` to search across all projects.

## Embeddings not generating

**Symptom:** `/health` shows `"embeddings": "disabled (FTS-only)"`.

```bash
curl http://localhost:8420/health | python3 -m json.tool
docker compose logs server | grep -i embed
```

**Causes:**
- **No `OPENAI_API_KEY` set.** Add to `.env` and restart: `docker compose restart server`.
- **Invalid API key.** Test: `curl https://api.openai.com/v1/models -H "Authorization: Bearer sk-..."`.
- **Ollama not running.** If `EMBEDDING_PROVIDER=ollama`, ensure Ollama is accessible at `OLLAMA_BASE_URL`.

Full-text search works well on its own. Embeddings add semantic understanding but are not required.

## "Invalid auth token" errors

**Symptom:** API requests return HTTP 401.

```bash
# Check server token
grep MEMLAYER_AUTH_TOKEN /path/to/memlayer/.env

# Check client token
cat ~/.config/memlayer/env

# Test manually
curl -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":1}' \
  http://localhost:8420/api/search
```

**Causes:**
- **Token mismatch.** Must be identical on server (`.env`) and client (`~/.config/memlayer/env`).
- **Token regenerated but not updated on client.** Re-run `setup_client.sh` or update manually.
- **Extra whitespace or newline in the token.**

## Database connection issues

**Symptom:** Server logs show "connection refused" for PostgreSQL.

```bash
docker compose ps db
docker compose logs db
df -h
```

**Causes:**
- **Database container not running.** Start it: `docker compose up -d db`.
- **Database still initializing.** Wait for "database system is ready to accept connections" in logs.
- **Disk full.** Free up space and restart: `docker compose restart db`.
- **Corrupted data volume.** Last resort (deletes all data): `docker compose down -v && docker compose up -d`.

## Backup and restore issues

**Symptom:** `memlayer-server backup` or `memlayer-server restore` fails.

```bash
docker compose ps
ls -la ~/.local/share/memlayer/backups/
tar -tzf /path/to/backup.tar.gz | head
```

**Causes:**
- **Docker containers not running.** Both backup and restore need the database container.
- **Insufficient disk space.**
- **Backup file corrupted or truncated.**

## Large response files not loading

**Symptom:** `memlayer read-file` returns "File not found" for a recently returned file ID.

```bash
docker compose exec server ls /data/response_files/
grep FILE_STORAGE .env
```

**Causes:**
- **File was evicted.** Increase `FILE_STORAGE_SOFT_LIMIT`/`FILE_STORAGE_HARD_LIMIT` or set to `0` for unlimited.
- **Client file cache is stale.** Clear it: `rm -rf ~/.claude/memlayer/cache/*`.

## Slow initial ingestion

**Symptom:** On first startup, the daemon takes a long time and uses noticeable CPU.

This is expected. The daemon scans all existing JSONL files on first run. If you have months of Claude Code history, initial ingestion can take several minutes. Subsequent runs are fast — the daemon tracks byte-offset cursors and only processes new data.

Monitor progress:

```bash
# Linux
journalctl --user -u claude-mem-daemon -f

# macOS
tail -f ~/Library/Logs/claude-mem-daemon.log
```
