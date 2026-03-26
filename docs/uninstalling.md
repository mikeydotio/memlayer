# Uninstalling

```bash
# From a repo clone
./uninstall.sh

# From an install.sh installation
~/.memlayer/uninstall.sh
```

The uninstaller walks through each component and asks before removing anything:

1. **Background service** — Stops and removes the systemd service or launchd agent
2. **Daemon binary** — Removes `~/.local/bin/memlayer-daemon`
3. **CLI binary** — Removes `~/.local/bin/memlayer`
4. **CLAUDE.md section** — Removes the memory instructions from `~/.claude/CLAUDE.md`
5. **Local data** — Removes cursor database and offline queue (`~/.local/share/memlayer/`)
6. **Server and database** — Stops Docker containers and removes volumes (with data loss warning)
7. **Repository clone** — Removes `~/.memlayer/` (only for `install.sh` installations)
