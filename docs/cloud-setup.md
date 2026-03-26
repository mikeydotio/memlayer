# Cloud Setup (Supabase + Fly.io)

Deploy Memlayer to **Supabase** (database) + **Fly.io** (server) for a fully cloud-hosted stack. No Docker or VPS management needed.

**Cost:** Free tier covers 1-10 users. Supabase Pro ($25/mo) + Fly.io (~$2/mo) scales to hundreds.

## Prerequisites

- A [Supabase](https://supabase.com) account (free tier works)
- A [Fly.io](https://fly.io) account with **billing info added** (free tier works, but Fly requires a credit card on file before deploying machines — add it at [fly.io/dashboard](https://fly.io/dashboard) → Billing)
- The `flyctl` CLI installed and authenticated
- An OpenAI API key (optional, for vector embeddings)

## Step 1: Create a Supabase Project

1. Go to [supabase.com/dashboard](https://supabase.com/dashboard) and create a new project.
2. Choose a region close to where you'll deploy Fly.io.
3. Set a strong database password.
4. Wait for the project to finish provisioning.

### Enable Required Extensions

In your Supabase dashboard, go to **Database > Extensions** and enable:

- `vector` — pgvector for embedding similarity search
- `pg_trgm` — trigram indexes for full-text search

> Supabase installs these in its `extensions` schema rather than `public`. The Memlayer server detects this automatically — no extra configuration needed.

### Get the Connection String

Go to **Settings > Database > Connection string > URI**.

> **Important:** Use the **direct connection** (port 5432), not the pooled connection (port 6543). Memlayer uses asyncpg with prepared statements, which requires a direct PostgreSQL connection.

## Step 2: Run the Setup Script

```bash
git clone https://github.com/mikeydotio/memlayer.git
cd memlayer
./setup_cloud_hosted.sh
```

The script will:

1. Verify flyctl is installed and authenticated
2. Prompt for your Supabase connection string
3. Generate a secure auth token
4. Optionally validate your OpenAI API key
5. Configure your Fly.io app name and region
6. Deploy the server to Fly.io
7. Wait for the health check to pass

## Step 3: Install the Client

On each machine where you use Claude Code:

```bash
curl -fsSL https://raw.githubusercontent.com/mikeydotio/memlayer/main/install.sh | bash
```

When prompted, enter the Fly.io server URL and auth token from the setup output.

## Managing Your Deployment

### View logs

```bash
flyctl logs -a YOUR_APP_NAME
```

### Redeploy after updates

```bash
cd memlayer
git pull
./deploy/fly-deploy.sh YOUR_APP_NAME
```

### Update secrets

```bash
flyctl secrets set OPENAI_API_KEY=sk-... -a YOUR_APP_NAME
```

### Scale up

```bash
# Increase memory
flyctl scale memory 512 -a YOUR_APP_NAME

# Keep machine always running (no scale-to-zero)
flyctl scale count 1 -a YOUR_APP_NAME
```

## Re-running after failure

The setup script saves your configuration (auth token, database URL, app name) to `~/.config/memlayer/cloud.env`. If the script fails at any step, just run it again — it will detect your saved config and offer to reuse it, so you won't end up with mismatched tokens or duplicate resources.

```bash
# Re-run the full setup (reuses saved config)
./setup_cloud_hosted.sh

# Or retry just the deploy step directly
source ~/.config/memlayer/cloud.env
./deploy/fly-deploy.sh $MEMLAYER_APP_NAME
```

Common failure scenarios:

- **Billing not set up** → Add billing at [fly.io/dashboard](https://fly.io/dashboard) → Billing, then re-run
- **Extensions not enabled** → Enable `vector` and `pg_trgm` in Supabase, then re-run
- **Wrong connection string** → Re-run and choose "no" when asked to keep the existing string
- **Deploy failed mid-build** → Re-run (app/volume already exist and will be reused)

## Troubleshooting

### Health check fails

```bash
flyctl logs -a YOUR_APP_NAME
```

Common causes: wrong connection string, extensions not enabled, or Supabase project paused (free tier pauses after 1 week of inactivity).

### Prepared statement errors

You're using the pooled connection (port 6543) instead of direct (port 5432). Update the secret:

```bash
flyctl secrets set DATABASE_URL="postgresql://..." -a YOUR_APP_NAME
```

### Slow first request

Fly machines scale to zero after idle periods. The first request wakes the machine (~2-5 seconds). Set `min_machines_running = 1` in `fly.toml` to keep it always on (~$1.94/mo).

### Supabase project paused

Free tier Supabase projects pause after 1 week of inactivity. Go to your dashboard and click "Restore" to unpause. Upgrade to Pro ($25/mo) to prevent pausing.
