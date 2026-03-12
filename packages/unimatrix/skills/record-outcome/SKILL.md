---
name: "record-outcome"
description: "Record a feature or bugfix outcome in Unimatrix. Use at the end of every session (design, delivery, bugfix, retrospective)."
---

# Record Outcome — Session Completion Record

## What This Skill Does

Stores a structured outcome record in Unimatrix. Outcomes enable the retrospective pipeline to analyze what shipped, how it went, and detect cross-feature patterns.

**Use at the END of every session** — design, delivery, bugfix, or retrospective.

---

## How to Record

Call `mcp__unimatrix__context_store` with these parameters:

| Parameter | Value |
|-----------|-------|
| `category` | `"outcome"` |
| `topic` | `"{feature-id}"` (e.g., `"col-011"`) |
| `feature_cycle` | `"{feature-id}"` |
| `tags` | Structured tags (see below) |
| `content` | What happened — artifacts, results, key facts |
| `agent_id` | Your role name (e.g. `uni-architect`) |

### Required Tags

Tags use `key:value` format. Include ALL applicable:

| Tag | Values | Required |
|-----|--------|----------|
| `type:{x}` | `feature`, `bugfix`, `incident`, `process`, `session` | Yes |
| `phase:{x}` | `research`, `design`, `implementation`, `testing`, `validation` | Yes |
| `result:{x}` | `pass`, `fail`, `rework`, `skip` | Yes |
| `gate:{x}` | `3a`, `3b`, `3c` | Only for delivery (last gate passed) |

### Examples

**Design session complete:**
```
mcp__unimatrix__context_store(
  category: "outcome",
  topic: "col-011",
  feature_cycle: "col-011",
  tags: ["type:feature", "phase:design", "result:pass"],
  content: "Session 1 complete. 9 artifacts produced. GH Issue #65.
    ADR entries: #250, #251. 3 scope risks identified (SR-01 through SR-03)."
)
```

**Implementation session complete:**
```
mcp__unimatrix__context_store(
  category: "outcome",
  topic: "col-011",
  feature_cycle: "col-011",
  tags: ["type:feature", "phase:implementation", "result:pass", "gate:3c"],
  content: "Session 2 complete. All 3 gates passed. PR #70.
    12 unit tests, 4 integration tests added. No rework needed."
)
```

**Bugfix complete:**
```
mcp__unimatrix__context_store(
  category: "outcome",
  topic: "col-011",
  feature_cycle: "col-011",
  tags: ["type:bugfix", "phase:implementation", "result:pass"],
  content: "Bug fix shipped. Root cause: off-by-one in confidence calculation.
    PR #72. 2 tests added. No rework."
)
```

---

## Content Guidelines

Keep content to 100-300 characters. Include:
- What shipped (artifact count, PR number)
- Key metrics (test count, gate results, rework count)
- Notable facts (ADR IDs, risk count, scope changes)

Do NOT include full artifact lists or file paths — those are in the feature directory.

---

## Self-Verification

After calling `context_store`, verify:
- Response confirms entry stored (returns entry ID)
- Tags follow the `key:value` format exactly
- `feature_cycle` matches the feature ID
