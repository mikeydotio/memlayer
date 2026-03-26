#!/usr/bin/env bash
# DigitalOcean guided cloud setup for Memlayer
# Interactive script that walks through deploying Memlayer on a DO droplet
set -euo pipefail

BOLD='\033[1m'
GREEN='\033[32m'
YELLOW='\033[33m'
CYAN='\033[36m'
RESET='\033[0m'

step() { printf "\n${BOLD}${CYAN}[Step %s]${RESET} %s\n" "$1" "$2"; }
info() { printf "${GREEN}→${RESET} %s\n" "$1"; }
warn() { printf "${YELLOW}⚠${RESET} %s\n" "$1"; }

echo ""
echo -e "${BOLD}Memlayer Cloud Setup — DigitalOcean${RESET}"
echo "This script guides you through deploying Memlayer on a DigitalOcean droplet."
echo ""

# ---------------------------------------------------------------------------
step "1/6" "Check prerequisites"

if ! command -v docker &>/dev/null; then
    warn "Docker not found. Install Docker first:"
    echo "  curl -fsSL https://get.docker.com | sh"
    echo ""
    echo "For a full walkthrough, see:"
    echo "  https://memlayer.io/docs/digitalocean-setup"
    exit 1
fi
info "Docker found: $(docker --version | head -1)"

if ! docker compose version &>/dev/null; then
    warn "Docker Compose v2 not found. Install it:"
    echo "  sudo apt install docker-compose-plugin"
    exit 1
fi
info "Docker Compose found: $(docker compose version --short)"

# ---------------------------------------------------------------------------
step "2/6" "Clone or update Memlayer"

if [ -d "memlayer" ]; then
    info "Found existing memlayer directory, updating..."
    cd memlayer
    git pull --ff-only
else
    info "Cloning memlayer..."
    git clone https://github.com/mikeydotio/memlayer.git
    cd memlayer
fi

# ---------------------------------------------------------------------------
step "3/6" "Generate credentials"

if [ -f .env ]; then
    warn "Existing .env found. Skipping credential generation."
    info "To regenerate: rm .env and re-run this script."
else
    POSTGRES_PASSWORD=$(openssl rand -hex 16)
    MEMLAYER_AUTH_TOKEN=$(openssl rand -hex 32)

    echo ""
    echo "Do you have an OpenAI API key for vector embeddings? (optional)"
    echo "Without it, Memlayer uses full-text search only."
    read -rp "OpenAI API key (press Enter to skip): " OPENAI_KEY

    cat > .env <<EOF
POSTGRES_PASSWORD=$POSTGRES_PASSWORD
MEMLAYER_AUTH_TOKEN=$MEMLAYER_AUTH_TOKEN
EMBEDDING_PROVIDER=openai
OPENAI_API_KEY=${OPENAI_KEY:-}
EOF

    info "Credentials saved to .env"
    echo ""
    echo -e "  ${BOLD}Auth token:${RESET} $MEMLAYER_AUTH_TOKEN"
    echo "  (Save this — you'll need it for client setup)"
fi

# ---------------------------------------------------------------------------
step "4/6" "Configure network binding"

DROPLET_IP=$(curl -sf http://169.254.169.254/metadata/v1/interfaces/public/0/ipv4/address 2>/dev/null || hostname -I | awk '{print $1}')
echo ""
echo "Detected IP: $DROPLET_IP"
echo ""
echo "How should Memlayer listen?"
echo "  1) Public (0.0.0.0) — accessible from any network"
echo "  2) This IP only ($DROPLET_IP) — accessible from internet but explicit"
echo "  3) Localhost only (127.0.0.1) — requires SSH tunnel or Tailscale"
echo ""
read -rp "Choice [1]: " BIND_CHOICE

case "${BIND_CHOICE:-1}" in
    2) BIND_ADDR="$DROPLET_IP" ;;
    3) BIND_ADDR="127.0.0.1" ;;
    *) BIND_ADDR="0.0.0.0" ;;
esac

if ! grep -q MEMLAYER_BIND_ADDR .env 2>/dev/null; then
    echo "MEMLAYER_BIND_ADDR=$BIND_ADDR" >> .env
fi
info "Binding to $BIND_ADDR:8420"

# ---------------------------------------------------------------------------
step "5/6" "Start Memlayer"

info "Building and starting Docker stack..."
docker compose up -d --build

echo ""
info "Waiting for health check..."
for i in $(seq 1 30); do
    if curl -sf "http://localhost:8420/health" | grep -q '"status":"ok"' 2>/dev/null; then
        info "Server is healthy!"
        break
    fi
    sleep 2
done

# ---------------------------------------------------------------------------
step "6/6" "Next steps"

SERVER_URL="http://${DROPLET_IP}:8420/api"
AUTH_TOKEN=$(grep MEMLAYER_AUTH_TOKEN .env | cut -d= -f2)

echo ""
echo -e "${BOLD}${GREEN}Memlayer is running!${RESET}"
echo ""
echo "Server URL: $SERVER_URL"
echo ""
echo "To set up Memlayer for Claude Code on your local machine, run:"
echo ""
echo "  curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash -s -- --server-url $SERVER_URL --auth-token $AUTH_TOKEN"
echo ""
echo "For detailed setup instructions, see:"
echo "  https://memlayer.io/docs/digitalocean-setup"
echo ""
