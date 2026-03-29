---
name: memory
description: Use when the user wants to recall past conversations, remember previous work, look up prior decisions, search conversation history, or reference past sessions. Trigger keywords include "remember", "recall", "past conversation", "previous session", "how did we", "why did we", "what was the approach", "look up", "what did we decide", "have we done this before", "earlier we", "last time", "when did we", "do you remember".
version: 0.1.0
---

# Memory - Cross-Session Recall

You have access to a searchable archive of all past Claude Code conversations across all projects. Use the `memlayer` CLI to recall previous work, decisions, solutions, and context.

## When to Use

Use memory tools when the user:
- References a past conversation ("Do you remember when we...", "How did we fix...")
- Asks about prior decisions ("Why did we choose...", "What was the approach for...")
- Needs context from another project ("In the agentsmith project, how did we...")
- Wants to avoid re-discovering something ("Look up how we configured...")
- References past work without explicitly asking to search

Proactively search when:
- You encounter a problem that seems like it may have been solved before
- The user asks "why" something is implemented a certain way
- Starting work on a project that was worked on in prior sessions

## Commands

### memlayer search

Search across all past conversations using hybrid semantic + full-text search.

```bash
memlayer search "how did we fix the pooling bug" --project /home/mikey/memlayer --limit 5
```

- `<query>` (positional): Natural language description of what to find (be specific)
- `--project <path>`: (optional) Limit to a specific project
- `--session-id <uuid>`: (optional) Limit to a specific session
- `--limit <n>`: (optional) Number of results, default 10
- `--after <iso8601>`: (optional) Entries after timestamp
- `--before <iso8601>`: (optional) Entries before timestamp
- `--types <types>`: (optional) Comma-separated: user,assistant,tool_use,tool_result
- `--format json|text`: (optional) Output format, default json

### memlayer session

Get chronological conversation history for a specific session.

```bash
memlayer session <session-uuid> --limit 200 --types user,assistant
```

- `<session_id>` (positional): The session UUID to retrieve
- `--limit <n>`: (optional) Max entries, default 200
- `--types <types>`: (optional) Comma-separated type filter
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

### memlayer status

Check server health and embedding status.

```bash
memlayer status
```

## Usage Pattern
1. Search broadly first with `memlayer search`
2. If a result looks relevant, use `memlayer session` with its session_id for full context
3. If search or session results include a `large_response` reference, use `memlayer read-file` with the file_id and line ranges from the structural index to read specific sections
4. Present findings with session date and project context
5. If no results found, say so honestly — do not fabricate memories
