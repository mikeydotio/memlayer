# Task Management with Storyhook

This project uses **storyhook** (`story` CLI) for work tracking.

**Important:** The `.storyhook/` directory is version-controlled project data. Do NOT add it to `.gitignore`.

## Session lifecycle

1. Run `story context` at the start of every session to understand project state.
2. Run `story next` to find the highest-priority ready task.
3. Update story status as you work: `story ME-<n> is in-progress`
4. Add progress notes: `story ME-<n> "what changed and why"`
5. Mark complete: `story ME-<n> is done "summary of what was delivered"`
6. Run `story handoff --since 2h` at end of session.

## Planning mode

When creating implementation plans, create a story for each discrete work item, phase, or issue:

```
story new "Phase 1: Set up database schema"
story new "Phase 2: Implement API endpoints"
story new "Phase 3: Add authentication middleware"
```

Define relationships between stories to express dependencies and structure:

```
story ME-1 parent-of ME-2
story ME-2 precedes ME-3
story ME-5 relates-to ME-2
story ME-6 obviates ME-7
```

Set priority on each story so `story next` surfaces the right work:

```
story ME-1 priority critical
story ME-4 priority high
story ME-6 priority medium
```

## During execution

- Before starting a story: `story ME-<n> is in-progress`
- When blocked: `story ME-<n> awaits "reason"`
- When unblocked: `story ME-<n> awaits --clear`
- When done: `story ME-<n> is done "what was delivered"`
- To check what's ready: `story next --count 5`
- To see blocked work: `story list --blocked`
- To see the dependency graph: `story graph`

## Commands

| Action | Command |
|---|---|
| Project overview | `story context` |
| Next ready task | `story next` |
| List open stories | `story list` |
| Show a story | `story ME-<n>` |
| Create a story | `story new "<title>"` |
| Add a comment | `story ME-<n> "comment text"` |
| Set priority | `story ME-<n> priority high` |
| Search | `story search "<query>"` |
| Summary stats | `story summary` |
| Dependency graph | `story graph` |
| Session handoff | `story handoff --since 2h` |
