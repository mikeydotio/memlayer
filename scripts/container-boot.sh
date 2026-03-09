#!/usr/bin/env bash
# Memlayer container boot script — non-interactive, idempotent
# Run on container launch to install/update the memlayer client.
#
# Required env vars (set in container environment or pass as args):
#   MEMLAYER_SERVER_URL  — e.g. http://100.x.y.z:8420/api
#   MEMLAYER_AUTH_TOKEN  — shared bearer token
#
# Usage:
#   # With env vars already set:
#   bash /path/to/container-boot.sh
#
#   # Or pass as args:
#   bash /path/to/container-boot.sh --server-url http://... --auth-token ...
#
# What this does:
#   1. Clones/updates the memlayer repo to ~/.memlayer
#   2. Builds the daemon from source (or downloads binary)
#   3. Builds the MCP server (npm install + tsc)
#   4. Registers MCP tools with Claude Code
#   5. Injects memory instructions into ~/.claude/CLAUDE.md
#   6. Starts the daemon as a background process
#
# Safe to run on every boot — all steps are idempotent.

set -euo pipefail

INSTALL_DIR="$HOME/.memlayer"
DAEMON_BIN="$HOME/.local/bin/claude-mem-daemon"
REPO_URL="https://github.com/mikeydotio/memlayer.git"
TARBALL_URL="https://github.com/mikeydotio/memlayer/archive/refs/heads/main.tar.gz"
RELEASE_BASE="https://github.com/mikeydotio/memlayer/releases/latest/download"
LOG_PREFIX="[memlayer-boot]"

log()  { echo "$LOG_PREFIX $*"; }
err()  { echo "$LOG_PREFIX ERROR: $*" >&2; }

# ── Parse args (override env vars) ─────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --server-url)  MEMLAYER_SERVER_URL="$2"; shift 2 ;;
        --auth-token)  MEMLAYER_AUTH_TOKEN="$2"; shift 2 ;;
        *)             shift ;;
    esac
done

# Validate required vars
if [[ -z "${MEMLAYER_SERVER_URL:-}" ]]; then
    err "MEMLAYER_SERVER_URL is required (env var or --server-url)"
    exit 1
fi
if [[ -z "${MEMLAYER_AUTH_TOKEN:-}" ]]; then
    err "MEMLAYER_AUTH_TOKEN is required (env var or --auth-token)"
    exit 1
fi

export MEMLAYER_SERVER_URL MEMLAYER_AUTH_TOKEN

# ── Step 1: Clone or update repo ──────────────────────────────────
log "Updating memlayer installation..."

if [[ -d "$INSTALL_DIR/.git" ]] && command -v git &>/dev/null; then
    (cd "$INSTALL_DIR" && git pull --ff-only -q 2>/dev/null) || true
elif [[ -d "$INSTALL_DIR" ]]; then
    # Tarball update
    rm -rf "$INSTALL_DIR"
    mkdir -p "$INSTALL_DIR"
    curl -fsSL "$TARBALL_URL" | tar -xz --strip-components=1 -C "$INSTALL_DIR"
else
    # Fresh install
    if command -v git &>/dev/null; then
        git clone -q "$REPO_URL" "$INSTALL_DIR"
    else
        mkdir -p "$INSTALL_DIR"
        curl -fsSL "$TARBALL_URL" | tar -xz --strip-components=1 -C "$INSTALL_DIR"
    fi
fi

if [[ ! -f "$INSTALL_DIR/setup_client.sh" ]]; then
    err "Installation incomplete — setup_client.sh not found"
    exit 1
fi

log "Repository ready at $INSTALL_DIR"

# ── Step 2: Install daemon binary ─────────────────────────────────
mkdir -p "$HOME/.local/bin"

