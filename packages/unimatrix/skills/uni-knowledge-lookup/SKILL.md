---
name: "uni-knowledge-lookup"
description: "Deterministic lookup of Unimatrix knowledge by exact filters. Use when you know what you want — a specific feature, category, entry ID, or status."
---

# Knowledge Lookup — Deterministic Query Against Unimatrix

## What This Skill Does

Retrieves Unimatrix entries by exact filters — topic, category, tags, status, or entry ID. Returns all matching entries without semantic ranking. Use when you know precisely what you're looking for.

**Use this when you KNOW what you want** — a specific feature's ADRs, entries with a particular status, or a known entry by ID.

---

## How to Look Up

Call the `mcp__unimatrix__context_lookup` MCP tool:

| Parameter | Required | Description |
|-----------|----------|-------------|
| `topic` | No* | Exact feature ID match (e.g., `"nxs-001"`) |
| `category` | No* | Exact category match (e.g., `"decision"`) |
| `tags` | No* | All specified tags must match |
| `status` | No | `"active"` (default), `"deprecated"`, `"proposed"` |
| `id` | No* | Specific entry ID (returns exactly one entry) |
| `limit` | No | Max results (default: 10) |
| `format` | No | `"summary"` (default), `"markdown"` (full content), `"json"` |
| `agent_id` | No | Your role name (e.g. `uni-architect`) |

*At least one filter parameter is required (topic, category, tags, or id).

### Examples

**Get all ADRs for a specific feature:**
```
mcp__unimatrix__context_lookup({"topic": "nxs-002", "category": "decision", "helpful": true})
```

**Get a specific entry by ID (full content):**
```
mcp__unimatrix__context_lookup({"id": 42, "format": "markdown"})
```

**Find all deprecated decisions:**
```
mcp__unimatrix__context_lookup({"category": "decision", "status": "deprecated"})
```

**Find entries tagged with a specific domain:**
```
mcp__unimatrix__context_lookup({"category": "decision", "tags": ["adr", "serialization"]})
```

**Get all knowledge for a feature (any category):**
```
mcp__unimatrix__context_lookup({"topic": "vnc-001"})
```

### Helpful Vote Guidance

Pass `helpful: true` when the retrieved entries applied to the task — this is the standard case for deliberate lookups.
Pass `helpful: false` when entries were retrieved but did not apply to the task (e.g., the result was for the wrong feature or an unrelated concern). Negative signal is valuable for confidence calibration.
Omit `helpful` when you cannot determine applicability, such as during exploratory browsing or pre-flight checks where you don't yet know which entries will be used.

Note: `context_lookup` already records a doubled access signal automatically (×2 weight vs. search). The `helpful` vote is an additional optional signal for quality calibration.

---

## Single Entry Retrieval

If you already have an entry ID (from a prior search or lookup result), use `context_get` for direct retrieval:

```
mcp__unimatrix__context_get({"id": 42, "format": "markdown"})
```

This is faster than a lookup with an ID filter and always returns full content.

---

## When to Use This vs /uni-knowledge-search

| Use `/uni-knowledge-lookup` when | Use `/uni-knowledge-search` when |
|------------------------------|------------------------------|
| You know the exact feature/category | Exploring a concept |
| "Give me all ADRs for nxs-002" | "What do we know about X?" |
| Retrieving a specific entry by ID | Finding related decisions |
| Filtering by exact status or tags | Discovering unknown patterns |
| Checking what exists before storing | Broad exploration |

---

## Common Workflows

**Before writing a new ADR (architect):**
```
1. mcp__unimatrix__context_lookup({"topic": "{feature-id}", "category": "decision"})
   → See what ADRs already exist for this feature
2. mcp__unimatrix__context_lookup({"category": "decision", "tags": ["adr", "{domain}"]})
   → See ADRs across features in the same domain
```

**Before implementing a component (developer):**
```
1. mcp__unimatrix__context_lookup({"topic": "{feature-id}", "category": "decision", "format": "markdown"})
   → Read all architectural decisions for this feature
```

**Checking for deprecated knowledge:**
```
mcp__unimatrix__context_lookup({"category": "decision", "status": "deprecated", "topic": "{feature-id}"})
→ See what decisions have been superseded
```

---

## When You Find Stale or Wrong Knowledge

Lookup may surface entries that are outdated or incorrect. Fix them:

| Situation | Action |
|-----------|--------|
| Entry is **wrong** | `mcp__unimatrix__context_correct({"original_id": 1234, "content": "{corrected version}", "reason": "{why}"})` — `original_id` is an integer, never quote it |
| Entry is **outdated** | `mcp__unimatrix__context_deprecate({"id": 1234, "reason": "{why}"})` — `id` is an integer, never quote it |
| Entry is **suspicious** | `mcp__unimatrix__context_quarantine({"id": 1234, "reason": "{concern}"})` — Admin only; `id` is an integer |

Every agent shares responsibility for knowledge quality. Don't leave wrong entries for the next agent to trip over.
