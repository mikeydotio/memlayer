#!/usr/bin/env bash
# Memlayer shared shell library — sourced by setup/uninstall scripts
set -euo pipefail

# ── Output formatting ───────────────────────────────────────────────
_color() {
    if [[ -t 1 ]]; then printf '\033[%sm' "$1"; else true; fi
}
_reset() {
    if [[ -t 1 ]]; then printf '\033[0m'; else true; fi
}

info()    { _color "1;34"; printf '[*] '; _reset; echo "$*"; }
success() { _color "1;32"; printf '[+] '; _reset; echo "$*"; }
warn()    { _color "1;33"; printf '[!] '; _reset; echo "$*"; }
error()   { _color "1;31"; printf '[-] '; _reset; echo "$*"; }

step() {
    local n="$1" total="$2"; shift 2
    _color "1;37"
    printf '[%d/%d] ' "$n" "$total"
    _reset
    _color "1"
    echo "$*"
    _reset
}

# ── Interactive prompts ─────────────────────────────────────────────

# confirm PROMPT DEFAULT → returns 0 (yes) or 1 (no)
# DEFAULT: "y" or "n"
confirm() {
    local prompt="$1" default="${2:-y}"
    if [[ ! -t 0 ]]; then
        [[ "$default" == "y" ]] && return 0 || return 1
    fi
    local hint
    [[ "$default" == "y" ]] && hint="[Y/n]" || hint="[y/N]"
    local answer
    read -r -p "  $prompt $hint " answer </dev/tty
    answer="${answer:-$default}"
    [[ "${answer,,}" == "y" ]] && return 0 || return 1
}

# prompt_value PROMPT DEFAULT → echoes input or DEFAULT
prompt_value() {
    local prompt="$1" default="${2:-}"
    if [[ ! -t 0 ]]; then
        echo "$default"
        return
    fi
    local value
    if [[ -n "$default" ]]; then
        read -r -p "  $prompt [$default]: " value </dev/tty
    else
        read -r -p "  $prompt: " value </dev/tty
    fi
    echo "${value:-$default}"
}

# prompt_secret PROMPT → reads without echo
prompt_secret() {
    local prompt="$1" value
    if [[ ! -t 0 ]]; then
        echo ""
        return
    fi
    read -r -s -p "  $prompt: " value </dev/tty
    echo >&2  # newline after hidden input
    echo "$value"
}

# ── Secret generation ───────────────────────────────────────────────

generate_secret() {
    openssl rand -base64 32 2>/dev/null | tr -dc 'a-zA-Z0-9' | head -c 32 ||
    head -c 32 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c 32
}

# ── Utilities ────────────────────────────────────────────────────────

require_cmd() {
    local name="$1"
    if ! command -v "$name" &>/dev/null; then
        error "'$name' is required but not found."
        case "$name" in
            docker)   echo "  Install: https://docs.docker.com/get-docker/" ;;
            node)     echo "  Install: https://nodejs.org/" ;;
            npm)      echo "  Install: https://nodejs.org/" ;;
            cargo)    echo "  Install: https://rustup.rs/" ;;
            claude)   echo "  Install: npm install -g @anthropic-ai/claude-code" ;;
            curl)     echo "  Install: apt install curl / brew install curl" ;;
            tar)      echo "  Install: apt install tar / brew install gnu-tar" ;;
            git)      echo "  Install: apt install git / brew install git" ;;
        esac
        exit 1
    fi
}

check_docker() {
    require_cmd docker
    if ! docker compose version &>/dev/null; then
        error "Docker Compose v2 is required (docker compose, not docker-compose)."
        echo "  Update Docker or install the compose plugin."
        exit 1
    fi
    if ! docker info &>/dev/null 2>&1; then
        error "Docker daemon is not running."
        echo "  Start Docker Desktop or: sudo systemctl start docker"
        exit 1
    fi
}

check_port() {
    local port="$1"
    if [[ "$(detect_os)" == "linux" ]]; then
        ss -tlnp 2>/dev/null | grep -q ":${port} " && return 0
    else
        lsof -iTCP:"${port}" -sTCP:LISTEN &>/dev/null && return 0
    fi
    return 1
}

detect_os() {
    case "$(uname -s)" in
        Linux*)  echo "linux" ;;
        Darwin*) echo "macos" ;;
        *)       echo "unknown" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)    echo "x86_64" ;;
        aarch64|arm64)   echo "aarch64" ;;
        *)               echo "unknown" ;;
    esac
}

# mask_token TOKEN → shows first 4 and last 4 chars
mask_token() {
    local t="$1"
    if [[ ${#t} -le 8 ]]; then
        echo "****"
    else
        echo "${t:0:4}...${t: -4}"
    fi
}

# print_box LINES... → bordered output box
print_box() {
    local max=0 line
    for line in "$@"; do
        (( ${#line} > max )) && max=${#line}
    done
    local border
    border=$(printf '─%.0s' $(seq 1 $((max + 2))))
    echo "┌${border}┐"
    for line in "$@"; do
        printf '│ %-*s │\n' "$max" "$line"
    done
    echo "└${border}┘"
}

# wait_for_url URL TIMEOUT_SECS → polls until healthy
wait_for_url() {
    local url="$1" timeout="${2:-60}"
    local elapsed=0
    printf "  Waiting for %s " "$url"
    while (( elapsed < timeout )); do
        if curl -sf --max-time 3 "$url" &>/dev/null; then
            echo " ready!"
            return 0
        fi
        printf "."
        sleep 2
        elapsed=$((elapsed + 2))
    done
    echo " timeout!"
    return 1
}
