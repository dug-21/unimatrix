---
name: "store-pattern"
description: "Store a reusable implementation pattern in Unimatrix. Use when you discover a gotcha, trap, or reusable solution that future agents should know."
---

# Store Pattern — Implementation Knowledge

## What This Skill Does

Stores a reusable pattern in Unimatrix. Patterns capture implementation gotchas, traps, and solutions — knowledge invisible in source code that you only learn by hitting it. They surface in future `/query-patterns` results so the next agent doesn't repeat the discovery.

**Use when:** you discover something that compiles but breaks at runtime, a non-obvious integration requirement, a crate-specific trap, or a reusable solution to a recurring problem.

---

## Pattern vs Lesson vs Procedure

| Type | When to use | Example |
|------|------------|---------|
| **Pattern** (`/store-pattern`) | Reusable solution or gotcha applicable regardless of failure context | "Don't hold lock_conn() across await — deadlocks under concurrent requests" |
| **Lesson** (`/store-lesson`) | Triggered by a specific failure; takeaway is preventive | "Gate 3b failed because rust-dev didn't read ADR before implementing" |
| **Procedure** (`/store-procedure`) | Ordered steps to accomplish a task | "How to add a new MCP tool: step 1, 2, 3..." |

**Decision rule:** If the knowledge was triggered by a specific failure and the takeaway is "don't do X," use `/store-lesson`. If the knowledge is a reusable solution applicable regardless of failure context — "when doing X, use approach Y because Z" — use `/store-pattern`.

---

## How to Store

### Step 1: Check for existing patterns in the same area

```
mcp__unimatrix__context_search(
  query: "{what the pattern is about}",
  category: "pattern",
  k: 3
)
```

If a matching pattern already exists, go to Step 2b (supersede) instead of creating a duplicate.

### Step 2a: Store NEW pattern (no prior exists)

Assemble the content from three required fields:

- **What**: The pattern in one sentence (max 200 chars). What to do or not do.
- **Why**: What goes wrong without it (min 10 chars). The consequence that makes this worth knowing.
- **Scope**: Where it applies — crate name, module, or context.

```
mcp__unimatrix__context_store(
  title: "{concise what statement}",
  content: "What: {what}\nWhy: {why}\nScope: {scope}",
  topic: "{crate name or module — e.g., 'unimatrix-store'}",
  category: "pattern",
  tags: ["{domain}", "{feature_cycle if known}"],
  agent_id: "{your role name, e.g. uni-rust-dev}"
)
```

### Step 2b: Supersede EXISTING pattern (prior exists but is incomplete or outdated)

```
mcp__unimatrix__context_correct(
  original_id: {old entry ID},
  content: "What: {updated what}\nWhy: {updated why}\nScope: {updated scope}",
  reason: "Updated: {what changed and why}"
)
```

This deprecates the old pattern and creates a corrected version with a supersession chain.

---

## Quality Rules

**Reject if:**
- `why` is missing or under 10 characters — no motivation, no value
- `what` exceeds 200 characters — not concise enough
- Content is API documentation, not a gotcha — "Store has a lock_conn() method" is docs, not a pattern

**Good patterns:**
```
What: Don't hold lock_conn() across await points
Why: Deadlocks under concurrent MCP requests — the Store mutex is not async-aware
Scope: unimatrix-store, any async caller
```

```
What: Use #[serde(default)] on all new EntryRecord fields
Why: Existing serialized records lack the field; deserialization panics without default
Scope: unimatrix-core EntryRecord, any schema evolution
```

**Bad patterns:**
```
What: I used Arc::clone for shared ownership
Why: It works
```
(No gotcha. No consequence. This is just Rust basics.)

---

## Tagging Conventions

| Tag type | Examples |
|----------|----------|
| Crate | `store`, `server`, `vector`, `core`, `embed`, `engine` |
| Domain | `async`, `serialization`, `migration`, `mcp`, `confidence` |
| Feature cycle | `vnc-009`, `crt-005` (for retro traceability) |

---

## Who Stores Patterns

| Agent | When |
|-------|------|
| uni-rust-dev | Implementation gotchas discovered while coding |
| uni-tester | Test infrastructure patterns, fixture usage discoveries |
| uni-risk-strategist | Risk patterns that recur across features |
| uni-researcher | Technical landscape patterns from problem space exploration |
| uni-vision-guardian | Recurring alignment variance patterns across features |
| uni-validator | Quality patterns (cross-feature, not gate-specific) |
| Retrospective agents | Patterns extracted from shipped feature analysis |