_need_daemon=false
if [[ -f "$DAEMON_BIN" ]]; then
    # Check if update available (compare versions)
    current=$("$DAEMON_BIN" --version 2>/dev/null | awk '{print $2}' || echo "0.0.0")
    if [[ -f "$INSTALL_DIR/daemon/Cargo.toml" ]]; then
        latest=$(grep '^version' "$INSTALL_DIR/daemon/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
        if [[ "$current" != "$latest" ]]; then
            log "Daemon update available: $current -> $latest"
            _need_daemon=true
        else
            log "Daemon up to date ($current)"
        fi
    fi
else
    _need_daemon=true
fi

if [[ "$_need_daemon" == "true" ]]; then
    _installed=false

    # Try pre-built binary first
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    [[ "$os" == "darwin" ]] && os="macos"
    arch=$(uname -m)
    [[ "$arch" == "amd64" ]] && arch="x86_64"
    [[ "$arch" == "arm64" ]] && arch="aarch64"
    tarball="claude-mem-daemon-${os}-${arch}.tar.gz"

    if curl -fsSL --max-time 30 -o "/tmp/$tarball" "${RELEASE_BASE}/${tarball}" 2>/dev/null; then
        tar -xzf "/tmp/$tarball" -C "$HOME/.local/bin/"
        chmod +x "$DAEMON_BIN"
        rm -f "/tmp/$tarball"
        log "Daemon installed from pre-built binary"
        _installed=true
    fi

    # Fall back to source build
    if [[ "$_installed" != "true" ]] && command -v cargo &>/dev/null; then
        log "Building daemon from source..."
        (cd "$INSTALL_DIR/daemon" && cargo build --release -q 2>/dev/null)
        cp "$INSTALL_DIR/daemon/target/release/claude-mem-daemon" "$DAEMON_BIN"
        chmod +x "$DAEMON_BIN"
        log "Daemon built from source"
        _installed=true
    fi

    if [[ "$_installed" != "true" ]]; then
        err "Could not install daemon (no binary available, no Rust toolchain)"
        exit 1
    fi
fi

# ── Step 3: Build MCP server ──────────────────────────────────────
mcp_dir="$INSTALL_DIR/mcp"
if [[ -f "$mcp_dir/package.json" ]] && command -v node &>/dev/null; then
    # Only rebuild if source changed
    if [[ ! -f "$mcp_dir/dist/index.js" ]] || \
       [[ "$mcp_dir/src/index.ts" -nt "$mcp_dir/dist/index.js" ]] || \
       [[ "$mcp_dir/src/api-client.ts" -nt "$mcp_dir/dist/index.js" ]]; then
        log "Building MCP server..."
        (cd "$mcp_dir" && npm install --no-audit --no-fund -q 2>/dev/null && npx tsc 2>/dev/null)
        log "MCP server built"
    else
        log "MCP server up to date"
    fi
else
    err "Node.js not found — MCP tools will not be available"
fi

# ── Step 4: Register MCP tools with Claude Code ───────────────────
if command -v claude &>/dev/null && [[ -f "$mcp_dir/dist/index.js" ]]; then
    # Remove existing registration (idempotent update)
    claude mcp remove claude-memory --scope user 2>/dev/null || true

    claude mcp add claude-memory --scope user \
        -e MEMLAYER_SERVER_URL="$MEMLAYER_SERVER_URL" \
        -e MEMLAYER_AUTH_TOKEN="$MEMLAYER_AUTH_TOKEN" \
        -- node "$mcp_dir/dist/index.js" 2>/dev/null

    if claude mcp list 2>/dev/null | grep -q claude-memory; then
        log "MCP tools registered"
    else
        err "MCP registration may have failed"
    fi
else
    log "Skipping MCP registration (claude CLI not found)"
fi

# ── Step 5: Inject memory instructions into CLAUDE.md ─────────────
claudemd="$HOME/.claude/CLAUDE.md"
template="$INSTALL_DIR/scripts/claude-memory.claudemd.template"
mkdir -p "$HOME/.claude"

if [[ -f "$template" ]]; then
    if [[ -f "$claudemd" ]] && grep -q '## Memory (Cross-Session Recall)' "$claudemd"; then
        # Replace existing section
        awk '
            /^## Memory \(Cross-Session Recall\)/ { skip=1; next }
            /^## / && skip { skip=0 }
            !skip { print }
        ' "$claudemd" > "${claudemd}.tmp"
        mv "${claudemd}.tmp" "$claudemd"
    fi

    [[ ! -f "$claudemd" ]] && touch "$claudemd"
    [[ -s "$claudemd" ]] && [[ "$(tail -c1 "$claudemd")" != "" ]] && echo >> "$claudemd"
    echo >> "$claudemd"
    cat "$template" >> "$claudemd"
    log "CLAUDE.md updated with memory instructions"
fi

# ── Step 6: Start daemon (background process) ─────────────────────
# Kill any existing daemon
if pgrep -f claude-mem-daemon &>/dev/null; then
    pkill -f claude-mem-daemon 2>/dev/null || true
    sleep 1
    log "Stopped existing daemon"
fi

# Start in background with nohup
nohup "$DAEMON_BIN" >> "$HOME/.local/share/memlayer/daemon.log" 2>&1 &
daemon_pid=$!

# Verify it started
sleep 2
if kill -0 "$daemon_pid" 2>/dev/null; then
    log "Daemon started (PID $daemon_pid)"
else
    err "Daemon failed to start — check $HOME/.local/share/memlayer/daemon.log"
    exit 1
fi

# ── Done ──────────────────────────────────────────────────────────
log "Memlayer client ready"
log "  Server:  $MEMLAYER_SERVER_URL"
log "  Daemon:  PID $daemon_pid"
log "  MCP:     $(claude mcp list 2>/dev/null | grep -c claude-memory || echo 0) tool(s) registered"
