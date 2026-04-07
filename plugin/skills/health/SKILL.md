---
name: memlayer:health
description: >-
  Use when the user wants to check if memlayer is working, diagnose connection
  issues, check server status, view embedding progress, troubleshoot the memory
  system, update the CLI or daemon, or rollback to a previous version. Trigger on
  "is memlayer running", "memory status", "check memlayer", "memlayer health",
  "server status", "embedding status", "version", "update memlayer", "upgrade",
  "rollback", "memlayer not working", "connection error", "troubleshoot memory".
version: 2.5.0
---

# Health - Memlayer Status and Diagnostics

Quick way to check if memlayer is healthy, diagnose issues, and manage updates.

## Commands

### memlayer status

Check server health, version compatibility, and embedding status. Shows client/server version, schema version, read-only mode, and any daemon version errors.

```bash
memlayer status
memlayer status --format text
```

If the daemon is blocked due to version incompatibility, `memlayer status` will show the error details and upgrade instructions.

### memlayer update

Check for and apply updates to the daemon and CLI.

```bash
memlayer update
```

### memlayer rollback

List or restore archived daemon binaries.

```bash
memlayer rollback --list
memlayer rollback
```

## Troubleshooting

### "memlayer: command not found"
Check that `~/.local/bin/memlayer` exists and is executable. Verify the symlink points to a valid binary. Ensure `~/.local/bin` is on `$PATH`.

### Connection refused / timeout
1. Check `MEMLAYER_SERVER_URL` is set correctly (default: `http://localhost:8420/api`)
2. Verify the server is running: `docker compose ps`
3. Check the auth token in `~/.config/memlayer/env`

### "version incompatible" / read-only mode
Run `memlayer status` to see version error details, then `memlayer update` to upgrade the CLI.

### No search results
- Check embedding status in `memlayer status` -- embeddings may still be processing
- Verify the daemon is running and ingesting new conversations
- Try `--all-types` in case results are in tool_use/tool_result entries

### Credentials not loading
Verify `~/.config/memlayer/env` exists and contains:
```
MEMLAYER_SERVER_URL=http://...
MEMLAYER_AUTH_TOKEN=...
```
