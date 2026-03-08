#!/usr/bin/env bash
# Memlayer one-liner installer
# Usage: curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash
set -euo pipefail

INSTALL_DIR="$HOME/.memlayer"
REPO_URL="https://github.com/mikeydotio/memlayer.git"
TARBALL_URL="https://github.com/mikeydotio/memlayer/archive/refs/heads/main.tar.gz"

info()    { printf '\033[1;34m[*]\033[0m %s\n' "$*"; }
success() { printf '\033[1;32m[+]\033[0m %s\n' "$*"; }
warn()    { printf '\033[1;33m[!]\033[0m %s\n' "$*"; }
error()   { printf '\033[1;31m[-]\033[0m %s\n' "$*"; }

# Check basic deps
for cmd in curl tar; do
    if ! command -v "$cmd" &>/dev/null; then
        error "'$cmd' is required but not found."
        exit 1
    fi
done

echo
echo "  Memlayer Installer"
echo "  ==================="
echo

# Idempotency: update if already installed
if [[ -d "$INSTALL_DIR" ]]; then
    info "Existing installation found at $INSTALL_DIR"

    # Non-interactive when piped
    if [[ -t 0 ]]; then
        read -r -p "  Update and re-run setup? [Y/n] " answer </dev/tty
        answer="${answer:-y}"
        if [[ "${answer,,}" != "y" ]]; then
            info "Aborted."
            exit 0
        fi
    fi

    if [[ -d "$INSTALL_DIR/.git" ]] && command -v git &>/dev/null; then
        info "Updating via git pull..."
        (cd "$INSTALL_DIR" && git pull --ff-only)
    else
        info "Updating via tarball..."
        rm -rf "$INSTALL_DIR"
        mkdir -p "$INSTALL_DIR"
        curl -fsSL "$TARBALL_URL" | tar -xz --strip-components=1 -C "$INSTALL_DIR"
    fi
else
    # Fresh install
    if command -v git &>/dev/null; then
        info "Cloning memlayer..."
        git clone "$REPO_URL" "$INSTALL_DIR"
    else
        info "Downloading memlayer..."
        mkdir -p "$INSTALL_DIR"
        curl -fsSL "$TARBALL_URL" | tar -xz --strip-components=1 -C "$INSTALL_DIR"
    fi
fi

# Sanity check
if [[ ! -f "$INSTALL_DIR/setup_client.sh" ]] || [[ ! -f "$INSTALL_DIR/mcp/package.json" ]]; then
    error "Download appears incomplete — setup_client.sh or mcp/package.json missing"
    exit 1
fi

success "Memlayer downloaded to $INSTALL_DIR"
echo

# Hand off to client setup
chmod +x "$INSTALL_DIR/setup_client.sh"
exec "$INSTALL_DIR/setup_client.sh" "$@"
