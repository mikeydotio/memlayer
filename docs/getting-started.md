# Getting Started (Self-Hosted)

Full installation guide for running Memlayer on your own hardware or VPS.

## Prerequisites

| Component | Requirements |
|-----------|-------------|
| Server | Docker and Docker Compose v2 |
| Client | Node.js 18+, npm |
| Optional | Rust toolchain (for building daemon from source) |
| Optional | Claude CLI (`npm install -g @anthropic-ai/claude-code`) for automatic CLAUDE.md setup |

## Server Setup

The server runs as two Docker containers: PostgreSQL 16 with pgvector for storage, and a FastAPI application for the API.

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
./setup_server.sh
```

The script runs five steps:

1. **Prerequisites** — Checks that Docker and Docker Compose v2 are installed.
2. **Configuration** — Generates a database password and auth token (or lets you provide your own). Optionally configures OpenAI for vector embeddings.
3. **Docker startup** — Runs `docker compose up -d` to start the database and server.
4. **Health check** — Waits up to 60 seconds for the server to respond at `http://localhost:8420/health`.
5. **Summary** — Displays the server URL and auth token.

If Tailscale is installed, the script offers to bind the server to your Tailscale IP for secure remote access.

Re-running is safe. The script detects existing installations and asks whether to keep or regenerate your configuration.

<details>
<summary>Manual server setup (without the script)</summary>

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
cp .env.example .env
```

Edit `.env` with your credentials:

```bash
# Required
POSTGRES_PASSWORD=your-secure-database-password
MEMLAYER_AUTH_TOKEN=your-secret-auth-token

# Optional — enables vector embeddings for semantic search
# Without this, search uses full-text search only (still works well)
OPENAI_API_KEY=sk-...
```

Start the containers:

```bash
docker compose up -d
```

Verify the server is running:

```bash
curl http://localhost:8420/health
# {"status":"ok","components":{"database":"ok","embeddings":"disabled (FTS-only)"}}
```

</details>

## Client Setup

The client consists of a Rust daemon (pre-built binaries available) and the `memlayer` CLI that Claude Code calls via the Bash tool.

```bash
./setup_client.sh
```

The script runs seven steps:

1. **Prerequisites** — Checks for Node.js 18+ and optionally Rust/Cargo.
2. **Daemon installation** — Downloads a pre-built binary or builds from source. Installs to `~/.local/bin/memlayer-daemon`.
3. **Server connection** — Auto-detects a local server or prompts for the server URL.
4. **Authentication** — Prompts for the auth token from server setup (or detects an existing one).
5. **Background service** — Installs a systemd service (Linux) or launchd agent (macOS) so the daemon starts automatically.
6. **CLI binary** — Builds the TypeScript CLI and installs `memlayer` to `~/.local/bin/`.
7. **CLAUDE.md** — Adds memory instructions to `~/.claude/CLAUDE.md` so Claude knows when and how to use the CLI.

Skip interactive prompts by passing arguments directly:

```bash
./setup_client.sh --server-url http://your-server:8420/api --auth-token your-token
```

<details>
<summary>Manual client setup (without the script)</summary>

**Build the daemon:**

```bash
cd daemon
cargo build --release
cp target/release/memlayer-daemon ~/.local/bin/
```

**Run the daemon:**

```bash
MEMLAYER_SERVER_URL="http://localhost:8420/api" \
MEMLAYER_AUTH_TOKEN="your-secret-token" \
~/.local/bin/memlayer-daemon
```

On first run, the daemon scans all existing JSONL files and ingests them. Subsequent runs pick up from where they left off using byte-offset cursors stored in `~/.local/share/memlayer/cursors.db`.

**Set up the daemon as a background service (Linux with systemd):**

```bash
# Create env file for secrets
mkdir -p ~/.config/memlayer
cat > ~/.config/memlayer/env << 'EOF'
MEMLAYER_AUTH_TOKEN=your-secret-token
MEMLAYER_SERVER_URL=http://localhost:8420/api
EOF
chmod 600 ~/.config/memlayer/env

# Create service file
mkdir -p ~/.config/systemd/user
cat > ~/.config/systemd/user/memlayer-daemon.service << 'EOF'
[Unit]
Description=Claude Memory Layer Daemon
After=network-online.target

[Service]
Type=simple
ExecStart=%h/.local/bin/memlayer-daemon
EnvironmentFile=%h/.config/memlayer/env
Environment=RUST_LOG=info
Restart=always
RestartSec=10

[Install]
WantedBy=default.target
EOF

systemctl --user daemon-reload
systemctl --user enable --now memlayer-daemon
```

**Build and install the CLI:**

```bash
cd cli
npm install
npx tsc

# Symlink to PATH
ln -sf "$(pwd)/dist/cli.js" ~/.local/bin/memlayer
chmod +x ~/.local/bin/memlayer
```

Verify the CLI works:

```bash
memlayer status
```

**Add memory instructions to CLAUDE.md:**

Add this to your global `~/.claude/CLAUDE.md`:

```markdown
## Memory (Cross-Session Recall)

The `memlayer` CLI provides commands to search past conversations:
- `memlayer search "<query>"` — hybrid search across all past Claude Code conversations
- `memlayer session <session-uuid>` — full chronological session history
- `memlayer read-file <file-uuid> --start <n> --end <n>` — read large response files

Use `memlayer search` when the user references past work, asks about prior
decisions, or encounters a problem that may have been solved before.
Use keyword-rich queries for best results.
```

</details>

## One-Liner Install

Client-only install that downloads everything automatically:

```bash
curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash
```

## Test It

Start a new Claude Code session and ask:

```
Do you remember what we worked on last week?
```

Claude should call `memlayer search` and return results from your conversation history.
