---
name: "uni-zero-review"
description: "Ad-hoc independent product-lens review via uni-zero-reviewer. Use for re-reviews after rework or reviews outside protocol runs. Advisory only — human makes the call."
---

# Uni-Zero Review — Ad-Hoc Product-Lens Review

## What This Skill Does

Spawns `uni-zero-reviewer` in a fresh, disconnected context to review a feature, fix approach, or PR through the product vision/roadmap lens. The protocols invoke the reviewer automatically at their human gates — this skill covers everything else:

- Re-review after rework changed the artifacts
- Review of work that predates the reviewer touchpoints
- A second look the human wants outside any protocol run

Advisory only. The review never blocks anything; the human makes the final judgment.

---

## Inputs

From the human:
- **Feature ID or issue number** (required)
- **Gate** (required): `scope-review` | `design-review` | `fix-approach` | `pr-review`
- **PR number** (required for `pr-review`)

Usage: `/uni-zero-review {feature-id|issue-number} {gate} [pr-number]`

If the gate is omitted, infer it from what exists and confirm with the human before spawning:
- Only SCOPE.md exists → `scope-review`
- Full design artifact set exists, no PR → `design-review`
- Open PR exists → `pr-review`
- Bugfix issue with diagnosis comments, no PR → `fix-approach`

---

## Execution

1. **Resolve identifiers.** Find the GH issue number (`gh issue list`, or from the feature's IMPLEMENTATION-BRIEF/synthesizer output). For `pr-review`, confirm the PR number.

2. **Spawn `uni-zero-reviewer`.** The spawn prompt carries ONLY agent ID, gate, identifiers, and artifact paths — never summaries, conclusions, or framing from this session. The fresh, disconnected context is the point.

```
Task(subagent_type: "uni-zero-reviewer",
  prompt: "Your agent ID: {feature-id}-zero-{gate}-adhoc

    GATE: {gate}
    Feature: {feature-id}
    GH Issue: #{issue-number}
    {PR: #{pr-number} — pr-review only}
    Artifacts: {paths per the gate table in the agent definition;
    for scope-review with no GH issue, instruct: write review to
    product/features/{id}/reviews/uni-zero-scope-review.md}")
```

3. **Relay verbatim.** Return the reviewer's stance and comment URL (or review file path) to the human. Do NOT parse, summarize, act on, or gate on the review.

---

## Re-Review Convention

For a repeat review at the same gate, the reviewer posts a new comment (comments are append-only history). Note in the spawn prompt only that a prior review comment exists — not what it said.

---

## What This Skill Does NOT Do

- Approve, merge, or block anything
- Re-run security reviews, gate validations, or vision-guardian checks
- Modify artifacts, goals, or the vision document
