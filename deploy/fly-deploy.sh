#!/usr/bin/env bash
# Deploy Memlayer server to Fly.io
# Usage: ./deploy/fly-deploy.sh [APP_NAME]
# Safe to re-run: all steps check for existing state before acting.
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

if ! flyctl auth whoami &>/dev/null 2>&1; then
    echo "Error: flyctl is not authenticated."
    echo "  Run: flyctl auth login"
    exit 1
fi

# ── Create app if it doesn't exist ────────────────────────────────
if ! flyctl apps list --json 2>/dev/null | grep -q "\"$APP_NAME\""; then
    echo "Creating Fly app: $APP_NAME"
    if ! flyctl apps create "$APP_NAME" --machines; then
        echo
        echo "Error: Failed to create Fly app."
        echo "  If you see a billing error, add billing info at:"
        echo "  https://fly.io/dashboard → Billing"
        echo
        echo "  Then re-run: ./deploy/fly-deploy.sh $APP_NAME"
        exit 1
    fi
else
    echo "Fly app '$APP_NAME' already exists"
fi

# ── Update fly.toml app name ─────────────────────────────────────
sed -i "s/^app = .*/app = \"$APP_NAME\"/" fly.toml

# ── Create volume if it doesn't exist ────────────────────────────
if ! flyctl volumes list -a "$APP_NAME" --json 2>/dev/null | grep -q '"response_files"'; then
    REGION=$(grep 'primary_region' fly.toml | sed 's/.*= *"//' | sed 's/".*//')
    echo "Creating persistent volume in $REGION"
    if ! flyctl volumes create response_files \
        --app "$APP_NAME" \
        --region "$REGION" \
        --size 1 \
        --yes; then
        echo
        echo "Error: Failed to create volume."
        echo "  Re-run to retry: ./deploy/fly-deploy.sh $APP_NAME"
        exit 1
    fi
else
    echo "Volume 'response_files' already exists"
fi

# ── Set secrets (only from env vars that are set) ─────────────────
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

# ── Pre-flight: build and smoke-test the cloud image locally ─────
echo "Running pre-flight smoke test (cloud Dockerfile, non-root user)..."
PREFLIGHT_PROJECT="memlayer-preflight-$$"

_preflight_cleanup() {
    docker compose -p "$PREFLIGHT_PROJECT" -f docker-compose.test.yml down -v --remove-orphans 2>/dev/null || true
}
trap _preflight_cleanup EXIT

if ! docker compose -p "$PREFLIGHT_PROJECT" -f docker-compose.test.yml up -d --build --wait 2>&1; then
    echo
    echo "Error: Pre-flight smoke test failed — the cloud image did not start."
    echo "  Fix the issue before deploying. Check logs with:"
    echo "  docker compose -p $PREFLIGHT_PROJECT -f docker-compose.test.yml logs"
    exit 1
fi

PREFLIGHT_URL="http://127.0.0.1:8421/health"
PREFLIGHT_OK=false
for _i in $(seq 1 10); do
    if curl -sf --max-time 3 "$PREFLIGHT_URL" >/dev/null 2>&1; then
        PREFLIGHT_OK=true
        break
    fi
    sleep 2
done

if ! $PREFLIGHT_OK; then
    echo
    echo "Error: Pre-flight health check failed — server did not respond at $PREFLIGHT_URL"
    echo "  Check logs: docker compose -p $PREFLIGHT_PROJECT -f docker-compose.test.yml logs test-server"
    exit 1
fi

echo "Pre-flight passed — cloud image starts and responds to health checks"
_preflight_cleanup
trap - EXIT

# ── Deploy ────────────────────────────────────────────────────────
echo "Deploying to Fly.io..."
if ! flyctl deploy --app "$APP_NAME" --remote-only; then
    echo
    echo "Error: Deployment failed."
    echo
    echo "  Check the build output above for details."
    echo "  To retry: ./deploy/fly-deploy.sh $APP_NAME"
    exit 1
fi

echo
echo "Deployed! Your server is at: https://$APP_NAME.fly.dev"
echo "Health check: https://$APP_NAME.fly.dev/health"
echo "API base:     https://$APP_NAME.fly.dev/api"
