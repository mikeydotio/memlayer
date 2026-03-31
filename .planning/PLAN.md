# Implementation Plan: Issues #37, #38, #39, #40

**Created:** 2026-03-28
**Issues:** #37 (recent command), #38 (truncate content), #39 (default types), #40 (skill docs)
**Scope:** 4 GitHub issues across CLI (Rust), server (Python), shared types (Rust), and documentation

---

## Architecture Overview

```
CLI (cli-rs/src/)          Shared (memlayer-common/src/)       Server (server/src/)
  commands/*.rs       <-->    api_types.rs                <-->   routes/*.py
  format/*.rs                 client.rs                          models.py
                              config.rs
Plugin (plugin/skills/memory/)
  SKILL.md
Scripts (scripts/)
  memlayer.claudemd.template
```

---

## Dependency Graph

```
#39 (default types)  ──────────────────────────────┐
                                                    ├──> #40 (skill docs)
#38 (truncate content) ───────────────────────────┤
                                                    │
#37 (recent command) ──── independent but benefits ─┘
                           from #38/#39 context
```

- #39 is purely CLI-side (default value + new flag). No server or type changes.
- #38 spans server (truncation logic) + types (new fields) + CLI (new --full flag + formatter updates).
- #37 is a new CLI command that reuses the existing `GET /api/sessions` endpoint and existing `SessionsPage` types. No server changes.
- #40 is documentation-only but should reflect the new defaults and commands from #38, #39, and #37.

---

## Wave Structure

### Wave 1 -- Independent, No Cross-Cutting Changes
Tasks that touch non-overlapping files and can be executed in parallel.

### Wave 2 -- Integration and Documentation
Tasks that depend on Wave 1 outcomes being known (docs that reference new flags/commands).

---

## Task Breakdown

### Wave 1 -- PARALLEL (3 independent tasks)

#### Task 1.1: Default to `--types user,assistant` in search (#39)

**Rationale:** Smallest change, purely CLI-side, no server or type changes. De-risks the most common UX complaint.

**Files to modify:**
- `cli-rs/src/commands/search.rs` -- change `types` arg default, add `--all-types` flag
- `cli-rs/src/commands/session.rs` -- change `types` arg default, add `--all-types` flag (consistency)

**Implementation details:**
1. In `SearchArgs`, change the `types` field: remove `Option`, set `default_value = "user,assistant"`.
   - Actually, keep it as `Option<String>` but introduce logic: if `--all-types` is set, pass `None` (no filter). If `--types` is explicitly set, use that value. If neither, default to `"user,assistant"`.
   - The cleanest approach: keep `types: Option<String>` but add a boolean `--all-types` flag. In `run()`, resolve: if `all_types` then `None`, else if `types.is_some()` use it, else default to `Some("user,assistant".to_string())`.
2. Same pattern in `SessionArgs` for consistency.
3. Verify the `SearchRequest` struct in `api_types.rs` already accepts `Option<Vec<String>>` for types -- it does, so `None` means "all types" at the server level. No server changes needed.

**Acceptance criteria:**
- [ ] `memlayer search "query"` only returns user and assistant entries (no tool_use/tool_result)
- [ ] `memlayer search "query" --all-types` returns all entry types including tool_use/tool_result
- [ ] `memlayer search "query" --types tool_use` returns only tool_use entries (explicit override works)
- [ ] `memlayer session <id>` only returns user and assistant entries by default
- [ ] `memlayer session <id> --all-types` returns all entry types
- [ ] `cargo build --workspace` compiles without warnings
- [ ] `cargo test --workspace` passes

**Risks:**
- Breaking change for scripts that expect tool_use/tool_result in default output. Mitigated by `--all-types` escape hatch.
- The session command currently has no default for types either; applying the same default there is a judgment call for consistency. If controversial, can scope to search only.

---

#### Task 1.2: Truncate content in search results (#38)

