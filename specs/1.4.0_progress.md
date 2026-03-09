# Roadmap & GitHub Milestones Update — Progress Tracker

This file tracks granular progress for the roadmap interview → implementation session.
If the session is interrupted, resume from the last unchecked item.

## Context

Updating roadmap and GitHub milestones based on a thorough interview covering v1.0.1 through v1.4.0+.
Key changes: revised cloud strategy, new server web UI scope, Astro website, deferred multi-tenancy.

---

## Phase 1: Update `specs/roadmap.md`

- [x] Rewrite v1.0.1 (file cache 50MB FIFO, 200KB response budget with file-based flow, analytics)
- [x] Rewrite v1.1.0 (executemany + fallback, Ollama real testing, line-range endpoint, analytics)
- [x] Rewrite v1.2.0 → "Server Web UI & Cloud Deployment" (web UI + DO/generic VPS)
- [x] Rewrite v1.3.0 → "Public Website & Launch" (Astro/Cloudflare, no billing)
- [x] Rewrite v1.4.0 → "Multi-Tenancy & Hosted Service" (everything deferred)
- [x] Add Triage item: non-DO VPS provider docs

## Phase 2: Update GitHub Milestones

- [x] Milestone 1 (v1.0.1): description updated
- [x] Milestone 2 (v1.1.0): description updated
- [x] Milestone 3: renamed to "v1.2.0 — Server Web UI & Cloud Deployment"
- [x] Milestone 4: renamed to "v1.4.0 — Multi-Tenancy & Hosted Service"
- [x] Milestone 5: renamed to "v1.3.0 — Public Website & Launch"

## Phase 3: Close Obsolete Issues

- [x] #11 Supabase compatibility — closed
- [x] #12 AWS ECS/Fargate — closed
- [x] #14 IaC templates — closed

## Phase 4: Update Existing Issues

- [x] #3 → "MCP response budget: 200KB cap with file-based overflow"
- [x] #8 → body updated with executemany + fallback approach
- [x] #13 → "DigitalOcean guided cloud setup (setup_cloud.sh)"
- [x] #17 → "Application-level tenant filtering" (not Postgres RLS)
- [x] #21 → "Astro site scaffolding + Cloudflare Pages deployment"
- [x] #23 → "Web dashboard for hosted users"
- [x] #27 → "Ollama local embedding support with integration tests"
- [x] #28 → "Line-range endpoint for file downloads"

## Phase 5: Move Issues Between Milestones

- [x] #22 (Stripe billing) → milestone 4 (v1.4.0)
- [x] #23 (Web dashboard) → milestone 4 (v1.4.0)
- [x] #24 (One-liner install) → milestone 4 (v1.4.0)
- [x] #25 (Privacy policy) → milestone 4 (v1.4.0)
- [x] #26 (Status page) → milestone 4 (v1.4.0)
- [x] #21 stays in milestone 5 (now v1.3.0)

## Phase 6: Create New Issues

- [x] #29 Server-side response size analytics (v1.0.1)
- [x] #30 Request pattern analytics — line-range vs file download (v1.1.0)
- [x] #31 Server web UI — setup wizard, config, dashboard, analytics (v1.2.0)
- [x] #32 Generic VPS setup documentation (v1.2.0)
- [x] #33 Download page with dual-path options (v1.3.0)
- [x] #34 Cloud onboarding tutorial sub-pages (v1.3.0)
- [x] #35 Hosted/reseller model — "You host it for me" (v1.4.0)
- [x] #36 Documentation for non-DO VPS providers (Triage, no milestone)

## Phase 7: Verification

- [x] Verify milestones via `gh api` — all 5 milestones correct
- [x] Verify issue assignments via `gh issue list` — all 34 open issues in correct milestones
- [x] Confirm roadmap.md matches GitHub state — confirmed

## Commits

| Commit | Contents | Pushed? |
|--------|----------|---------|
| a125195 | roadmap.md rewrite + this progress file | Yes |
| (pending) | verification complete update | No |

---

## Final Milestone Summary (expected)

| Milestone | Issues (open) |
|-----------|---------------|
| v1.0.1 — Polish & Test Coverage | #2, #3, #4, #5, #6, #7, #29 |
| v1.1.0 — Performance & Observability | #8, #9, #10, #27, #28, #30 |
| v1.2.0 — Server Web UI & Cloud Deployment | #13, #31, #32 |
| v1.3.0 — Public Website & Launch | #21, #33, #34 |
| v1.4.0 — Multi-Tenancy & Hosted Service | #15, #16, #17, #18, #19, #20, #22, #23, #24, #25, #26, #35 |
| Triage (no milestone) | #1, #36 |
