---
name: "uni-store-lesson"
description: "Store a lesson learned from a failure, gate rejection, or unexpected issue. Use after bugfixes and gate failures to prevent recurrence."
---

# Store Lesson — Failure Knowledge

## What This Skill Does

Stores a lesson learned in Unimatrix. Lessons capture what went wrong, why, and the takeaway. They surface in future briefings and searches to prevent the same failure from recurring.

**Use after:** gate failures, bug diagnoses, unexpected issues, rework cycles.

---

## How to Store

### Step 1: Check for existing lessons in the same area

```
mcp__unimatrix__context_search({
  "query": "{what went wrong}",
  "category": "lesson-learned",
  "k": 3
})
```

If a matching lesson already exists, go to Step 2b (supersede) instead of creating a duplicate.

### Step 2a: Store NEW lesson (no prior exists)

```
mcp__unimatrix__context_store({
  "title": "{concise description of what went wrong}",
  "content": "{structured lesson content}",
  "topic": "{feature-id or crate}",
  "category": "lesson-learned",
  "tags": ["{domain}", "{failure-type}"],
  "agent_id": "{your role name, e.g. uni-architect}"
})
```

### Step 2b: Supersede EXISTING lesson (prior exists but is incomplete or outdated)

```
mcp__unimatrix__context_correct({
  "original_id": {old entry ID},
  "content": "{updated lesson with new evidence or broader scope}",
  "reason": "Updated: {what new evidence or context was added}"
})
```

This deprecates the old lesson and creates a corrected version with a supersession chain. Future searches return the latest version.

### When to deprecate without replacing

If a lesson is simply wrong or no longer applies (e.g., the underlying code was redesigned):

```
mcp__unimatrix__context_deprecate({"id": {entry ID}, "reason": "{why it no longer applies}"})
```

---

## Content Format

Structure as: **What happened -> Root cause -> Takeaway** (200-500 chars):

```
Gate 3b rejected: confidence calculation used f32 intermediate values
despite f64 pipeline decision (ADR-003). Root cause: rust-dev didn't
read ADR before implementing. Takeaway: MANDATORY ADR read step in
rust-dev pseudocode consumption is not optional — validator should
check ADR compliance explicitly.
```

NOT a full incident report. NOT a narrative. Just the facts that prevent recurrence.

---

## Tagging Conventions

| Tag type | Examples |
|----------|----------|
| Failure type | `gate-failure`, `bug`, `rework`, `regression`, `scope-fail` |
| Gate | `gate-3a`, `gate-3b`, `gate-3c` |
| Domain | `confidence`, `storage`, `mcp`, `testing` |
| Severity | `minor`, `major`, `critical` |

---

## Who Stores Lessons

| Agent | When |
|-------|------|
| uni-bug-investigator | After diagnosing root cause — store the generalizable pattern |
| uni-validator | After gate failure — store what the gate caught and why |
| Coordinator | After rework cycle — store what caused the rework |
| Retrospective agents | After analyzing session data — store systemic issues |

---

## What Makes a Good Lesson

**Generalizable** — applies beyond this one incident. "Off-by-one in loop" is not a lesson. "Boundary conditions at table scan limits not covered by unit tests" is.

**Actionable** — the takeaway prevents recurrence. "Be more careful" is not actionable. "Add boundary condition tests for every table scan method" is.

**Concise** — 200-500 chars. If it needs more, it's probably a procedure or a pattern, not a lesson.