**Rationale:** Requires changes across three layers (server, types, CLI) but is self-contained and does not conflict with #39 or #37.

**Files to modify:**
- `server/src/models.py` -- add `content_truncated: bool` and `content_length: int` fields to `SearchResult`
- `server/src/routes/search.py` -- add `truncate` query param (default `true`), truncate `raw_content` to 200 chars when enabled, populate new fields
- `memlayer-common/src/api_types.rs` -- add `content_truncated: bool` and `content_length: i64` fields to `SearchResult` (with `#[serde(default)]` for backward compat)
- `cli-rs/src/commands/search.rs` -- add `--full` flag that sends `truncate=false` to the server
- `cli-rs/src/format/search.rs` -- show truncation indicator in text format when content is truncated (e.g., `[...truncated, 1247 chars total]`)
- `memlayer-common/src/api_types.rs` -- add `truncate` field to `SearchRequest` (Option<bool>)

**Implementation details:**

_Server side (`server/src/routes/search.py`):_
1. Add a `truncate: bool = True` field to the Python `SearchRequest` model (in `server/src/models.py`).
2. After building the `results` list, if `truncate` is true, for each result:
   - Store original length in `content_length`
   - If `len(raw_content) > 200`: set `raw_content = raw_content[:200]`, `content_truncated = True`
   - Else: `content_truncated = False`
3. The 200-char limit should be a constant `TRUNCATION_LIMIT = 200` at the top of the route file.

_Rust types (`memlayer-common/src/api_types.rs`):_
1. Add to `SearchResult`:
   ```rust
   #[serde(default)]
   pub content_truncated: bool,
   #[serde(default)]
   pub content_length: i64,
   ```
2. Add to `SearchRequest`:
   ```rust
   #[serde(skip_serializing_if = "Option::is_none")]
   pub truncate: Option<bool>,
   ```

_CLI side (`cli-rs/src/commands/search.rs`):_
1. Add `--full` flag (boolean, default false).
2. When constructing `SearchRequest`, set `truncate: if args.full { Some(false) } else { None }` (None lets server use its default of true).

_Formatter (`cli-rs/src/format/search.rs`):_
1. In JSON output: include `content_truncated` and `content_length` fields.
2. In text output: if `content_truncated` is true, append `\n[...truncated, {content_length} chars total]` after the content.

**Acceptance criteria:**
- [ ] `memlayer search "query"` returns results where raw_content is at most 200 chars
- [ ] Results that were truncated have `content_truncated: true` and `content_length` showing original size
- [ ] `memlayer search "query" --full` returns full untruncated content
- [ ] Short content (<= 200 chars) is not truncated and shows `content_truncated: false`
- [ ] Text format shows `[...truncated, N chars total]` indicator for truncated results
- [ ] JSON format includes `content_truncated` and `content_length` fields
- [ ] `cargo build --workspace` compiles without warnings
- [ ] Server starts without errors after model changes
- [ ] Backward compatibility: old clients that do not send `truncate` field get truncated results (server default)

**Risks:**
- The `SearchRequest` model change requires the server Python model and the Rust type to stay in sync. The `truncate` field uses `default=True` on server and `Option` on client, so missing field = truncation on.
- The `content_truncated` and `content_length` fields use `#[serde(default)]` so old server responses without these fields will deserialize as `false`/`0` -- safe.
- The large response offload path (`_maybe_offload`) should measure size BEFORE truncation, since truncation reduces the response size. Need to check: offload decision happens after result construction. With truncation, responses will be smaller, reducing offload frequency. This is acceptable behavior.

---

#### Task 1.3: `memlayer recent` command (#37)

**Rationale:** New command, touches new files only (no modification of existing command files). Reuses existing `get_sessions` client method and `SessionsPage`/`SessionInfo` types.

**Files to create:**
- `cli-rs/src/commands/recent.rs` -- new command module
- `cli-rs/src/format/recent.rs` -- new formatter module

