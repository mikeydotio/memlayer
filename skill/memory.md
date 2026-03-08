# Memory - Cross-Session Recall

You have access to a searchable archive of all past Claude Code conversations across all projects. Use the memory tools to recall previous work, decisions, solutions, and context.

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

## Tools

### search_memory
Hybrid semantic + full-text search across all past conversations.
- `query`: Natural language description of what to find (be specific)
- `session_id`: (optional) Limit to a specific session
- `project_path`: (optional) Limit to a specific project, e.g. "/home/mikey/projects/agentsmith"
- `limit`: (optional) Number of results, default 10

### get_session_summary
Get chronological conversation history for a specific session.
- `session_id`: The session UUID to retrieve
- `limit`: (optional) Max entries, default 200

## Usage Pattern
1. Search broadly first with `search_memory`
2. If a result looks relevant, use `get_session_summary` with its session_id for full context
3. Present findings with session date and project context
4. If no results found, say so honestly — do not fabricate memories
