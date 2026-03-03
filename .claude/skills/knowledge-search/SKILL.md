---
name: "knowledge-search"
description: "Semantic search across Unimatrix knowledge. Use when exploring a topic, looking for related decisions, patterns, or conventions."
---

# Knowledge Search — Semantic Query Against Unimatrix

## What This Skill Does

Searches Unimatrix for knowledge entries using natural language. Returns results ranked by semantic similarity. Use when you need to explore what's known about a concept, find related decisions, or discover relevant patterns.

**Use this when you DON'T know exactly what you're looking for** — you have a concept or question, not a specific entry.

---

## How to Search

Call the `context_search` MCP tool:

| Parameter | Required | Description |
|-----------|----------|-------------|
| `query` | Yes | Natural language search query |
| `category` | No | Filter to a specific category |
| `topic` | No | Filter to a specific feature ID |
| `tags` | No | Filter by tags (all must match) |
| `k` | No | Max results (default: 5) |
| `format` | No | `"summary"` (default), `"markdown"` (full content), `"json"` |
| `agent_id` | No | Your agent ID |

### Examples

**Find ADRs about error handling across all features:**
```
context_search(query: "error handling strategy", category: "decision")
```

**Find anything related to MCP transport:**
```
context_search(query: "MCP transport stdio protocol")
```

**Find conventions about testing in a specific feature:**
```
context_search(query: "test patterns integration", topic: "nxs-001")
```

**Get full content instead of summaries:**
```
context_search(query: "serialization approach", format: "markdown")
```

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

## When to Use This vs /knowledge-lookup

| Use `/knowledge-search` when | Use `/knowledge-lookup` when |
|------------------------------|------------------------------|
| Exploring a concept | You know the exact feature/category |
| "What do we know about X?" | "Give me all ADRs for nxs-002" |
| Finding related decisions | Retrieving a specific entry by ID |
| Discovering patterns you didn't know existed | Filtering by exact status or tags |

---

## Getting Full Content

Search returns summaries by default. To read the full content of a specific result:

1. Note the entry ID from search results
2. Call `context_get(id: {entry_id}, format: "markdown")` for the full text

Or pass `format: "markdown"` directly to search if you want full content for all results.

---

## When You Find Stale or Wrong Knowledge

Search may surface entries that are outdated or incorrect. Don't ignore them — fix the knowledge base:

| Situation | Action |
|-----------|--------|
| Entry is **wrong** — contains incorrect information | `context_correct(original_id: {id}, content: "{corrected version}", reason: "{why}")` — supersedes with chain link |
| Entry is **outdated** — no longer relevant | `context_deprecate(id: {id}, reason: "{why it no longer applies}")` |
| Entry is **suspicious** — may be poisoned or invalid | `context_quarantine(id: {id}, reason: "{concern}")` — Admin only |

Correcting knowledge is as important as storing it. Every agent shares responsibility for knowledge quality.