**Files to modify:**
- `cli-rs/src/commands/mod.rs` -- add `Recent` variant to `Commands` enum
- `cli-rs/src/format/mod.rs` -- add `pub mod recent;`

**Implementation details:**

_Command (`cli-rs/src/commands/recent.rs`):_
1. Define `RecentArgs`:
   ```rust
   #[derive(Args)]
   pub struct RecentArgs {
       /// Max sessions to show (1-50)
       #[arg(long, default_value = "10")]
       limit: u32,
       /// Filter to project path
       #[arg(long)]
       project: Option<String>,
       /// Output format: json or text
       #[arg(long, default_value = "json")]
       format: String,
   }
   ```
2. `run()` function: load config, create client, call `client.get_sessions(project.as_deref(), 0, limit)`, format and print.

_Formatter (`cli-rs/src/format/recent.rs`):_
1. `format_recent_json(page: &SessionsPage) -> String`: JSON array of sessions with selected fields.
2. `format_recent_text(page: &SessionsPage) -> String`: Human-readable table/list showing session_id (truncated), slug, last_seen_at (relative time like "2h ago"), entry_count, and project info if available.

_Registration (`cli-rs/src/commands/mod.rs`):_
1. Add `pub mod recent;`
2. Add variant: `/// List recent sessions without a search query` / `Recent(recent::RecentArgs)`
3. Add match arm: `Commands::Recent(args) => recent::run(args).await`

**Acceptance criteria:**
- [ ] `memlayer recent` lists up to 10 most recent sessions ordered by last_seen_at DESC
- [ ] `memlayer recent --limit 5` limits output to 5 sessions
- [ ] `memlayer recent --project /home/mikey/memlayer` filters to sessions from that project
- [ ] `memlayer recent --format text` shows human-readable output with session IDs, slugs, timestamps, and entry counts
- [ ] `memlayer recent --format json` shows machine-readable JSON output
- [ ] The command requires no search query (positional argument)
- [ ] `cargo build --workspace` compiles without warnings
- [ ] `cargo test --workspace` passes
- [ ] The `SessionsPage` and `SessionInfo` types are reused (no new API types needed)
- [ ] The existing `get_sessions` client method is reused (no new client methods needed)

**Risks:**
- The `GET /api/sessions` endpoint returns `SessionsPage` which includes `total`, `limit`, `offset`. The `recent` command only uses `offset=0`. If the user wants pagination later, this is extensible but not in scope.
- The `SessionInfo` struct has `slug: Option<String>` -- sessions without slugs need graceful display (show "unnamed" or similar).

---

### Wave 2 -- SEQUENTIAL (depends on Wave 1 completion)

#### Task 2.1: Update memory skill documentation (#40)

**Depends on:** Tasks 1.1, 1.2, 1.3 (needs to reference new flags, defaults, and commands)

**Files to modify:**
- `plugin/skills/memory/SKILL.md` -- major expansion
- `scripts/memlayer.claudemd.template` -- update command reference to include `memlayer recent`

**Implementation details:**

_SKILL.md updates:_
1. **Add `memlayer recent` command section** with usage examples and flag documentation.
2. **Update `memlayer search` section:**
   - Document that `--types` defaults to `user,assistant` (not all types)
   - Document the `--all-types` flag
   - Document that content is truncated to 200 chars by default
   - Document the `--full` flag for untruncated results
3. **Add "Search Strategy Guidance" section** covering:
   - Keyword-rich queries vs. natural language
   - When to use `--project` to scope results
   - Temporal cue handling: "yesterday" -> use `--after`, "last week" -> use `--after`/`--before`
   - Common mistakes: overly broad queries, not filtering by project, expecting exact string match
   - When to drill into a session vs. searching again
   - Using `memlayer recent` as a starting point for temporal browsing (e.g., "What was I working on yesterday?")
4. **Update "Usage Pattern" section** to incorporate `recent` as step 0 for temporal exploration.

