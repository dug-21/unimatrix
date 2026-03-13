---
name: "uni-store-procedure"
description: "Store or update a technical procedure (how-to) in Unimatrix. Use during retrospectives when a technique has evolved or been discovered."
---

# Store Procedure — Technical How-To Knowledge

## What This Skill Does

Stores a step-by-step technical procedure in Unimatrix. Procedures describe HOW to accomplish specific tasks in this project. They evolve as the project evolves.

**Use during retrospectives** — not while implementing. Procedures are extracted from evidence, not guessed mid-session.

---

## Procedure vs Convention vs Pattern

| Type | What it is | Example |
|------|-----------|---------|
| **Procedure** | Ordered steps to accomplish a task | "How to add a new MCP tool: step 1, 2, 3..." |
| **Convention** | A rule or standard | "No .unwrap() in non-test code" |
| **Pattern** | A reusable solution to a recurring problem | "Fresh context for unbiased review" |

If it has **numbered steps**, it's a procedure. If it's a **rule**, it's a convention. If it's a **when/why/how solution**, it's a pattern.

---

## How to Store a New Procedure

### Step 1: Check for existing procedure

```
mcp__unimatrix__context_search(
  query: "{what the procedure covers}",
  category: "procedure",
  k: 3
)
```

If an existing procedure covers the same task, use Step 2 (Update) instead.

### Step 2a: Store NEW procedure

```
mcp__unimatrix__context_store(
  title: "How to {task description}",
  content: "{step-by-step content}",
  topic: "{crate or area — e.g., 'unimatrix-server'}",
  category: "procedure",
  tags: ["{domain}", "{consuming-roles}"],
  agent_id: "{your role name, e.g. uni-architect}"
)
```

### Step 2b: UPDATE existing procedure (supersedes old version)

```
mcp__unimatrix__context_correct(
  original_id: {old entry ID},
  content: "{updated step-by-step content}",
  reason: "Updated: {what changed and why}"
)
```

This deprecates the old entry and creates a new one with a supersession chain. Agents querying later will get the latest version.

---

## Content Format

Procedures should be **concise and actionable** (200-500 chars):

```
How to add a new MCP tool:
1. Add validate_{tool}_params fn in validation.rs (pure, no I/O)
2. Add format_{tool}_success fn in response.rs (summary + markdown + json)
3. Add handler block in tools.rs match arm
4. Add AuditEvent variant in audit.rs
5. Add tool schema in server registration
6. Add integration test in product/test/infra-001/
```

NOT:
```
When you want to add a new MCP tool to the server, you should first consider
the validation requirements. The validation module in validation.rs contains
pure functions that validate input parameters...
```

---

## Tagging Conventions

| Tag type | Examples |
|----------|----------|
| Crate/area | `server`, `store`, `vector`, `core`, `embed` |
| Consuming roles | `rust-dev`, `pseudocode`, `architect` |
| Domain | `mcp-tool`, `schema`, `testing`, `integration` |

Always include at least one consuming-role tag so `/uni-query-patterns` can filter by who needs it.

---

## When to Store vs When to Skip

**Store when:**
- A multi-step technique was used across 2+ features
- An existing procedure was wrong and needed correction
- A developer had to figure out steps that should have been documented

**Skip when:**
- The technique was used once and may not recur
- The steps are obvious to any Rust developer (not project-specific)
- The procedure is workflow choreography (that stays in coordinator agent defs)
