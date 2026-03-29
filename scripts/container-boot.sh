#!/usr/bin/env bash
# Memlayer container boot script — non-interactive, idempotent
# Run on container launch to install/update the memlayer client.
#
# Configuration (checked in priority order):
#   1. CLI args:       --server-url ... --auth-token ...
#   2. Config file:    /etc/memlayer/client.conf (bind-mount from host)
#   3. User config:    ~/.config/memlayer/client.conf
#   4. Env vars:       MEMLAYER_SERVER_URL, MEMLAYER_AUTH_TOKEN
#
# Recommended for container orchestration:
#   Mount your config file read-only into the container:
#     -v /path/to/memlayer-client.conf:/etc/memlayer/client.conf:ro
#
# What this does:
#   1. Clones/updates the memlayer repo to ~/.memlayer
#   2. Builds the daemon from source (or downloads binary)
#   3. Builds the CLI (npm install + tsc)
#   4. Installs CLI binary to ~/.local/bin/memlayer
#   5. Injects memory instructions into ~/.claude/CLAUDE.md
#   6. Installs Claude Code plugin
#   7. Writes ~/.config/memlayer/env for daemon and CLI
#   8. Starts the daemon as a background process
#
# Safe to run on every boot — all steps are idempotent.

set -euo pipefail

INSTALL_DIR="$HOME/.memlayer"
DAEMON_BIN="$HOME/.local/bin/memlayer-daemon"
REPO_URL="https://github.com/mikeydotio/memlayer.git"
TARBALL_URL="https://github.com/mikeydotio/memlayer/archive/refs/heads/main.tar.gz"
RELEASE_BASE="https://github.com/mikeydotio/memlayer/releases/latest/download"
LOG_PREFIX="[memlayer-boot]"

# Config file search paths (first match wins)
CONF_PATHS=(
    "/etc/memlayer/client.conf"
    "$HOME/.config/memlayer/client.conf"
)

log()  { echo "$LOG_PREFIX $*"; }
err()  { echo "$LOG_PREFIX ERROR: $*" >&2; }

