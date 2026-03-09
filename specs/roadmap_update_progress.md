# Implementation Progress

Session recovery file. Resume from the last unchecked item.

---

## v1.0.1 — Polish & Test Coverage — COMPLETE (tagged v1.0.1)

- [x] #4 CLAUDE.md template, #5 debounce map, #3 response budget, #2 file cache
- [x] #29 analytics, #6 MCP tests (15), #7 E2E tests (9), auth middleware fix
- **197 tests** (63 daemon + 110 server + 15 MCP + 9 E2E)

---

## v1.1.0 — Performance & Observability — COMPLETE (tagged v1.1.0)

- [x] #8 Batch INSERT: unnest-based batch + one-at-a-time fallback (28331cb)
- [x] #9 Embedding progress endpoint: GET /api/embeddings/status (28331cb)
- [x] #10 Embedding progress in /health (28331cb)
- [x] #28 Line-range endpoint: GET /api/files/{id}/lines?start=N&end=M (28331cb)
- [x] #30 Request pattern analytics: file download vs line-range tracking (28331cb)
- [x] #27 Ollama integration tests: real Ollama in Docker Compose, nomic-embed-text (91e45ba)

### Release
- [x] All v1.1.0 issues closed on GitHub
- [x] Version strings bumped
- [x] Tagged v1.1.0

---

## v1.2.0 — Server Web UI & Cloud Deployment — NOT STARTED

### Issues
- [ ] #31 Server web UI (setup wizard, config, dashboard, analytics)
- [ ] #13 DigitalOcean guided cloud setup (setup_cloud.sh)
- [ ] #32 Generic VPS setup documentation

---

## v1.3.0 — Public Website & Launch — NOT STARTED

### Issues
- [ ] #21 Astro site scaffolding + Cloudflare Pages deployment
- [ ] #33 Download page with dual-path options
- [ ] #34 Cloud onboarding tutorial sub-pages

---

## v1.4.0 — Multi-Tenancy & Hosted Service — NOT STARTED

### Issues
- [ ] #15 GitHub OAuth, #16 API keys, #17 tenant filtering, #18 rate limiting
- [ ] #19 GDPR, #20 background job queue, #22 Stripe, #23 dashboard
- [ ] #24 one-liner install, #25 privacy policy, #26 status page, #35 hosted model

---

## Commits Log

| Hash | Issue | Description | Pushed |
|------|-------|-------------|--------|
| ddd5463 | #4 | CLAUDE.md template: add read_memory_file docs | Yes |
| 4ed88d3 | #5 | Debounce map: prune stale entries every 60s | Yes |
| cdb900d | #3 | 200KB response budget with file-based overflow | Yes |
| d470d6a | #2 | File cache eviction: 50MB soft / 100MB hard, FIFO | Yes |
| 6843b17 | #29 | Server-side response size analytics | Yes |
| 431c241 | #6 | MCP unit tests: file-cache + api-client (15 tests) | Yes |
| 9b3a3e3 | #7 | E2E test + auth middleware fix (9 tests) | Yes |
| 763c4c4 | — | v1.0.1 release: version bump + tag | Yes |
| 28331cb | #8,#9,#10,#28,#30 | Batch INSERT, embedding progress, line-range, analytics | Yes |
| 91e45ba | #27 | Ollama integration tests | Yes |
