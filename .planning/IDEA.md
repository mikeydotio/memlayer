# Memlayer: Fly.io to Brainframe Migration with Validated Deployment Pipeline

## Vision
Migrate the memlayer server from Fly.io (which is crash-looping) to the local brainframe server, with a CI-style deployment pipeline that validates every build before it touches production. Pushes from agentsmith containers trigger brainframe to checkout, test, and blue-green deploy — preventing the class of startup failures that took down Fly.io.

## Problem Statement
The current Fly.io deployment is fragile — v2.1.0 crashed due to non-root user + missing pg_dump, and the feedback loop for diagnosing failures on Fly.io is slow and painful. Moving to brainframe gives direct control, and a dry-run pipeline ensures broken builds never reach production.

## Target Users
- Mikey (sole operator) — deploying from agentsmith containers
- The memlayer daemon (running on agentsmith containers) — sending data to the server

## Key Requirements
- [ ] Signal mechanism: git push from container → brainframe detects and acts
- [ ] Brainframe listener: receives signal, checks out new code, runs pipeline
- [ ] Ephemeral test stack: fresh PG + server container per dry-run, torn down after
- [ ] Realistic test data: seed with fake but representative sessions, entries, entities
- [ ] Full smoke test: health check → ingest test entries → poll embedding status until complete → search verifies results
- [ ] Embedding status polling: check `/health` embedding_progress every 30s until all test entries have embeddings
- [ ] Failure artifact: on test failure, create timestamped+versioned tarball of DB data, config, logs, then clean up
- [ ] Blue-green production deploy: start new container, health check, switch Tailnet hostname, remove old container
- [ ] Slack notification: post deploy result (success/failure with details) to webhook
- [ ] Separate test OpenAI API key for smoke test embeddings
- [ ] Data migration from Fly.io via existing server-to-server migration flow
- [ ] Production stack: docker-compose based, PG + server on brainframe

## Assumptions (Examined)
| Assumption | Challenged? | Status |
|-----------|------------|--------|
| Brainframe has Docker + Compose ready | Asked directly | Validated |
| Tailscale connectivity between container and brainframe | Established by agentsmith infra | Validated |
| Daemon offline queue handles brief downtime during deploy | Known from code review | Validated |
| Server-to-server migration flow works for data transfer | User confirmed it exists | Needs verification |
| `/health` endpoint has embedding_progress data | Verified in code | Validated |
| Ephemeral PG container starts fast enough for CI feel | Typical PG startup ~2-3s | Validated |

## Constraints
- Signal mechanism should be lightweight (no webhook server needed on brainframe)
- Test OpenAI key separate from production
- Slack webhook URL stored as secret on brainframe
- Pipeline runs on brainframe directly (not in container)
- No orphaned containers — always clean up, even on failure (tarball first)

## What "Done" Looks Like
1. Push to main from this container → brainframe pulls, tests, deploys within ~3 minutes
2. If dry-run fails: Slack alert with failure details, tarball artifact saved, no production impact
3. If dry-run passes: blue-green deploy, health-gated traffic switch, Slack success alert
4. Production memlayer accessible on Tailnet from all agentsmith containers
5. Existing data migrated from Fly.io to brainframe's PG

## Open Questions
- What Tailnet hostname should production memlayer use on brainframe?
- Where should failure tarballs be stored on brainframe?
- What Slack webhook URL / channel to use?
- What's the brainframe Tailscale IP / hostname for the signal mechanism?
- Where is the production .env (secrets) stored on brainframe?

## Prior Art
- agentsmith deploy-web pattern: signal file + systemd path unit (proven pattern in this infra)
- GitHub Actions CI: already runs tests but doesn't deploy
- Fly.io deploy hook in post-bump: tried flyctl deploy, fragile
