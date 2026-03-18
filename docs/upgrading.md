# Upgrading

## From v0.x to v1.0.0

Your conversation data is preserved — nothing is lost during the upgrade.

**Server:**

```bash
cd memlayer
git pull
./setup_server.sh
```

The setup script detects your existing `.env` and Docker containers. It offers to rebuild with the latest code. Database migrations are applied automatically on server startup.

**Client (installed via `install.sh`):**

```bash
curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash
```

**Client (installed via git clone):**

```bash
cd memlayer
git pull
./setup_client.sh
```

Re-running the setup scripts is safe and idempotent.

## What Changed in v1.4.0

- **Server migration** — Move your memlayer instance to a new host with zero downtime and zero data loss. See [migration.md](migration.md).
- **Ed25519-signed 449 redirects** — The daemon automatically follows server redirects during migration, queuing entries locally to guarantee no data is lost.
- **Migration auth** — Time-limited migration keys with SHA-256 hashing, automatic TTL extension after handshake, and stale cleanup.
- **Transfer worker** — Background pull-based transfer of entries, embeddings, and response files from source to destination.
- **Daemon credential provisioning** — After migration completes, the daemon automatically obtains new credentials from the destination server.

## What Changed in v1.0.0

- **Automatic schema migrations** with tracking — no manual SQL, no repeated migrations
- **Version compatibility** checks between daemon and server
- **`memlayer-server` CLI** with `status`, `backup`, `restore`, `verify`, and `forget` commands
- **Date range and message type filters** on search and session summary
- **Graceful shutdown** with queue flushing (daemon) and connection draining (server)
- **Structured JSON logging** option for production deployments
- **Enhanced health endpoint** with component-level status (database, embeddings)
- **Comprehensive test suite** — unit, integration, and end-to-end tests
