#!/usr/bin/env bash
# Memlayer client setup — interactive, idempotent
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/scripts/lib.sh"

TOTAL_STEPS=7
DAEMON_BIN="$HOME/.local/bin/memlayer-daemon"
RELEASE_BASE="https://github.com/mikeydotio/memlayer/releases/latest/download"

# ── Parse CLI args ──────────────────────────────────────────────────
server_url=""
auth_token=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --server-url)  server_url="$2"; shift 2 ;;
        --auth-token)  auth_token="$2"; shift 2 ;;
        *)             shift ;;
    esac
done

echo
echo "  Memlayer Client Setup"
echo "  ====================="
echo

# ── Step 1: Prerequisites ───────────────────────────────────────────
step 1 $TOTAL_STEPS "Checking prerequisites"

require_cmd node
require_cmd npm

# Check Node version >= 18
node_version=$(node --version | sed 's/^v//' | cut -d. -f1)
if (( node_version < 18 )); then
    error "Node.js 18+ required, found v${node_version}"
    exit 1
fi
success "Node.js v$(node --version | sed 's/^v//') detected"

if command -v cargo &>/dev/null; then
    info "Rust/Cargo detected (source builds available)"
else
    warn "Rust not found — only pre-built binary install available"
fi

if command -v claude &>/dev/null; then
    info "Claude CLI detected"
fi

# ── Step 2: Daemon installation ─────────────────────────────────────
step 2 $TOTAL_STEPS "Installing daemon"

mkdir -p "$HOME/.local/bin"

if [[ -f "$DAEMON_BIN" ]]; then
    current_version=$("$DAEMON_BIN" --version 2>/dev/null || echo "unknown")
    info "Existing daemon found: $current_version"
    if ! confirm "Update?" "y"; then
        success "Keeping existing daemon"
        _skip_daemon=true
    fi
fi

if [[ "${_skip_daemon:-}" != "true" ]]; then
    echo
    echo "  How would you like to install the daemon?"
    echo "    1) Download pre-built binary (recommended)"
    echo "    2) Build from source (requires Rust)"

    if [[ -t 0 ]]; then
        read -r -p "  Choice [1]: " install_choice </dev/tty
    else
        install_choice=""
    fi
    install_choice="${install_choice:-1}"

    if [[ "$install_choice" == "2" ]]; then
        # Build from source
        if ! command -v cargo &>/dev/null; then
            error "Rust/Cargo not found. Install from https://rustup.rs/"
            exit 1
        fi

        daemon_src=""
        if [[ -f "$SCRIPT_DIR/daemon/Cargo.toml" ]]; then
            daemon_src="$SCRIPT_DIR/daemon"
        else
            daemon_src=$(prompt_value "Path to memlayer/daemon directory" "$HOME/memlayer/daemon")
        fi

        info "Building daemon from source..."
        (cd "$daemon_src" && cargo build --release)
        cp "$daemon_src/target/release/memlayer-daemon" "$DAEMON_BIN"
        chmod +x "$DAEMON_BIN"
        success "Daemon built and installed to $DAEMON_BIN"
    else
        # Download pre-built binary
        os=$(detect_os)
        arch=$(detect_arch)
        tarball="memlayer-daemon-${os}-${arch}.tar.gz"
        url="${RELEASE_BASE}/${tarball}"

        info "Downloading $tarball..."
        if curl -fSL --max-time 60 -o "/tmp/$tarball" "$url" 2>/dev/null; then
            tar -xzf "/tmp/$tarball" -C "$HOME/.local/bin/"
            chmod +x "$DAEMON_BIN"
            rm -f "/tmp/$tarball"
            success "Daemon installed to $DAEMON_BIN"
        else
            warn "Download failed (binary may not be available for $os/$arch)"
            echo
            if command -v cargo &>/dev/null; then
                if confirm "Build from source instead?" "y"; then
                    daemon_src=""
                    if [[ -f "$SCRIPT_DIR/daemon/Cargo.toml" ]]; then
                        daemon_src="$SCRIPT_DIR/daemon"
                    else
                        daemon_src=$(prompt_value "Path to memlayer/daemon directory" "$HOME/memlayer/daemon")
                    fi
                    info "Building daemon from source..."
                    (cd "$daemon_src" && cargo build --release)
                    cp "$daemon_src/target/release/memlayer-daemon" "$DAEMON_BIN"
                    chmod +x "$DAEMON_BIN"
                    success "Daemon built and installed to $DAEMON_BIN"
                else
                    error "Cannot continue without daemon binary"
                    exit 1
                fi
            else
                error "No pre-built binary available and Rust not installed."
                echo "  Install Rust (https://rustup.rs/) and re-run, or"
                echo "  check releases at https://github.com/mikeydotio/memlayer/releases"
                exit 1
            fi
        fi
    fi

    # Check PATH
    if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
        warn "$HOME/.local/bin is not in your PATH"
        echo "  Add to your shell profile: export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
