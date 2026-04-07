---
name: memlayer:recall
description: >-
  Use when the user wants to recall past conversations, remember previous work,
  look up prior decisions, search conversation history, browse recent sessions,
  or read large response files. Trigger on "remember", "recall", "past conversation",
  "previous session", "how did we", "why did we", "what was the approach",
  "look up", "what did we decide", "have we done this before", "earlier we",
  "last time", "when did we", "do you remember", "recent sessions",
  "what was I working on".
version: 2.5.0
---

# Recall - Cross-Session Memory Search

You have access to a searchable archive of all past Claude Code conversations across all projects. Use the `memlayer` CLI to recall previous work, decisions, solutions, and context.

## When to Use

Use recall tools when the user:
- References a past conversation ("Do you remember when we...", "How did we fix...")
- Asks about prior decisions ("Why did we choose...", "What was the approach for...")
- Needs context from another project ("In the agentsmith project, how did we...")
- Wants to avoid re-discovering something ("Look up how we configured...")
- References past work without explicitly asking to search
- Asks what they were working on recently or yesterday

Proactively search when:
- You encounter a problem that seems like it may have been solved before
- The user asks "why" something is implemented a certain way
- Starting work on a project that was worked on in prior sessions

## Commands

### memlayer recent

List recent sessions ordered by last activity. Use this for temporal browsing ("What was I working on yesterday?") or as a starting point before diving into a specific session.

```bash
memlayer recent --limit 10 --format text
memlayer recent --project /home/mikey/memlayer --format text
```

- `--limit <n>`: (optional) Max sessions to show, default 10
- `--project <path>`: (optional) Filter to a specific project
- `--format json|text`: (optional) Output format, default json

### memlayer search

Search across all past conversations using hybrid semantic + full-text search.

**Important defaults:**
- Results only include `user` and `assistant` entries (tool_use/tool_result are filtered out). Use `--all-types` to include everything.
- Content is truncated to 200 characters. Use `--full` for complete content.

```bash
memlayer search "how did we fix the pooling bug" --project /home/mikey/memlayer --limit 5
memlayer search "database migration" --all-types --full
memlayer search "auth middleware" --types tool_use --full
memlayer search "deployment" --after 2026-03-01 --before 2026-03-15
```

- `<query>` (positional): Natural language description of what to find (be specific)
- `--project <path>`: (optional) Limit to a specific project
- `--session-id <uuid>`: (optional) Limit to a specific session
- `--limit <n>`: (optional) Number of results, default 10
- `--after <iso8601>`: (optional) Entries after timestamp
- `--before <iso8601>`: (optional) Entries before timestamp
- `--types <types>`: (optional) Comma-separated: user,assistant,tool_use,tool_result (default: user,assistant)
- `--all-types`: (optional) Include all content types (overrides default filter)
- `--full`: (optional) Return full untruncated content
- `--format json|text`: (optional) Output format, default json

Note: `--types` and `--all-types` cannot be used together.

### memlayer session

Get chronological conversation history for a specific session.

**Important defaults:**
- Only `user` and `assistant` entries are shown by default. Use `--all-types` to include tool_use/tool_result.

```bash
memlayer session <session-uuid> --limit 200 --format text
memlayer session <session-uuid> --all-types
memlayer session <session-uuid> --types tool_use,tool_result
```

- `<session_id>` (positional): The session UUID to retrieve
- `--limit <n>`: (optional) Max entries, default 200
- `--types <types>`: (optional) Comma-separated type filter (default: user,assistant)
- `--all-types`: (optional) Include all content types
- `--format json|text`: (optional) Output format, default json

### memlayer read-file

Read specific line ranges from a large response file that was offloaded to storage. Use this after `memlayer search` or `memlayer session` returns a `large_response` reference with a structural index. The index tells you which line ranges contain the content you need.

```bash
memlayer read-file <file-uuid> --start 1 --end 50
```

- `<file_id>` (positional): The file UUID from the `large_response.file_id` field
- `--start <n>`: Start line number (1-indexed, inclusive)
- `--end <n>`: End line number (1-indexed, inclusive)
- `--format json|text`: (optional) Output format, default json

## Usage Pattern

1. **Browse first** -- If the user asks about recent work or "what was I working on", start with `memlayer recent`
2. **Search broadly** -- Use `memlayer search` with a keyword-rich query
3. **Drill into a session** -- If a result looks relevant, use `memlayer session` with its session_id for full context
4. **Read large responses** -- If results include a `large_response` reference, use `memlayer read-file` with the file_id and line ranges from the structural index
5. **Present findings** with session date and project context
6. If no results found, say so honestly -- do not fabricate memories

## Search Strategy Guidance

- **Use keyword-rich queries, not natural language**: "pooling bug fix connection timeout" works better than "how did we fix the pooling bug that was causing timeouts"
- **Scope with `--project`**: When you know which project the user is asking about, always pass `--project` to reduce noise
- **Convert temporal cues to date filters**: When the user says "yesterday" or "last week", calculate the date and use `--after` / `--before`
- **Start with `memlayer recent` for temporal browsing**: "What was I working on?" or "What did we do yesterday?" -- use `recent` first, then drill into specific sessions
- **Use `--full` only when you need complete content**: The default 200-char truncation is enough to identify relevance
- **Use `--all-types` when searching for tool output**: If the user asks about a specific command that was run, a file that was read, or build output, you need `--all-types` since tool_use and tool_result are excluded by default
- **Drill into a session rather than re-searching**: If you find a relevant result, use `memlayer session <session_id>` to get full context rather than crafting more search queries

## Common Mistakes

- **Too broad a query**: "database" returns too many results. Be specific: "database connection pool exhaustion fix"
- **Not filtering by project**: Without `--project`, results come from all projects, which adds noise
- **Expecting exact string match**: The search is semantic + full-text, not literal. It finds conceptually related entries.
- **Forgetting default type filter**: If you search for something and get no results, the content might be in tool_use/tool_result entries. Try `--all-types`.
- **Ignoring truncation**: If a result looks relevant but the content is cut off, re-run with `--full` to see the complete entry.
