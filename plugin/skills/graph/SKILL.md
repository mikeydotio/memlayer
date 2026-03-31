---
name: memlayer:graph
description: >-
  Use when the user explicitly asks about the memlayer memory graph, memory
  graph entities, memory graph relationships, or memory graph statistics.
  Trigger on "memory graph", "memlayer graph", "memlayer entities",
  "memory graph stats", "curate memory graph", "merge memory entities",
  "graph-expanded search". Do NOT trigger on generic uses of "graph",
  "entities", or "relationships" without the "memory" or "memlayer" qualifier.
version: 2.1.0
---

# Graph - Memory Knowledge Graph

The memlayer knowledge graph contains entities and relationships extracted from past conversations: concepts, decisions, bugs, patterns, tools, libraries, and architectural choices. Use these tools to browse, search, and curate the graph.

## Browsing Commands

### memlayer entities

List and search knowledge graph entities.

```bash
memlayer entities --format text
memlayer entities --query "auth middleware" --format text
memlayer entities --type decision --format text
memlayer entities --type bug --project /home/mikey/myproject --format text
```

- `--query <text>`: (optional) Fuzzy search on entity name
- `--type <type>`: (optional) Filter by type: concept, decision, bug, pattern, tool, library, architecture, file, person, project
- `--project <path>`: (optional) Filter to project
- `--status <status>`: (optional) Filter by status: active (default), superseded, resolved, archived
- `--limit <n>`: (optional) Max results, default 20
- `--format json|text`: (optional) Output format, default json

### memlayer entity

View entity detail with relationships, aliases, and recent mentions.

```bash
memlayer entity 42 --format text
memlayer entity 42 --neighbors --format text
```

- `<id>` (positional): Entity ID
- `--neighbors`: (optional) Also show graph neighbors (1-hop connections)
- `--format json|text`: (optional) Output format, default json

### memlayer graph stats

Show knowledge graph statistics: entity/relationship counts by type, extraction progress.

```bash
memlayer graph stats
```

## Graph-Expanded Search

Augment regular search with graph-connected entries. When enabled, results include entries linked to your search results through entity relationships -- even if they don't match the query directly.

```bash
memlayer search "auth middleware" --expand-graph --format text
memlayer search "database migration" --expand-graph --graph-weight 1.0
```

- `--expand-graph`: Enable graph expansion (default: off)
- `--graph-weight <float>`: Weight for graph re-ranking, 0.0-2.0 (default: 0.5)

## Agent Graph Tools

When you recognize important concepts, decisions, or patterns during a conversation, you can directly create entities and relationships via the server API. Source credentials from `~/.config/memlayer/env` if `MEMLAYER_AUTH_TOKEN` and `MEMLAYER_SERVER_URL` are not in the environment.

### Create Entity

```bash
curl -s -X POST -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  "$MEMLAYER_SERVER_URL/entities" \
  -d '{"canonical_name": "cursor-based pagination", "entity_type": "pattern", "description": "Used for browse endpoints to handle large result sets", "project_path": "/home/mikey/memlayer"}'
```

### Create Relationship

```bash
curl -s -X POST -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  "$MEMLAYER_SERVER_URL/relationships" \
  -d '{"source_entity_id": 42, "target_entity_id": 15, "relationship_type": "supersedes", "description": "New auth flow replaces the old one", "confidence": 1.0}'
```

Relationship types: `supports`, `contradicts`, `supersedes`, `depends_on`, `refines`, `implements`, `related_to`, `part_of`, `caused_by`, `resolved_by`

### Update Entity Status

Mark decisions as superseded, bugs as resolved:

```bash
curl -s -X PATCH -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  "$MEMLAYER_SERVER_URL/entities/42" \
  -d '{"status": "superseded"}'
```

### Merge Entities

When you notice duplicate entities representing the same concept:

```bash
curl -s -X PATCH -H "Authorization: Bearer $MEMLAYER_AUTH_TOKEN" \
  -H "Content-Type: application/json" \
  "$MEMLAYER_SERVER_URL/entities/43" \
  -d '{"merge_into": 42}'
```

## When to Use Graph Tools

- **Create entities** for: key decisions made during conversation, significant bugs discussed, architectural patterns adopted, important tools or libraries chosen
- **Create relationships** when: a new decision supersedes an old one, a bug is caused by a specific component, a pattern depends on a specific tool
- **Update status** when: a bug is resolved, a decision is reversed, an approach is abandoned
- **Merge entities** when: you find two entities that refer to the same thing ("auth rewrite" and "authentication refactor")
- Do NOT create entities for ephemeral chatter, greetings, or transient debugging steps
