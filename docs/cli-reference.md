# CLI Reference

## Search CLI (`memlayer`)

The `memlayer` CLI searches and recalls past Claude Code conversations. It reads configuration from environment variables or `~/.config/memlayer/env`.

### memlayer search

Search across all past conversations.

```bash
memlayer search "how did we fix the pooling bug" --project /home/mikey/memlayer --limit 5
```

See [CLI Tools & Search](cli-tools.md) for full flag reference and search filter examples.

### memlayer session

Retrieve full conversation history for a session.

```bash
memlayer session <session-uuid> --limit 200 --types user,assistant
```

### memlayer read-file

Read a line range from a large response file.

```bash
memlayer read-file <file-uuid> --start 1 --end 50
```

### memlayer status

Show server health and embedding status.

```bash
memlayer status
```

---

## Server Admin CLI (`memlayer-server`)

The `memlayer-server` CLI manages the memory database. All commands connect to the local Docker stack.

### memlayer-server status

Show the current state of all memlayer components.

```bash
memlayer-server status
```

Example output:

```
Memlayer Status
===============
Server:       http://localhost:8420 (healthy)
Database:     ok (6,243 entries)
Daemon:       running (systemd), PID 12345
```

### memlayer-server backup

Create a backup of the entire memory database and response files.

```bash
# Backup to the default location
memlayer-server backup

# Backup to a specific file
memlayer-server backup /path/to/backup.tar.gz
```

### memlayer-server restore

Restore a backup to the database and response file storage.

```bash
memlayer-server restore /path/to/backup.tar.gz
```

### memlayer-server forget

Permanently delete conversations from the database.

```bash
# Forget a specific session
memlayer-server forget --session 550e8400-e29b-41d4-a716-446655440000

# Forget all conversations for a project
memlayer-server forget --project /home/user/my-project
```

### memlayer-server verify

Check database integrity and report issues.

```bash
memlayer-server verify
```

### memlayer-server setup

Run the interactive setup wizard.

```bash
memlayer-server setup server
memlayer-server setup client
```

### memlayer-server migrate

Move your memlayer instance to a new server with zero data loss.

```bash
memlayer-server migrate
```

See [migration.md](migration.md) for the full guide.