fi

# ── Step 3: Server connection ───────────────────────────────────────
step 3 $TOTAL_STEPS "Connecting to server"

if [[ -z "$server_url" ]]; then
    # Try auto-detect
    if curl -sf --max-time 3 "http://localhost:8420/health" &>/dev/null; then
        info "Found server at localhost:8420"
        if confirm "Use this server?" "y"; then
            server_url="http://localhost:8420/api"
        fi
    fi

    if [[ -z "$server_url" ]]; then
        server_url=$(prompt_value "Enter server URL" "http://localhost:8420/api")
    fi
fi

# Validate server
health_url="${server_url%/api}/health"
health_url="${health_url%/}/health"
# Normalize: handle cases like http://host:8420/api → http://host:8420/health
health_url=$(echo "$server_url" | sed 's|/api$||; s|/api/$||; s|/*$||')/health

if curl -sf --max-time 5 "$health_url" &>/dev/null; then
    success "Server is reachable"
else
    warn "Could not reach server at $health_url"
    if ! confirm "Continue anyway?" "y"; then
        exit 1
    fi
fi

# ── Step 4: Auth token ──────────────────────────────────────────────
step 4 $TOTAL_STEPS "Configuring authentication"

# Check existing token in env file or service files
existing_token=""
env_file="$HOME/.config/memlayer/env"
service_file="$HOME/.config/systemd/user/memlayer-daemon.service"
plist_file="$HOME/Library/LaunchAgents/io.memlayer.daemon.plist"

if [[ -f "$env_file" ]]; then
    existing_token=$(grep '^MEMLAYER_AUTH_TOKEN=' "$env_file" | tail -1 | sed 's/^MEMLAYER_AUTH_TOKEN=//')
elif [[ -f "$service_file" ]]; then
    existing_token=$(grep 'MEMLAYER_AUTH_TOKEN=' "$service_file" | tail -1 | sed 's/.*MEMLAYER_AUTH_TOKEN=//')
fi
if [[ -z "$existing_token" && -f "$plist_file" ]]; then
    existing_token=$(grep -A1 'MEMLAYER_AUTH_TOKEN' "$plist_file" | grep '<string>' | sed 's/.*<string>//;s|</string>||')
fi

if [[ -n "$existing_token" && -z "$auth_token" ]]; then
    info "Found existing auth token: $(mask_token "$existing_token")"
    if confirm "Keep this token?" "y"; then
        auth_token="$existing_token"
    fi
fi

# Try reading from local .env
if [[ -z "$auth_token" && -f "$SCRIPT_DIR/.env" ]]; then
    local_token=$(grep '^MEMLAYER_AUTH_TOKEN=' "$SCRIPT_DIR/.env" 2>/dev/null | cut -d= -f2 || true)
    if [[ -n "$local_token" && "$local_token" != "changeme" ]]; then
        info "Found token in local .env: $(mask_token "$local_token")"
        if confirm "Use this token?" "y"; then
            auth_token="$local_token"
        fi
    fi
fi