# ── Load config file (lowest priority — env vars and args override) ─
_conf_source=""
for _conf in "${CONF_PATHS[@]}"; do
    if [[ -f "$_conf" ]]; then
        # Source only lines that look like KEY=value (skip comments, blank lines)
        while IFS='=' read -r key value; do
            key="${key%%#*}"        # strip inline comments
            key="${key// /}"        # strip spaces
            [[ -z "$key" ]] && continue
            [[ "$key" == \#* ]] && continue
            # Only set if not already in environment (env vars take precedence)
            value="${value%\"}"     # strip trailing quote
            value="${value#\"}"     # strip leading quote
            if [[ -z "${!key:-}" ]]; then
                export "$key=$value"
            fi
        done < "$_conf"
        _conf_source="$_conf"
        log "Loaded config from $_conf"
        break
    fi
done

# ── Parse args (highest priority — override everything) ─────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --server-url)  MEMLAYER_SERVER_URL="$2"; shift 2 ;;
        --auth-token)  MEMLAYER_AUTH_TOKEN="$2"; shift 2 ;;
        --config)      # Explicit config file path
            if [[ -f "$2" ]]; then
                while IFS='=' read -r key value; do
                    key="${key%%#*}"; key="${key// /}"
                    [[ -z "$key" || "$key" == \#* ]] && continue
                    value="${value%\"}"; value="${value#\"}"
                    export "$key=$value"
                done < "$2"
                _conf_source="$2"
                log "Loaded config from $2"
            else
                err "Config file not found: $2"
                exit 1
            fi
            shift 2 ;;
        *)             shift ;;
    esac
done

# Validate required vars
if [[ -z "${MEMLAYER_SERVER_URL:-}" ]]; then
    err "MEMLAYER_SERVER_URL is required"
    err "Provide via: config file, env var, or --server-url"
    [[ -z "$_conf_source" ]] && err "No config file found at: ${CONF_PATHS[*]}"
    exit 1
fi
if [[ -z "${MEMLAYER_AUTH_TOKEN:-}" ]]; then
    err "MEMLAYER_AUTH_TOKEN is required"
    err "Provide via: config file, env var, or --auth-token"
    exit 1
fi

export MEMLAYER_SERVER_URL MEMLAYER_AUTH_TOKEN

# Read optional settings with defaults
MEMLAYER_INSTALL_PLUGIN="${MEMLAYER_INSTALL_PLUGIN:-true}"
MEMLAYER_INJECT_CLAUDEMD="${MEMLAYER_INJECT_CLAUDEMD:-true}"

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
    tarball="memlayer-daemon-${os}-${arch}.tar.gz"

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
        cp "$INSTALL_DIR/daemon/target/release/memlayer-daemon" "$DAEMON_BIN"
        chmod +x "$DAEMON_BIN"
        log "Daemon built from source"
        _installed=true
    fi

    if [[ "$_installed" != "true" ]]; then
        err "Could not install daemon (no binary available, no Rust toolchain)"
        exit 1
    fi
fi

# ── Step 3: Build CLI ─────────────────────────────────────────────
cli_dir="$INSTALL_DIR/cli"
if [[ -f "$cli_dir/package.json" ]] && command -v node &>/dev/null; then
    # Only rebuild if source changed
    if [[ ! -f "$cli_dir/dist/cli.js" ]] || \
       [[ "$cli_dir/src/cli.ts" -nt "$cli_dir/dist/cli.js" ]] || \
       [[ "$cli_dir/src/api-client.ts" -nt "$cli_dir/dist/cli.js" ]]; then
        log "Building CLI..."
        (cd "$cli_dir" && npm install --no-audit --no-fund -q 2>/dev/null && npx tsc 2>/dev/null)
        log "CLI built"
    else
        log "CLI up to date"
    fi
else
    err "Node.js not found — CLI will not be available"
fi

# ── Step 4: Install CLI binary ────────────────────────────────────
if [[ -f "$cli_dir/dist/cli.js" ]]; then
    ln -sf "$cli_dir/dist/cli.js" "$HOME/.local/bin/memlayer"
    chmod +x "$HOME/.local/bin/memlayer"
    log "CLI installed to ~/.local/bin/memlayer"

    # Remove old MCP registration if present
    if command -v claude &>/dev/null && claude mcp list 2>/dev/null | grep -q memlayer; then
        claude mcp remove memlayer --scope user 2>/dev/null || true
        log "Old MCP registration removed"
    fi
fi

# ── Step 5: Inject memory instructions into CLAUDE.md ────────────
if [[ "$MEMLAYER_INJECT_CLAUDEMD" == "true" ]]; then
    claudemd="$HOME/.claude/CLAUDE.md"
    template="$INSTALL_DIR/scripts/memlayer.claudemd.template"
    mkdir -p "$HOME/.claude"

    if [[ -f "$template" ]]; then
        if [[ -f "$claudemd" ]] && grep -q '## Memory (Cross-Session Recall)' "$claudemd"; then
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
else
    log "CLAUDE.md injection disabled via config"
fi

# ── Step 6: Install Claude Code plugin ─────────────────────────────
plugin_src="$INSTALL_DIR/plugin"
plugin_cache="$HOME/.claude/plugins/cache/memlayer-local/latest"

if [[ "$MEMLAYER_INSTALL_PLUGIN" != "true" ]]; then
    log "Plugin installation disabled via config"
elif [[ -d "$plugin_src/.claude-plugin" ]]; then
    mkdir -p "$plugin_cache"
    if command -v rsync &>/dev/null; then
        rsync -a "$plugin_src/" "$plugin_cache/"
    else
        cp -r "$plugin_src/." "$plugin_cache/"
    fi
    chmod +x "$plugin_cache/hooks/memory-read-hook.sh" 2>/dev/null || true

    # Register in installed_plugins.json
    installed_file="$HOME/.claude/plugins/installed_plugins.json"
    if [[ -f "$installed_file" ]] && command -v jq &>/dev/null; then
        if ! jq -e '.plugins["memlayer@memlayer-local"]' "$installed_file" &>/dev/null 2>&1; then
            jq --arg path "$plugin_cache" \
               --arg now "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)" \
              '.plugins["memlayer@memlayer-local"] = [{"scope":"user","installPath":$path,"version":"latest","installedAt":$now,"lastUpdated":$now}]' \
              "$installed_file" > "${installed_file}.tmp" && mv "${installed_file}.tmp" "$installed_file"
        fi
    fi

    # Enable in settings.json
    settings_file="$HOME/.claude/settings.json"
    if [[ -f "$settings_file" ]] && command -v jq &>/dev/null; then
        if ! jq -e '.enabledPlugins["memlayer@memlayer-local"]' "$settings_file" &>/dev/null 2>&1; then
            jq '.enabledPlugins["memlayer@memlayer-local"] = true' \
              "$settings_file" > "${settings_file}.tmp" && mv "${settings_file}.tmp" "$settings_file"
        fi
    fi

    log "Claude Code plugin installed"
else
    log "Plugin source not found — skipping"
fi

# ── Step 7: Write credentials env file ─────────────────────────────
env_dir="$HOME/.config/memlayer"
env_file="$env_dir/env"
mkdir -p "$env_dir"
cat > "$env_file" <<ENVEOF
MEMLAYER_AUTH_TOKEN=$MEMLAYER_AUTH_TOKEN
MEMLAYER_SERVER_URL=$MEMLAYER_SERVER_URL
ENVEOF
chmod 600 "$env_file"
log "Credentials written to $env_file"

# ── Step 8: Start daemon (background process) ─────────────────────
mkdir -p "$HOME/.local/share/memlayer"
# Kill any existing daemon
if pgrep -f memlayer-daemon &>/dev/null; then
    pkill -f memlayer-daemon 2>/dev/null || true
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
log "  Config:  ${_conf_source:-env/args}"
log "  Server:  $MEMLAYER_SERVER_URL"
log "  Daemon:  PID $daemon_pid"
log "  CLI:     $(command -v memlayer &>/dev/null && echo installed || echo not found)"
log "  Plugin:  $MEMLAYER_INSTALL_PLUGIN"
