# Implementation Progress

Session recovery file. Resume from the last unchecked item.

---

## v1.0.1 — Polish & Test Coverage — COMPLETE

All waves shipped. All 7 issues closed. Tagged v1.0.1.

### Wave 1 — COMPLETE
- [x] #4 CLAUDE.md template (ddd5463)
- [x] #5 Debounce map leak (4ed88d3)
- [x] #3 Response budget 200KB (cdb900d)
- [x] #2 File cache eviction (d470d6a)

### Wave 2 — COMPLETE
- [x] #29 Response analytics (6843b17)

### Wave 3 — COMPLETE
- [x] #6 MCP unit tests — 15 tests (431c241)

### Wave 4 — COMPLETE
- [x] #7 E2E integration test — 9 tests, isolated docker-compose.test.yml (9b3a3e3)
- [x] Bonus: Fixed auth middleware 500→401 bug

### Release — COMPLETE
- [x] All v1.0.1 issues closed on GitHub
- [x] Version strings bumped: server, daemon, MCP
- [x] Tagged v1.0.1

### Test Summary
- Daemon: 63 tests (cargo test)
- Server: 110 tests (pytest)
- MCP: 15 tests (vitest)
- E2E: 9 tests (docker-compose.test.yml + e2e.sh)
- **Total: 197 tests**

---

## v1.1.0 — Performance & Observability — NOT STARTED

### Dependency Graph
```
Wave 1 (parallel, no deps):
  #8   Batch INSERT (executemany + fallback)
  #9   Embedding progress endpoint
  #10  Embedding progress in health endpoint
  #27  Ollama integration tests (real Docker Compose + nomic-embed-text)

Wave 2 (depends on nothing, but logically after file endpoints exist):
  #28  Line-range endpoint (GET /api/files/{id}/lines)
  #30  Request pattern analytics

Wave 3 (depends on all above):
  Run full test suite, tag v1.1.0
```

### Issues
- [ ] #8 Batch INSERT for ingest (executemany + one-at-a-time fallback)
- [ ] #9 Embedding progress endpoint (GET /api/embeddings/status)
- [ ] #10 Embedding progress in health endpoint
- [ ] #27 Ollama integration tests (real Ollama in Docker Compose test stack)
- [ ] #28 Line-range endpoint (GET /api/files/{id}/lines?start=N&end=M)
- [ ] #30 Request pattern analytics (line-range vs file download)

---

## Commits Log

| Hash | Issue | Description | Pushed |
|------|-------|-------------|--------|
| a125195 | — | Roadmap rewrite from interview | Yes |
| 9ec2826 | — | Verification complete | Yes |
| 1846c65 | — | Rename progress file | Yes |
| ddd5463 | #4 | CLAUDE.md template: add read_memory_file docs | Yes |
| 4ed88d3 | #5 | Debounce map: prune stale entries every 60s | Yes |
| cdb900d | #3 | 200KB response budget with file-based overflow | Yes |
| d470d6a | #2 | File cache eviction: 50MB soft / 100MB hard, FIFO | Yes |
| 6843b17 | #29 | Server-side response size analytics | Yes |
| 431c241 | #6 | MCP unit tests: file-cache + api-client (15 tests) | Yes |
| 9b3a3e3 | #7 | E2E test + auth middleware fix (9 tests) | Yes |
