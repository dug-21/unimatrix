---
name: "uni-query-patterns"
description: "Query Unimatrix for component patterns, procedures, and conventions before designing or implementing. Use BEFORE writing pseudocode or code."
---

# Query Patterns — Find How Things Are Done Here

## What This Skill Does

Searches Unimatrix for established patterns, procedures, and conventions relevant to the work you're about to do. Returns actionable guidance on how similar work was done before.

**Use BEFORE designing or implementing** — not after.

---

## When to Use

| Situation | Query approach |
|-----------|---------------|
| Designing a new MCP tool | Search for component patterns in `unimatrix-server` |
| Adding a new table to redb | Search for procedures in `unimatrix-store` |
| Writing integration tests | Search for testing conventions |
| Implementing any component | Search for patterns in the affected crate |

---

## How to Query

### Step 1: Search by crate/area

```
mcp__unimatrix__context_search({
  "query": "{what you're building — e.g., 'MCP tool handler'}",
  "category": "pattern",
  "k": 5
})
```

### Step 2: Also check conventions for the area

```
mcp__unimatrix__context_search({
  "query": "{area — e.g., 'server tool pipeline'}",
  "category": "convention",
  "k": 5
})
```

### Step 3: Check for procedures (step-by-step techniques)

```
mcp__unimatrix__context_search({
  "query": "{task — e.g., 'adding a new MCP tool'}",
  "category": "procedure",
  "k": 3
})
```

### Step 4: Check for relevant ADRs

```
mcp__unimatrix__context_lookup({
  "topic": "{feature-id}",
  "category": "decision"
})
```

---

## Interpreting Results

**Patterns** tell you the reusable structure. Follow them unless your component has a good reason to deviate. If you deviate, note why in your pseudocode or code comments.

**Procedures** tell you the steps. Follow them in order. If a step is wrong or missing, note it — the retrospective will update the procedure.

**Conventions** tell you the rules. Follow them always. No deviations.

**ADRs** tell you what was decided and why. Respect the decision. If it seems wrong for your case, flag it to the coordinator — don't silently override.

---

## If Nothing Is Found

No results means either:
1. This is genuinely new work with no prior patterns — proceed with your best design
2. Patterns exist but aren't stored yet — check the codebase directly for similar code

In either case, your work may establish a NEW pattern that the retrospective will extract.

---

## When You Find Stale or Wrong Knowledge

Query results may include entries that are outdated or incorrect. Fix them before they mislead the next agent:

| Situation | Action |
|-----------|--------|
| Pattern/procedure is **wrong** | `mcp__unimatrix__context_correct({"original_id": 1234, "content": "{corrected version}", "reason": "{why}"})` — `original_id` is an integer, never quote it |
| Pattern/procedure is **outdated** | `mcp__unimatrix__context_deprecate({"id": 1234, "reason": "{why}"})` — `id` is an integer, never quote it |
| Convention no longer applies | `mcp__unimatrix__context_deprecate({"id": 1234, "reason": "{why}"})` — `id` is an integer, never quote it |

If you correct or deprecate an entry during your session, mention it in your return to the coordinator so it can be noted in the outcome.

---

## What NOT to Do

- Do NOT store patterns during this query phase — that's the retrospective's job
- Do NOT ignore results because "my approach is better" — patterns represent accumulated project wisdom
- Do NOT query for workflow choreography — that's in your coordinator, not Unimatrix
