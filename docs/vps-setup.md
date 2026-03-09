# Generic VPS Setup

Deploy Memlayer on any VPS with Docker. This guide works for any Linux VPS
(DigitalOcean, Linode, Hetzner, Vultr, AWS EC2, etc.).

## Prerequisites

- Linux VPS with 1+ GB RAM and Docker installed
- Docker Compose v2+ (`docker compose` command)
- A domain or IP address for your server

## Quick Start

```bash
# 1. Clone the repository
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer

# 2. Generate credentials
POSTGRES_PASSWORD=$(openssl rand -hex 16)
MEMLAYER_AUTH_TOKEN=$(openssl rand -hex 32)

# 3. Create .env file
cat > .env <<EOF
POSTGRES_PASSWORD=$POSTGRES_PASSWORD
MEMLAYER_AUTH_TOKEN=$MEMLAYER_AUTH_TOKEN
EMBEDDING_PROVIDER=openai
OPENAI_API_KEY=sk-...your-key-here...
EOF

# 4. Start the stack
docker compose up -d

# 5. Verify
curl http://localhost:8420/health
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `POSTGRES_PASSWORD` | Yes | Database password |
| `MEMLAYER_AUTH_TOKEN` | Yes | Bearer token for API auth |
| `EMBEDDING_PROVIDER` | No | `openai` (default) or `ollama` |
| `OPENAI_API_KEY` | No | Required if using OpenAI embeddings |
| `MEMLAYER_BIND_ADDR` | No | Bind address (default: `0.0.0.0`). Set to your Tailscale IP for private access |
| `RESPONSE_BUDGET_BYTES` | No | Response size budget (default: 200000 = 200KB) |

For the full list, see the [README environment variables section](../README.md#server-variables).

## Client Setup

On each machine that runs Claude Code:

```bash
curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash
```

When prompted, enter:
- **Server URL:** `http://YOUR_VPS_IP:8420/api`
- **Auth token:** The `MEMLAYER_AUTH_TOKEN` from your `.env`

## Security Notes

- The server listens on port 8420. Use a firewall to restrict access.
- For private networks, set `MEMLAYER_BIND_ADDR` to your Tailscale/VPN IP.
- All API requests require the bearer token.
- Database is only accessible within the Docker network (not exposed externally by default).

## Updating

```bash
cd memlayer
git pull
docker compose up -d --build
```

## Backup & Restore

```bash
# Backup
./scripts/memlayer.sh backup

# Restore
./scripts/memlayer.sh restore backup-YYYY-MM-DD.tar.gz
```
