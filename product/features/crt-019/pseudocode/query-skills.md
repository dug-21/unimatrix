# Component: query-skills

**Files**:
- `.claude/skills/uni-knowledge-search/SKILL.md`
- `.claude/skills/uni-knowledge-lookup/SKILL.md`

## Purpose

Documentation-only changes. No code changes. Update two skill files to:
1. Include `helpful: true` in the primary example invocation.
2. Add guidance on when to pass `helpful: false`.

These updates wire the last mile of the helpful-vote infrastructure: after
crt-019 activates the formula to respond to votes, agents following the skill
guidance will start generating vote signal automatically.

## Changes

### uni-knowledge-search/SKILL.md

In the primary example invocation block (wherever the tool call parameters are
shown), add `helpful: true` to the parameter list.

In the guidance text, add a note similar to:

```
By default, pass `helpful: true` when the retrieved entries were useful for the
task. Pass `helpful: false` when the entries were retrieved but did not apply —
this negative signal is also valuable for confidence calibration. Omit
`helpful` only when you cannot determine applicability (e.g., the tool is
called for exploration, not task completion).
```

The exact wording is at the implementor's discretion, but must cover:
- When to use `helpful: true` (standard case: used the entry)
- When to use `helpful: false` (retrieved but not applicable)
- When to omit (exploratory or uncertain)

### uni-knowledge-lookup/SKILL.md

Same additions as the search skill: add `helpful: true` to the primary example
and equivalent guidance on `helpful: false`.

Note: `context_lookup` already has doubled access signal (×2) from
crt-019 without needing a vote. The `helpful` vote on lookup is an additional
optional signal for quality calibration when the agent knows the entry applied.

## No Code Changes

This component has zero Rust code changes. No MCP parameter schema changes are
required — `helpful: Option<bool>` already exists on all query tool parameter
structs.

## Verification

Manual review of the diff to the two SKILL.md files. AC-09 criterion:
- `.claude/skills/uni-knowledge-search/SKILL.md` has `helpful: true` in at
  least one example and guidance on `helpful: false`.
- `.claude/skills/uni-knowledge-lookup/SKILL.md` has the same.

## Key Test Scenarios

No automated tests — this is documentation. Verification is by manual review
(AC-09).
