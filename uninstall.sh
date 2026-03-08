#!/usr/bin/env bash
# Memlayer uninstaller — interactive, asks before each removal
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/scripts/lib.sh"

TOTAL_STEPS=7

echo
echo "  Memlayer Uninstall"
echo "  ==================="
echo

# ── Step 1: Stop and remove background service ─────────────────────
step 1 $TOTAL_STEPS "Background service"

_os=$(detect_os)
service_removed=false

if [[ "$_os" == "linux" ]]; then
    service_file="$HOME/.config/systemd/user/claude-mem-daemon.service"
    if systemctl --user is-enabled claude-mem-daemon &>/dev/null 2>&1 || [[ -f "$service_file" ]]; then
        if confirm "Stop and remove systemd service?" "y"; then
            systemctl --user stop claude-mem-daemon 2>/dev/null || true
            systemctl --user disable claude-mem-daemon 2>/dev/null || true
            rm -f "$service_file"
            systemctl --user daemon-reload
            success "systemd service removed"
            service_removed=true
        else
            info "Skipping service removal"
        fi
    else
        info "No systemd service found"
    fi
elif [[ "$_os" == "macos" ]]; then
    plist_file="$HOME/Library/LaunchAgents/io.memlayer.daemon.plist"
    if [[ -f "$plist_file" ]] || launchctl list 2>/dev/null | grep -q io.memlayer.daemon; then
        if confirm "Stop and remove launch agent?" "y"; then
            launchctl unload "$plist_file" 2>/dev/null || true
            rm -f "$plist_file"
            success "Launch agent removed"
            service_removed=true
        else
            info "Skipping launch agent removal"
        fi
    else
        info "No launch agent found"
    fi
fi

# ── Step 2: Remove daemon binary ───────────────────────────────────
step 2 $TOTAL_STEPS "Daemon binary"

daemon_bin="$HOME/.local/bin/claude-mem-daemon"
if [[ -f "$daemon_bin" ]]; then
    if confirm "Remove daemon binary ($daemon_bin)?" "y"; then
        rm -f "$daemon_bin"
        success "Daemon binary removed"
    else
        info "Keeping daemon binary"
    fi
else
    info "No daemon binary found at $daemon_bin"
fi

# ── Step 3: Remove MCP registration ────────────────────────────────
step 3 $TOTAL_STEPS "MCP registration"

if command -v claude &>/dev/null; then
    if claude mcp list 2>/dev/null | grep -q claude-memory; then
        if confirm "Remove claude-memory MCP registration?" "y"; then
            claude mcp remove claude-memory --scope user 2>/dev/null || true
            success "MCP registration removed"
        else
            info "Keeping MCP registration"
        fi
    else
        info "No claude-memory MCP registration found"
    fi
else
    info "Claude CLI not found — skipping MCP check"
fi

# ── Step 4: Remove CLAUDE.md memory section ────────────────────────
step 4 $TOTAL_STEPS "CLAUDE.md memory section"

claudemd="$HOME/.claude/CLAUDE.md"
if [[ -f "$claudemd" ]] && grep -q '## Memory (Cross-Session Recall)' "$claudemd"; then
    if confirm "Remove memory section from ~/.claude/CLAUDE.md?" "y"; then
        awk '
            /^## Memory \(Cross-Session Recall\)/ { skip=1; next }
            /^## / && skip { skip=0 }
            !skip { print }
        ' "$claudemd" > "${claudemd}.tmp"
        mv "${claudemd}.tmp" "$claudemd"

        # Remove file if empty (only whitespace remaining)
        if [[ ! -s "$claudemd" ]] || ! grep -q '[^[:space:]]' "$claudemd"; then
            rm -f "$claudemd"
            success "Memory section removed (CLAUDE.md was empty, deleted)"
        else
            success "Memory section removed from CLAUDE.md"
        fi
    else
        info "Keeping memory section in CLAUDE.md"
    fi
else
    info "No memory section found in CLAUDE.md"
fi

# ── Step 5: Remove local data ─────────────────────────────────────
step 5 $TOTAL_STEPS "Local daemon data"

data_dir="$HOME/.local/share/memlayer"
if [[ -d "$data_dir" ]]; then
    if confirm "Remove local daemon data (cursors, offline queue)?" "n"; then
        rm -rf "$data_dir"
        success "Local data removed"
    else
        info "Keeping local data at $data_dir"
    fi
else
    info "No local data directory found"
fi

# ── Step 6: Remove server (if co-located) ──────────────────────────
step 6 $TOTAL_STEPS "Server and database"

if [[ -f "$SCRIPT_DIR/docker-compose.yml" ]]; then
    running=$(docker compose -f "$SCRIPT_DIR/docker-compose.yml" ps --format '{{.Name}}' 2>/dev/null | grep -c memlayer || true)
    if (( running > 0 )) || [[ -f "$SCRIPT_DIR/.env" ]]; then
        warn "WARNING: This will delete ALL indexed conversations from the database."
        if confirm "Remove server and database?" "n"; then
            (cd "$SCRIPT_DIR" && docker compose down -v 2>/dev/null || true)
            rm -f "$SCRIPT_DIR/.env"
            success "Server containers and database removed"
        else
            info "Keeping server and database"
        fi
    else
        info "No running server found"
    fi
else
    info "No docker-compose.yml found — skipping server removal"
fi

# ── Step 7: Remove repo clone (if installed via install.sh) ────────
step 7 $TOTAL_STEPS "Repository clone"

if [[ "$SCRIPT_DIR" == "$HOME/.memlayer" ]]; then
    if confirm "Remove $HOME/.memlayer?" "n"; then
        # We're running from inside this directory, so defer deletion
        echo "  Will remove after script exits."
        trap 'rm -rf "$HOME/.memlayer"' EXIT
        success "Repository will be removed"
    else
        info "Keeping $HOME/.memlayer"
    fi
else
    info "Not installed via install.sh — skipping repo removal"
fi

# ── Summary ─────────────────────────────────────────────────────────
echo
success "Uninstall complete."
echo
