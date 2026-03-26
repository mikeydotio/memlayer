#!/usr/bin/env bash
# Deploy Memlayer server to Fly.io
# Usage: ./deploy/fly-deploy.sh [APP_NAME]
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

APP_NAME="${1:-memlayer}"

cd "$REPO_ROOT"

# ── Check flyctl ──────────────────────────────────────────────────
if ! command -v flyctl &>/dev/null; then
    echo "Error: flyctl is not installed."
    echo "  Install: curl -L https://fly.io/install.sh | sh"
    exit 1
fi

# ── Create app if it doesn't exist ────────────────────────────────
if ! flyctl apps list --json 2>/dev/null | grep -q "\"$APP_NAME\""; then
    echo "Creating Fly app: $APP_NAME"
    flyctl apps create "$APP_NAME" --machines
else
    echo "Fly app '$APP_NAME' already exists"
fi

# ── Update fly.toml app name ─────────────────────────────────────
sed -i "s/^app = .*/app = \"$APP_NAME\"/" fly.toml

# ── Create volume if it doesn't exist ────────────────────────────
if ! flyctl volumes list -a "$APP_NAME" --json 2>/dev/null | grep -q '"response_files"'; then
    REGION=$(grep 'primary_region' fly.toml | sed 's/.*= *"//' | sed 's/".*//')
    echo "Creating persistent volume in $REGION"
    flyctl volumes create response_files \
        --app "$APP_NAME" \
        --region "$REGION" \
        --size 1 \
        --yes
else
    echo "Volume 'response_files' already exists"
fi

# ── Set secrets (only if not already set) ─────────────────────────
_set_secrets() {
    local secrets_to_set=()

    if [[ -n "${DATABASE_URL:-}" ]]; then
        secrets_to_set+=("DATABASE_URL=$DATABASE_URL")
    fi
    if [[ -n "${MEMLAYER_AUTH_TOKEN:-}" ]]; then
        secrets_to_set+=("MEMLAYER_AUTH_TOKEN=$MEMLAYER_AUTH_TOKEN")
    fi
    if [[ -n "${OPENAI_API_KEY:-}" ]]; then
        secrets_to_set+=("OPENAI_API_KEY=$OPENAI_API_KEY")
    fi
    if [[ -n "${EMBEDDING_PROVIDER:-}" ]]; then
        secrets_to_set+=("EMBEDDING_PROVIDER=$EMBEDDING_PROVIDER")
    fi
    if [[ -n "${ANTHROPIC_API_KEY:-}" ]]; then
        secrets_to_set+=("ANTHROPIC_API_KEY=$ANTHROPIC_API_KEY")
    fi
    if [[ -n "${INDEX_MODE:-}" ]]; then
        secrets_to_set+=("INDEX_MODE=$INDEX_MODE")
    fi
    if [[ -n "${INDEX_LLM_PROVIDER:-}" ]]; then
        secrets_to_set+=("INDEX_LLM_PROVIDER=$INDEX_LLM_PROVIDER")
    fi

    if [[ ${#secrets_to_set[@]} -gt 0 ]]; then
        echo "Setting ${#secrets_to_set[@]} secret(s)..."
        flyctl secrets set "${secrets_to_set[@]}" --app "$APP_NAME" --stage
    fi
}

_set_secrets

# ── Deploy ────────────────────────────────────────────────────────
echo "Deploying to Fly.io..."
flyctl deploy --app "$APP_NAME" --remote-only

echo
echo "Deployed! Your server is at: https://$APP_NAME.fly.dev"
echo "Health check: https://$APP_NAME.fly.dev/health"
echo "API base:     https://$APP_NAME.fly.dev/api"
