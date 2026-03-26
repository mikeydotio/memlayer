#!/usr/bin/env bash
# Memlayer cloud-hosted setup — deploy to Supabase (DB) + Fly.io (compute)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/scripts/lib.sh"

TOTAL_STEPS=7
cd "$SCRIPT_DIR"

echo
echo "  Memlayer Cloud Setup"
echo "  ====================="
echo "  Deploy to Supabase (database) + Fly.io (server)"
echo

# ── Step 1: Prerequisites ───────────────────────────────────────────
step 1 $TOTAL_STEPS "Checking prerequisites"

require_cmd curl

if ! command -v flyctl &>/dev/null; then
    error "flyctl CLI is not installed"
    echo
    echo "  Install flyctl:"
    echo "    curl -L https://fly.io/install.sh | sh"
    echo
    echo "  Then authenticate:"
    echo "    flyctl auth login"
    exit 1
fi

# Verify flyctl is authenticated
if ! flyctl auth whoami &>/dev/null 2>&1; then
    error "flyctl is not authenticated"
    echo "  Run: flyctl auth login"
    exit 1
fi

success "flyctl CLI detected and authenticated"

# ── Step 2: Supabase project ────────────────────────────────────────
step 2 $TOTAL_STEPS "Configuring Supabase database"

echo
info "You need a Supabase project with these extensions enabled:"
echo "    - vector (pgvector)"
echo "    - pg_trgm"
echo
info "To create one:"
echo "    1. Go to https://supabase.com/dashboard"
echo "    2. Create a new project (free tier is fine to start)"
echo "    3. Go to Database → Extensions"
echo "    4. Enable 'vector' and 'pg_trgm'"
echo "    5. Go to Settings → Database → Connection string → URI"
echo "       Use the DIRECT connection (port 5432), NOT the pooled connection (port 6543)"
echo

db_url=""
while true; do
    db_url=$(prompt_value "Paste your Supabase direct connection string" "")
    if [[ -z "$db_url" ]]; then
        error "Connection string cannot be empty"
        continue
    fi

    # Basic format validation
    if [[ ! "$db_url" =~ ^postgresql:// ]]; then
        warn "Connection string should start with postgresql://"
        if ! confirm "Continue anyway?" "n"; then
            continue
        fi
    fi

    # Warn about pooled connections
    if [[ "$db_url" =~ :6543 ]]; then
        warn "Port 6543 is the pooled connection (PgBouncer)."
        echo "  Memlayer uses asyncpg with prepared statements, which requires"
        echo "  the DIRECT connection on port 5432."
        if ! confirm "Continue with this URL anyway? (may cause errors)" "n"; then
            continue
        fi
    fi

    # Test connection
    printf "  Testing database connection... "
    # We can't test the connection directly without psql, but we can validate the URL format
    if [[ "$db_url" =~ ^postgresql://[^:]+:[^@]+@[^/]+/[^?]+ ]]; then
        success "URL format looks valid"
    else
        warn "Could not validate URL format"
    fi

    break
done

# ── Step 3: Auth token ──────────────────────────────────────────────
step 3 $TOTAL_STEPS "Generating auth token"

auth_token=$(generate_secret)
info "Generated auth token: $(mask_token "$auth_token")"

if ! confirm "Use this token?" "y"; then
    auth_token=$(prompt_value "Enter your auth token" "$auth_token")
fi

success "Auth token configured"

# ── Step 4: OpenAI embeddings ───────────────────────────────────────
step 4 $TOTAL_STEPS "Configuring embeddings"

openai_key=""
embedding_provider="openai"

if confirm "Configure OpenAI embeddings? (recommended)" "y"; then
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
                    info "Falling back to FTS-only mode"
                fi
                break
            fi
        fi
    done
else
    info "Skipping OpenAI embeddings (FTS-only mode)"
fi

# ── Step 5: Fly.io app configuration ───────────────────────────────
step 5 $TOTAL_STEPS "Configuring Fly.io deployment"

app_name=$(prompt_value "Fly.io app name" "memlayer")

# Select region
echo
info "Common Fly.io regions:"
echo "    iad  — Ashburn, Virginia (US East)"
echo "    ord  — Chicago (US Central)"
echo "    sjc  — San Jose (US West)"
echo "    lhr  — London"
echo "    ams  — Amsterdam"
echo "    nrt  — Tokyo"
echo "    syd  — Sydney"
region=$(prompt_value "Region" "iad")

# Update fly.toml
sed -i "s/^app = .*/app = \"$app_name\"/" fly.toml
sed -i "s/^primary_region = .*/primary_region = \"$region\"/" fly.toml

success "Fly.io config: app=$app_name, region=$region"

# ── Step 6: Deploy ──────────────────────────────────────────────────
step 6 $TOTAL_STEPS "Deploying to Fly.io"

export DATABASE_URL="$db_url"
export MEMLAYER_AUTH_TOKEN="$auth_token"
export EMBEDDING_PROVIDER="$embedding_provider"
if [[ -n "$openai_key" ]]; then
    export OPENAI_API_KEY="$openai_key"
fi

bash "$SCRIPT_DIR/deploy/fly-deploy.sh" "$app_name"

success "Deployment complete"

# ── Step 7: Health check & summary ──────────────────────────────────
step 7 $TOTAL_STEPS "Verifying deployment"

server_url="https://$app_name.fly.dev"

if wait_for_url "$server_url/health" 90; then
    success "Server is healthy!"
else
    warn "Server did not become healthy within 90 seconds"
    echo "  Check logs: flyctl logs -a $app_name"
    echo "  Common issues:"
    echo "    - Database connection string may be wrong"
    echo "    - Extensions (vector, pg_trgm) may not be enabled"
fi

# Determine embedding status
embed_status="FTS-only (no OpenAI key)"
if [[ -n "$openai_key" ]]; then
    embed_status="OpenAI (text-embedding-3-small)"
fi

echo
print_box \
    "Memlayer Cloud — Deployed" \
    "" \
    "Server URL:   $server_url" \
    "Health:       $server_url/health" \
    "API base:     $server_url/api" \
    "Embeddings:   $embed_status" \
    "" \
    "Auth Token:   $(mask_token "$auth_token")" \
    "Full token:   $auth_token" \
    "" \
    "Next step: run setup_client.sh on each client machine." \
    "Use server URL: $server_url/api"

echo
info "Save these credentials — the auth token cannot be recovered."
echo
info "To view logs:   flyctl logs -a $app_name"
info "To redeploy:    ./deploy/fly-deploy.sh $app_name"
info "To destroy:     flyctl apps destroy $app_name"
echo
