# Component: Retro-lifecycle restructure (BOTH protocols)

Files: `.claude/protocols/uni/uni-delivery-protocol.md:~516-521`,
`.claude/protocols/uni/uni-bugfix-protocol.md:~418-435`. Part of the CON-1 atomic unit (ships with the
server change). DOC edit specs, not code.

## Purpose

Keep the pr-review / bug-review phase OPEN through the human merge decision; close the cycle ONLY after
merge; run `/uni-retro` post-close. Strict ordering **merge → close cycle → retro**, in BOTH protocols
(ADR-005 / FR-21..23 / AC-17). Trivially safe now: no reaper anywhere on review→close→retro (review and
close are both non-destructive).

## `uni-delivery-protocol.md` — edit spec (currently :516-521)

CURRENT (defect): after returning to the human, it immediately runs `phase-end` + `stop` — closing the
cycle BEFORE the human merges.

```
# BEFORE (:516-521):
After returning to the human, close the pr-review phase and stop the cycle:
    context_cycle(type:"phase-end", phase:"pr-review", ...)
    context_cycle(type:"stop", topic:"{feature-id}", ...)
```

REPLACE with: pr-review phase stays OPEN through the merge decision; close AFTER merge; then retro.

```
# AFTER:
After returning to the human, KEEP the pr-review phase OPEN. Do NOT stop the cycle yet.
The human merge gate is unchanged.

ONCE THE HUMAN MERGES (strict order — merge → close → retro):
  1. context_cycle(type:"phase-end", phase:"pr-review", agent_id:"{feature-id}-delivery-leader")
  2. context_cycle(type:"stop", topic:"{feature-id}",
       outcome:"Session 2 complete. Merged. PR: {url}", agent_id:"{feature-id}-delivery-leader")
  3. /uni-retro   # post-close verbatim-candidate harvest; retrieval is repeatable + non-destructive
```

Add a one-line rationale: `context_cycle(stop)` is non-purging (drains only the retrospective queue +
writes an audit row — ADR-005), and the review never purges, so the post-close retro reads an intact
buffer and may retrieve repeatedly.

## `uni-bugfix-protocol.md` — edit spec (currently :418-435)

CURRENT (defect): `phase-end` + `stop` fire when the security review returns clean — BEFORE Phase 5
(Human Review & Merge).

```
# BEFORE (:418-435):
When the security review returns with no blocking findings:
    context_cycle(type:"phase-end", phase:"bug-review", ...)
    context_cycle(type:"stop", topic:"bugfix-{issue-number}", ...)
```

REPLACE: on a clean security review, KEEP the bug-review phase open; proceed to Phase 5 (Human Review &
Merge); close + retro only AFTER merge.

```
# AFTER:
When the security review returns with no blocking findings, KEEP the bug-review phase OPEN and proceed
to Phase 5 (Human Review & Merge). Do NOT stop the cycle yet.

ONCE THE HUMAN MERGES (strict order — merge → close → retro):
  1. context_cycle(type:"phase-end", topic:"bugfix-{issue-number}", phase:"bug-review",
       outcome:"{1-2 sentence phase result}", agent_id:"{issue-number}-bugfix-leader")
  2. context_cycle(type:"stop", topic:"bugfix-{issue-number}",
       outcome:"Bugfix complete. Merged. Root cause: {summary}. PR: {url}",
       agent_id:"{issue-number}-bugfix-leader")
  3. /uni-retro   # post-close harvest; repeatable, non-destructive

If the review returns blocking findings, resolve them before proceeding to merge.
```

Note: Phase 5 (Human Review & Merge, :441+) already presents the PR — the close block moves to AFTER
that merge, not before it.

## Invariants both edits must satisfy (AC-17 / R-04 / R-08)

- Review phase NOT stopped pre-merge (FR-21).
- `context_cycle(stop)` fires ONLY after the human merges (FR-22) — never ahead of the merge decision.
- `/uni-retro` invoked post-close, ordering exactly merge → close → retro (FR-23). A retro before close,
  or a close before merge, is a defect.
- Human merge gate UNCHANGED.
- Both files edited — a single-protocol fix FAILS the gate (protocol-parity, #4915).

## Key test scenarios (per protocol — a single-protocol green suite does NOT pass, #5383)

- End-to-end full-cycle simulation PER protocol: open → review phase → simulated human merge →
  `phase-end` → `stop` → `/uni-retro`; assert (a) cycle still OPEN at merge, (b) close only after merge,
  (c) post-close `transcript:{}` retrieval returns non-empty candidates + loss (R-04 sc.1).
- Ordering assertion: executed sequence is exactly merge → close → retro (R-04 sc.2).
- Protocol-parity grep: BOTH files contain the post-close `/uni-retro` step and NEITHER retains a
  pre-merge `context_cycle(stop)` (R-04 sc.3).
- `context_cycle(stop)`-is-buffer-inert + close-then-retrieve-still-delivers (R-08) — synchronous buffer
  observation, never absence-of-async-audit (R-10).