_CLAUDEMD template updates:_
1. Add `memlayer recent` to the command list.
2. Note the default type filtering behavior.

**Acceptance criteria:**
- [ ] SKILL.md documents the `memlayer recent` command with all flags
- [ ] SKILL.md documents the `--all-types` flag on search and session
- [ ] SKILL.md documents the `--full` flag on search
- [ ] SKILL.md notes that search results are truncated by default (200 chars)
- [ ] SKILL.md notes that default types are user,assistant
- [ ] SKILL.md contains a "Search Strategy Guidance" section with at least 5 concrete heuristics
- [ ] SKILL.md contains common mistakes section
- [ ] SKILL.md contains temporal cue handling guidance
- [ ] `memlayer.claudemd.template` lists `memlayer recent` command
- [ ] All command examples in SKILL.md are syntactically correct (could be copy-pasted and run)
- [ ] The skill description (YAML frontmatter) trigger keywords include "recent", "latest", "what was I working on"

**Risks:**
- If Wave 1 tasks deviate from this plan (e.g., different flag names), the docs will need adjustment. Mitigated by writing docs after Wave 1 is verified.

---

## Risk Register

| ID | Risk | Severity | Likelihood | Mitigation |
|----|------|----------|------------|------------|
| R1 | Default types change (#39) breaks existing scripts or agent behavior | Medium | Low | `--all-types` escape hatch; skill docs explain the change |
| R2 | Server truncation logic interacts with large response offload measurement | Low | Medium | Measure offload size BEFORE truncation; verify in code review |
| R3 | `SearchResult` type changes (#38) cause deserialization failures | Medium | Low | Use `#[serde(default)]` on new fields; Python model uses `Optional` with defaults |
| R4 | `recent` command name conflicts with future subcommand plans | Low | Low | `recent` is a clear, unambiguous name; no known conflicts |
| R5 | Skill doc changes are too verbose and Claude ignores them | Medium | Medium | Keep guidance concise with bullet points, not prose paragraphs |
| R6 | Python `SearchRequest` model change requires API version bump | Low | Low | Field has a default value; old clients without the field get truncation (safe default) |
| R7 | Truncation at 200 chars cuts mid-word or mid-tag | Low | High | Accept this for now; a smarter truncation (word boundary) is a follow-up |

---

## Verification Steps

### Per-Task Verification

Each task should be verified with these checks before marking complete:

**Build verification:**
```bash
cargo build --workspace 2>&1 | grep -E "error|warning"
cargo test --workspace
```

**Server verification (for #38 only):**
```bash
cd /home/mikey/memlayer && docker compose up server --build -d
# Test truncation
curl -s -X POST http://localhost:8420/api/search \
  -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "test", "limit": 3}' | python3 -m json.tool | head -40
# Verify content_truncated and content_length fields present
# Test --full equivalent
curl -s -X POST http://localhost:8420/api/search \
  -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"query": "test", "limit": 3, "truncate": false}' | python3 -m json.tool | head -40
```

**CLI integration verification:**
```bash
# Build the CLI
cargo build -p memlayer-cli --release

# Test #39: default types
./target/release/memlayer search "test" --limit 3 --format json 2>/dev/null | \
  python3 -c "import sys,json; r=json.load(sys.stdin); types=set(x.get('content_type','') for x in r.get('results',[])); assert types <= {'user','assistant','text'}, f'Unexpected types: {types}'"

# Test #39: --all-types
./target/release/memlayer search "test" --limit 20 --all-types --format json 2>/dev/null | \
  python3 -c "import sys,json; r=json.load(sys.stdin); types=set(x.get('content_type','') for x in r.get('results',[])); print(f'Types found: {types}')"

# Test #38: truncation
./target/release/memlayer search "test" --limit 3 --format json 2>/dev/null | \
  python3 -c "import sys,json; r=json.load(sys.stdin); [print(len(x.get('content','')), x.get('content_truncated')) for x in r.get('results',[])]"

# Test #38: --full
./target/release/memlayer search "test" --limit 3 --full --format json 2>/dev/null | \
  python3 -c "import sys,json; r=json.load(sys.stdin); [print(len(x.get('content','')), x.get('content_truncated')) for x in r.get('results',[])]"

# Test #37: recent command
./target/release/memlayer recent --limit 5 --format json
./target/release/memlayer recent --limit 5 --format text
./target/release/memlayer recent --project /home/mikey/memlayer --format text
```

### Cross-Task Integration Verification

After both waves are complete:
1. Verify `memlayer search` with default types AND truncation active simultaneously
2. Verify `memlayer recent` followed by `memlayer session <id>` workflow
3. Verify the skill doc examples all execute without errors
4. Verify `memlayer --help` shows the new `recent` subcommand
5. Verify `memlayer search --help` shows `--full` and `--all-types` flags

---

## Reviewer Perspectives

### UX Designer Concerns
- The `--all-types` flag name is clear and discoverable via `--help`
- The `--full` flag follows the convention of "more detail" flags (like `--verbose`)
- Truncation at 200 chars is enough to identify relevance without noise
- The `recent` command fills the "I want to browse, not search" gap
- Text format for `recent` should show relative timestamps ("2h ago") not raw ISO strings

### Senior Engineer Concerns
- Serde `#[serde(default)]` ensures backward compatibility when new fields are added to `SearchResult`
- The truncation happens server-side to save bandwidth, not just client-side for display
- No database migrations required for any of these changes
- The `truncate` field in `SearchRequest` has a safe default (true) so old clients get the new behavior
- The `recent` command reuses existing API and types -- no new endpoints or types needed

### Code Reviewer Concerns
- Verify `--all-types` and `--types` are mutually exclusive (use clap's `conflicts_with`)
- Verify truncation happens AFTER query execution but BEFORE response serialization
- Verify `content_length` is captured from the original content, not the truncated content
- Check that the `SearchRequest` Rust struct and Python model stay in sync
- Ensure `format_recent_text` handles `None` slugs gracefully

### QA Engineer Concerns
- Test edge cases: empty search results with truncation, session with 0 entries in recent
- Test that `--types user --all-types` produces a clear error (not silent precedence)
- Test `memlayer recent --limit 0` and `--limit 51` for validation boundaries
- Test content exactly 200 chars (boundary: should NOT be marked truncated)
- Test content of 201 chars (boundary: should be marked truncated, 200 chars shown)
- Test that `memlayer recent` with no sessions in DB returns clean empty output

---

## File Impact Matrix

| File | #37 | #38 | #39 | #40 |
|------|-----|-----|-----|-----|
| `cli-rs/src/commands/mod.rs` | M | | | |
| `cli-rs/src/commands/search.rs` | | M | M | |
| `cli-rs/src/commands/session.rs` | | | M | |
| `cli-rs/src/commands/recent.rs` | C | | | |
| `cli-rs/src/format/mod.rs` | M | | | |
| `cli-rs/src/format/search.rs` | | M | | |
| `cli-rs/src/format/recent.rs` | C | | | |
| `memlayer-common/src/api_types.rs` | | M | | |
| `memlayer-common/src/client.rs` | | | | |
| `server/src/models.py` | | M | | |
| `server/src/routes/search.py` | | M | | |
| `plugin/skills/memory/SKILL.md` | | | | M |
| `scripts/memlayer.claudemd.template` | | | | M |

Legend: C = Create, M = Modify

**Conflict analysis:** The only file modified by more than one task is `cli-rs/src/commands/search.rs` (tasks 1.2 and 1.3). Since task 1.2 adds `--full` and task 1.3 adds `--all-types`, these are additive changes to different parts of the `SearchArgs` struct and `run()` function. They can be done in parallel without merge conflicts if implemented as separate PRs, or sequentially in the same branch.

Wait -- correcting: Task 1.2 modifies `search.rs` (adds `--full`) and Task 1.1 modifies `search.rs` (adds `--all-types`). Both touch the same file. However, they add different fields to the Args struct and different logic to `run()`, so merge conflicts are unlikely if done in separate branches. If done by the same agent in sequence, no issue at all.

---

## Progress

### Wave 1 -- COMPLETE
- [x] Task 1.1: Default `--types user,assistant` in search and session commands (#39)
  - Acceptance: `memlayer search "query"` excludes tool_use/tool_result by default; `--all-types` overrides
  - Depends on: nothing
  - Files: `cli-rs/src/commands/search.rs`, `cli-rs/src/commands/session.rs`
- [x] Task 1.2: Truncate content in search results (#38)
  - Acceptance: Search results truncated to 200 chars with metadata fields; `--full` returns untruncated
  - Depends on: nothing
  - Files: `server/src/models.py`, `server/src/routes/search.py`, `memlayer-common/src/api_types.rs`, `cli-rs/src/commands/search.rs`, `cli-rs/src/format/search.rs`
  - Note: Also updated `cli-rs/src/tui/tabs/search.rs` to add `truncate: None` to TUI SearchRequest constructions
- [x] Task 1.3: `memlayer recent` command (#37)
  - Acceptance: `memlayer recent` lists sessions by recency; supports `--limit`, `--project`, `--format`
  - Depends on: nothing
  - Files: `cli-rs/src/commands/recent.rs` (new), `cli-rs/src/format/recent.rs` (new), `cli-rs/src/commands/mod.rs`, `cli-rs/src/format/mod.rs`

### Wave 2 -- COMPLETE
- [x] Task 2.1: Update memory skill documentation (#40 + updates for #37/#38/#39)
  - Acceptance: SKILL.md documents all new commands/flags; contains search strategy guidance with 5+ heuristics
  - Depends on: Tasks 1.1, 1.2, 1.3
  - Files: `plugin/skills/memory/SKILL.md`, `scripts/memlayer.claudemd.template`

## Deviations Log
| Task | Original Plan | Actual | Reason |
|------|--------------|--------|--------|
| 1.2 | Only modify files listed in plan | Also updated `cli-rs/src/tui/tabs/search.rs` | TUI constructs `SearchRequest` directly; needed `truncate` field added |

## Requirement Traceability
| Requirement (Issue) | Task(s) | Status |
|---------------------|---------|--------|
| #37: `memlayer recent` command | Task 1.3 | done |
| #37: `--limit` flag | Task 1.3 | done |
| #37: `--project` filter | Task 1.3 | done |
| #37: `--format` flag | Task 1.3 | done |
| #38: Truncate raw_content to ~200 chars | Task 1.2 (server) | done |
| #38: `--full` flag for untruncated | Task 1.2 (CLI) | done |
| #38: `content_truncated` field | Task 1.2 (types) | done |
| #38: `content_length` field | Task 1.2 (types) | done |
| #39: Default types to user,assistant | Task 1.1 | done |
| #39: `--all-types` override flag | Task 1.1 | done |
| #40: Search strategy guidance | Task 2.1 | done |
| #40: Common mistakes | Task 2.1 | done |
| #40: Temporal cue handling | Task 2.1 | done |

## Resumption State
If interrupted, read this file and pick up from the first incomplete task in the current wave. All tasks in Wave 1 are independent and can be started in any order. Wave 2 requires all of Wave 1 to be complete.

**Key context for resumption:**
- Rust workspace builds with `cargo build --workspace`
- CLI binary is at `cli-rs/src/`, shared types at `memlayer-common/src/`
- Server is Python FastAPI at `server/src/`
- The `GET /api/sessions` endpoint already exists in `server/src/routes/browse.py` and is called by `client.get_sessions()` -- no server work needed for #37
- The `SearchRequest` and `SearchResult` types exist in both Rust (`api_types.rs`) and Python (`models.py`) -- changes for #38 must be mirrored in both
