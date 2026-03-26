#!/usr/bin/env bash
# Memlayer server setup — interactive, idempotent
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/scripts/lib.sh"

TOTAL_STEPS=5
cd "$SCRIPT_DIR"

echo
echo "  Memlayer Server Setup"
echo "  ====================="
echo

# ── Step 1: Prerequisites ───────────────────────────────────────────
step 1 $TOTAL_STEPS "Checking prerequisites"

check_docker
success "Docker and Docker Compose v2 detected"

# ── Step 2: Environment configuration ───────────────────────────────
step 2 $TOTAL_STEPS "Configuring environment"

if [[ -f .env ]]; then
    info "Existing .env found:"
    # Show masked values
    while IFS='=' read -r key value; do
        [[ -z "$key" || "$key" =~ ^# ]] && continue
        case "$key" in
            *PASSWORD*|*TOKEN*|*KEY*)
                echo "    $key=$(mask_token "$value")"
                ;;
            *)
                echo "    $key=$value"
                ;;
        esac
    done < .env

    if confirm "Keep existing configuration?" "y"; then
        success "Keeping existing .env"
        # Source for later use
        set -a; source .env; set +a
    else
        info "Regenerating .env..."
        _generate_env=true
    fi
else
    _generate_env=true
fi

if [[ "${_generate_env:-}" == "true" ]]; then
    db_pass=$(generate_secret)
    auth_token=$(generate_secret)

    echo
    info "Generated credentials:"
    echo "    POSTGRES_PASSWORD=$db_pass"
    echo "    MEMLAYER_AUTH_TOKEN=$auth_token"

    if ! confirm "Use these values?" "y"; then
        db_pass=$(prompt_value "Database password" "$db_pass")
        auth_token=$(prompt_value "Auth token" "$auth_token")
    fi

    # Optional OpenAI
    openai_key=""
    embedding_provider="openai"
    if confirm "Configure OpenAI embeddings? (optional)" "n"; then
        while true; do
            openai_key=$(prompt_secret "OpenAI API key")
            if [[ -z "$openai_key" ]]; then
                warn "No key entered, skipping OpenAI embeddings"
                break
            fi
            printf "  Testing API key... "
            if test_openai_key "$openai_key"; then
                success "API key is valid"
                break
            else
                warn "API key test failed (invalid key or network error)"
                if ! confirm "Try a different key?" "y"; then
                    if confirm "Continue without OpenAI embeddings?" "y"; then
                        openai_key=""
                        info "Skipping OpenAI embeddings (FTS-only mode)"
                    fi
                    break
                fi
            fi
        done
    fi

    cat > .env <<EOF
# PostgreSQL
POSTGRES_PASSWORD=$db_pass

# Server auth
MEMLAYER_AUTH_TOKEN=$auth_token

# Embeddings (OpenAI)
OPENAI_API_KEY=${openai_key:-}
EMBEDDING_PROVIDER=${embedding_provider}
EMBEDDING_MODEL=text-embedding-3-small
EMBEDDING_DIMENSIONS=1536

# Embeddings (Ollama alternative)
# EMBEDDING_PROVIDER=ollama
# OLLAMA_BASE_URL=http://host.docker.internal:11434
EOF

    success ".env written"

    # Export for later steps
    export POSTGRES_PASSWORD="$db_pass"
    export MEMLAYER_AUTH_TOKEN="$auth_token"
    export OPENAI_API_KEY="${openai_key:-}"

    # Source for consistency
    set -a; source .env; set +a
fi

# ── Step 3: Docker Compose startup ──────────────────────────────────
step 3 $TOTAL_STEPS "Starting Docker services"

running_containers=$(docker compose ps --format '{{.Name}}' 2>/dev/null | grep -c memlayer || true)

if (( running_containers > 0 )); then
    info "Found $running_containers running memlayer container(s)"
    if confirm "Rebuild and restart?" "n"; then
        docker compose pull
        docker compose up -d --build
    else
        info "Keeping existing containers"
    fi
else
    docker compose up -d
fi

success "Docker services started"

# ── Step 4: Health check ────────────────────────────────────────────
step 4 $TOTAL_STEPS "Waiting for server to be ready"

if ! wait_for_url "http://localhost:8420/health" 60; then
    error "Server did not become healthy within 60 seconds"
    echo "  Check logs: docker compose logs -f server"
    exit 1
fi

success "Server is healthy"

# ── Detect Tailscale ────────────────────────────────────────────────
if command -v tailscale &>/dev/null; then
    ts_ip=$(tailscale ip -4 2>/dev/null || true)
    if [[ -n "$ts_ip" ]]; then
        info "Tailscale detected (IP: $ts_ip)"
        if confirm "Bind server to Tailscale only? (recommended for security)" "n"; then
            # Update .env
            if grep -q '^MEMLAYER_BIND_ADDR=' .env; then
                sed -i "s/^MEMLAYER_BIND_ADDR=.*/MEMLAYER_BIND_ADDR=$ts_ip/" .env
            else
                echo "MEMLAYER_BIND_ADDR=$ts_ip" >> .env
            fi
            success "Server will bind to $ts_ip on next restart"
            info "Run 'docker compose up -d' to apply"
        fi
    fi
fi

# ── Step 5: Summary ─────────────────────────────────────────────────
step 5 $TOTAL_STEPS "Setup complete"

# Determine embedding status
embed_status="FTS-only (no OpenAI key)"
if [[ -n "${OPENAI_API_KEY:-}" && "${OPENAI_API_KEY:-}" != "sk-..." ]]; then
    embed_status="OpenAI (${EMBEDDING_MODEL:-text-embedding-3-small})"
fi

# Read auth token from .env for display
display_token="${MEMLAYER_AUTH_TOKEN:-}"

# Determine the best reachable address for clients
client_host="localhost"
if [[ -n "${ts_ip:-}" ]]; then
    client_host="$ts_ip"
elif [[ -n "${MEMLAYER_BIND_ADDR:-}" && "${MEMLAYER_BIND_ADDR}" != "0.0.0.0" && "${MEMLAYER_BIND_ADDR}" != "127.0.0.1" ]]; then
    client_host="$MEMLAYER_BIND_ADDR"
fi
client_api_url="http://${client_host}:8420/api"

echo
print_box \
    "Memlayer Server — Running" \
    "" \
    "Server URL:   http://localhost:8420" \
    "Health:       http://localhost:8420/health" \
    "API base:     http://localhost:8420/api" \
    "Embeddings:   $embed_status" \
    "" \
    "Auth Token:   $(mask_token "$display_token")"

echo
info "To set up Memlayer for Claude Code on each client machine, run:"
echo
echo "  curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash -s -- --server-url $client_api_url --auth-token $display_token"
echo
if [[ "$client_host" == "localhost" ]]; then
    warn "Server address is localhost — replace with your server's IP or hostname"
    echo "  if installing on a different machine."
    echo
fi
echo
