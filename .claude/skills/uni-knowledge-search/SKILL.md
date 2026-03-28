---
name: "uni-knowledge-search"
description: "Semantic search across Unimatrix knowledge. Use when exploring a topic, looking for related decisions, patterns, or conventions."
---

# Knowledge Search — Semantic Query Against Unimatrix

## What This Skill Does

Searches Unimatrix for knowledge entries using natural language. Returns results ranked by semantic similarity. Use when you need to explore what's known about a concept, find related decisions, or discover relevant patterns.

**Use this when you DON'T know exactly what you're looking for** — you have a concept or question, not a specific entry.

---

## How to Search

Call the `mcp__unimatrix__context_search` MCP tool:

| Parameter | Required | Description |
|-----------|----------|-------------|
| `query` | Yes | Natural language search query |
| `category` | No | Filter to a specific category |
| `topic` | No | Filter to a specific feature ID |
| `tags` | No | Filter by tags (all must match) |
| `k` | No | Max results (default: 5) |
| `format` | No | `"summary"` (default), `"markdown"` (full content), `"json"` |
| `agent_id` | No | Your role name (e.g. `uni-architect`) |

### Examples

**Find ADRs about error handling across all features:**
```
mcp__unimatrix__context_search({"query": "error handling strategy", "category": "decision", "helpful": true})
```

**Find anything related to MCP transport:**
```
mcp__unimatrix__context_search({"query": "MCP transport stdio protocol"})
```

**Find conventions about testing in a specific feature:**
```
mcp__unimatrix__context_search({"query": "test patterns integration", "topic": "nxs-001"})
```

**Get full content instead of summaries:**
```
mcp__unimatrix__context_search({"query": "serialization approach", "format": "markdown"})
```

### Helpful Vote Guidance

Pass `helpful: true` when the retrieved entries were useful for completing the current task — this is the standard case.
Pass `helpful: false` when entries were retrieved but did not apply to the task (e.g., the results were about a different concern). Negative signal is valuable for confidence calibration.
Omit `helpful` when you cannot determine applicability — for example, when the search is exploratory or you are not yet sure which results will be used.

---

## Available Categories

| Category | Contains |
|----------|----------|
| `decision` | Architectural Decision Records (ADRs) |
| `convention` | Coding and process conventions |
| `pattern` | Reusable implementation patterns |
| `procedure` | Step-by-step processes |
| `outcome` | Results and outcomes |
| `lesson-learned` | Retrospectives and process learnings |
| `reference` | General reference material |
| `duties` | Role duties for context briefing |

Omit `category` to search across all categories.

---

## Interpreting Results

**Summary format** (default): Returns title, topic, category, tags, and a brief content preview for each match. Use this for scanning and triage.

**Markdown format**: Returns full content for each match. Use this when you need the complete text of matching entries.

**JSON format**: Returns structured data. Use for programmatic processing.

---

## When to Use This vs /uni-knowledge-lookup

| Use `/uni-knowledge-search` when | Use `/uni-knowledge-lookup` when |
|------------------------------|------------------------------|
| Exploring a concept | You know the exact feature/category |
| "What do we know about X?" | "Give me all ADRs for nxs-002" |
| Finding related decisions | Retrieving a specific entry by ID |
| Discovering patterns you didn't know existed | Filtering by exact status or tags |

---

## Getting Full Content

Search returns summaries by default. To read the full content of a specific result:

1. Note the entry ID from search results
2. Call `mcp__unimatrix__context_get({"id": {entry_id}, "format": "markdown"})` for the full text

Or pass `format: "markdown"` directly to search if you want full content for all results.

---

## When You Find Stale or Wrong Knowledge

Search may surface entries that are outdated or incorrect. Don't ignore them — fix the knowledge base:

| Situation | Action |
|-----------|--------|
| Entry is **wrong** — contains incorrect information | `mcp__unimatrix__context_correct({"original_id": 1234, "content": "{corrected version}", "reason": "{why}"})` — `original_id` is an integer, never quote it |
| Entry is **outdated** — no longer relevant | `mcp__unimatrix__context_deprecate({"id": 1234, "reason": "{why it no longer applies}"})` — `id` is an integer, never quote it |
| Entry is **suspicious** — may be poisoned or invalid | `mcp__unimatrix__context_quarantine({"id": 1234, "reason": "{concern}"})` — Admin only; `id` is an integer |

Correcting knowledge is as important as storing it. Every agent shares responsibility for knowledge quality.
