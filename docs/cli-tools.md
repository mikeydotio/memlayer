# CLI Tools & Search

Memlayer provides a `memlayer` CLI binary that Claude Code calls via the Bash tool to recall past conversations.

## memlayer search

Search across all past Claude Code conversations using hybrid semantic + full-text search. Returns relevant conversation excerpts ranked by relevance.

```bash
memlayer search <query> [flags]
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `<query>` (positional) | string | required | Natural language search query |
| `--project` | string | - | Filter to project path |
| `--session-id` | UUID | - | Filter to specific session |
| `--limit` | number | 10 | Max results (1-50) |
| `--after` | ISO 8601 | - | Entries after timestamp |
| `--before` | ISO 8601 | - | Entries before timestamp |
| `--types` | comma-sep | - | user,assistant,tool_use,tool_result |
| `--format` | text\|json | json | Output format |

**Example:**

```bash
# Claude searches for a past fix
memlayer search "database connection pooling fix" --project /home/mikey/memlayer --limit 5

# With date range
memlayer search "deployment script" --after 2025-06-01T00:00:00Z --before 2025-07-01T00:00:00Z
```

Results include the session ID, project path, date, content type, relevance score, and raw conversation content for each match.

## memlayer session

Retrieve the full chronological conversation history for a specific session. Use after `memlayer search` returns interesting results to get complete context.

```bash
memlayer session <session-uuid> [flags]
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `<session_id>` (positional) | UUID | required | Session to retrieve |
| `--limit` | number | 200 | Max entries (1-500) |
| `--types` | comma-sep | - | Filter content types |
| `--format` | text\|json | json | Output format |

## memlayer read-file

Read a specific line range from a large response offloaded to file storage. When `memlayer search` or `memlayer session` returns a response exceeding the configured threshold, the full response is stored as a file and a structural index is returned instead. Use the index to identify which line ranges you need.

```bash
memlayer read-file <file-uuid> --start <n> --end <n> [flags]
```

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `<file_id>` (positional) | UUID | required | File ID from large_response |
| `--start` | number | required | Start line (1-indexed, inclusive) |
| `--end` | number | required | End line (1-indexed, inclusive) |
| `--format` | text\|json | json | Output format |

## memlayer status

Show server health and embedding status.

```bash
memlayer status [--format json|text]
```

**Typical workflow:**

1. Run `memlayer search` to find relevant conversations.
2. Run `memlayer session` with a result's `session_id` for full context.
3. If the response includes a `large_response` reference with a structural index, use `memlayer read-file` with the `file_id` and line ranges to read specific sections.
4. Present findings with session dates and project context.

## Search Filters

All filters can be combined in any combination.

### Date Filters

Restrict results to a time window using ISO 8601 timestamps:

```bash
# Everything after a date
memlayer search "deployment script" --after 2025-06-01T00:00:00Z

# Everything before a date
memlayer search "auth bug" --before 2025-03-15T00:00:00Z

# A specific date range
memlayer search "refactor" --after 2025-01-01T00:00:00Z --before 2025-02-01T00:00:00Z
```

### Type Filters

Filter by the kind of conversation entry:

| Type | What it captures |
|------|-----------------|
| `user` | Messages you sent to Claude |
| `assistant` | Claude's text responses |
| `tool_use` | Tool calls Claude made (file reads, edits, bash commands) |
| `tool_result` | Results returned from those tool calls |

```bash
# Only human and assistant messages (skip tool noise)
memlayer search "database schema" --types user,assistant

# Only tool results (find error outputs, command results)
memlayer search "npm install error" --types tool_result
```

### Project Filter

Restrict to conversations about a specific project:

```bash
memlayer search "API design" --project /home/user/my-api
```

The `--project` value must match exactly as it appears in the database. You can find project paths in search results.

### Session Filter

Look within a single known session:

```bash
memlayer search "the fix we applied" --session-id 550e8400-e29b-41d4-a716-446655440000
```

### Combining Filters

```bash
memlayer search "migration fix" \
  --project /home/user/my-api \
  --after 2025-06-01T00:00:00Z \
  --types user,assistant \
  --limit 5
```