if [[ -z "$auth_token" ]]; then
    if [[ ! -t 0 ]]; then
        error "Auth token is required but stdin is not interactive (piped install)."
        echo
        echo "  Re-run with arguments:"
        echo "    ~/.memlayer/setup_client.sh --server-url https://YOUR-APP.fly.dev/api --auth-token YOUR_TOKEN"
        echo
        echo "  Or run interactively:"
        echo "    cd ~/.memlayer && ./setup_client.sh"
        exit 1
    fi
    while true; do
        auth_token=$(prompt_value "Paste the auth token from setup_server.sh output" "")
        if [[ -z "$auth_token" ]]; then
            error "Auth token cannot be empty"
            continue
        fi
        if (( ${#auth_token} < 16 )); then
            error "Auth token must be at least 16 characters"
            continue
        fi
        break
    done
fi

# Verify token against server
api_base="${server_url%/}"
if curl -sf --max-time 5 \
    -H "Authorization: Bearer $auth_token" \
    -H "Content-Type: application/json" \
    -d '{"query":"test","limit":1}' \
    "${api_base}/search" &>/dev/null; then
    success "Auth token verified"
else
    warn "Could not verify token against server (server may be unreachable)"
    if ! confirm "Continue with this token?" "y"; then
        exit 1
    fi
fi

# ── Step 5: Background service ──────────────────────────────────────
step 5 $TOTAL_STEPS "Setting up background service"

_os=$(detect_os)

if [[ "$_os" == "linux" ]]; then
    # systemd
    service_dir="$HOME/.config/systemd/user"
    service_path="$service_dir/memlayer-daemon.service"

    if systemctl --user is-active memlayer-daemon &>/dev/null; then
        info "Daemon service is already running"
        if ! confirm "Update service?" "y"; then
            success "Keeping existing service"
            _skip_service=true
        else
            systemctl --user stop memlayer-daemon || true
        fi
    fi

    if [[ "${_skip_service:-}" != "true" ]]; then
        if confirm "Install systemd service for automatic startup?" "y"; then
            # Write secrets to env file (not embedded in service unit)
            env_dir="$HOME/.config/memlayer"
            env_file="$env_dir/env"
            mkdir -p "$env_dir"
            cat > "$env_file" <<ENVEOF
MEMLAYER_AUTH_TOKEN=$auth_token
MEMLAYER_SERVER_URL=$server_url
ENVEOF
            chmod 600 "$env_file"
            success "Credentials written to $env_file"

            mkdir -p "$service_dir"
            sed \
                -e "s|{{DAEMON_PATH}}|$DAEMON_BIN|g" \
                "$SCRIPT_DIR/scripts/memlayer-daemon.service.template" > "$service_path"

            systemctl --user daemon-reload
            systemctl --user enable --now memlayer-daemon
            success "systemd service installed and started"

            # Check lingering
            if ! loginctl show-user "$USER" 2>/dev/null | grep -q 'Linger=yes'; then
                warn "User lingering is not enabled — service won't survive logout"
                if confirm "Enable lingering? (requires sudo)" "y"; then
                    sudo loginctl enable-linger "$USER"
                    success "Lingering enabled"
                fi
            fi
        else
            echo
            info "To run manually:"
            echo "    MEMLAYER_SERVER_URL=\"$server_url\" \\"
            echo "    MEMLAYER_AUTH_TOKEN=\"$auth_token\" \\"
            echo "    $DAEMON_BIN"
        fi
    fi

elif [[ "$_os" == "macos" ]]; then
    # launchd
    plist_dir="$HOME/Library/LaunchAgents"
    plist_path="$plist_dir/io.memlayer.daemon.plist"

    if launchctl list 2>/dev/null | grep -q io.memlayer.daemon; then
        info "Daemon launch agent is already loaded"
        if ! confirm "Update launch agent?" "y"; then
            success "Keeping existing launch agent"
            _skip_service=true
        else
            launchctl unload "$plist_path" 2>/dev/null || true
        fi
    fi

    if [[ "${_skip_service:-}" != "true" ]]; then
        if confirm "Install launchd agent for automatic startup?" "y"; then
            mkdir -p "$plist_dir"
            mkdir -p "$HOME/Library/Logs"
            sed \
                -e "s|{{DAEMON_PATH}}|$DAEMON_BIN|g" \
                -e "s|{{SERVER_URL}}|$server_url|g" \
                -e "s|{{AUTH_TOKEN}}|$auth_token|g" \
                -e "s|{{HOME}}|$HOME|g" \
                "$SCRIPT_DIR/scripts/io.memlayer.daemon.plist.template" > "$plist_path"

            launchctl load -w "$plist_path"
            success "Launch agent installed and loaded"
        else
            echo
            info "To run manually:"
            echo "    MEMLAYER_SERVER_URL=\"$server_url\" \\"
            echo "    MEMLAYER_AUTH_TOKEN=\"$auth_token\" \\"
            echo "    $DAEMON_BIN"
        fi
    fi
else
    warn "Unknown OS — skipping service installation"
    echo
    info "To run manually:"
    echo "    MEMLAYER_SERVER_URL=\"$server_url\" \\"
    echo "    MEMLAYER_AUTH_TOKEN=\"$auth_token\" \\"
    echo "    $DAEMON_BIN"
fi

# ── Step 6: CLI binary ────────────────────────────────────────────
step 6 $TOTAL_STEPS "Installing CLI binary"

CLI_BIN="$HOME/.local/bin/memlayer"
cli_dir="$SCRIPT_DIR/cli"
_cli_status="skipped"

if [[ -f "$cli_dir/package.json" ]] && command -v node &>/dev/null; then
    info "Building CLI..."
    (cd "$cli_dir" && npm install --no-audit --no-fund && npx tsc)

    # Remove old MCP registration if present
    if command -v claude &>/dev/null && claude mcp list 2>/dev/null | grep -q memlayer; then
        info "Removing old MCP registration..."
        claude mcp remove memlayer --scope user 2>/dev/null || true
    fi

    # Create symlink to CLI binary
    mkdir -p "$HOME/.local/bin"
    ln -sf "$cli_dir/dist/cli.js" "$CLI_BIN"
    chmod +x "$CLI_BIN"
    success "CLI installed to $CLI_BIN"
    _cli_status="installed"
else
    warn "Node.js not found — CLI will not be available"
fi

# ── Step 7: CLAUDE.md ──────────────────────────────────────────────
step 7 $TOTAL_STEPS "Configuring CLAUDE.md"

_claudemd_status="skipped"
claudemd="$HOME/.claude/CLAUDE.md"

if confirm "Add memory instructions to ~/.claude/CLAUDE.md?" "y"; then
    mkdir -p "$HOME/.claude"

    if [[ -f "$claudemd" ]]; then
        if grep -q '## Memory (Cross-Session Recall)' "$claudemd"; then
            info "Memory section already exists in CLAUDE.md"
            if confirm "Replace with updated version?" "y"; then
                # Remove old section: from heading to next ## or EOF
                # Use awk for reliable multi-line deletion
                awk '
                    /^## Memory \(Cross-Session Recall\)/ { skip=1; next }
                    /^## / && skip { skip=0 }
                    !skip { print }
                ' "$claudemd" > "${claudemd}.tmp"
                mv "${claudemd}.tmp" "$claudemd"
            else
                success "Keeping existing memory section"
                _claudemd_status="existing"
                _skip_claudemd=true
            fi
        fi
    fi

    if [[ "${_skip_claudemd:-}" != "true" ]]; then
        if [[ ! -f "$claudemd" ]]; then
            touch "$claudemd"
        fi
        # Ensure trailing newline before appending
        [[ -s "$claudemd" ]] && [[ "$(tail -c1 "$claudemd")" != "" ]] && echo >> "$claudemd"
        echo >> "$claudemd"
        cat "$SCRIPT_DIR/scripts/memlayer.claudemd.template" >> "$claudemd"
        success "Memory instructions added to CLAUDE.md"
        _claudemd_status="installed"
    fi
else
    info "Skipping CLAUDE.md setup"
fi

# ── Summary ─────────────────────────────────────────────────────────
echo

# Determine service status
_service_status="not installed"
if [[ "$_os" == "linux" ]] && systemctl --user is-active memlayer-daemon &>/dev/null 2>&1; then
    _service_status="running (systemd)"
elif [[ "$_os" == "macos" ]] && launchctl list 2>/dev/null | grep -q io.memlayer.daemon; then
    _service_status="running (launchd)"
fi

print_box \
    "Memlayer Client — Setup Complete" \
    "" \
    "Daemon:       $DAEMON_BIN" \
    "Service:      $_service_status" \
    "Server:       $server_url" \
    "CLI:          $_cli_status" \
    "CLAUDE.md:    $_claudemd_status"

echo
success "Start a new Claude Code session and ask: \"Do you remember what we worked on?\""
echo
