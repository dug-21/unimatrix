# protocols — Restart Re-Declaration Line

**Source**: `.claude/protocols/uni/uni-design-protocol.md`,
`uni-delivery-protocol.md`, `uni-bugfix-protocol.md` (extend). **AC**: AC-09 /
FR-24. Documentation-only; no code, no behavior change to any binary.

## Purpose

Each of the three protocols gains one line: on RE-ENTERING a broken/interrupted
session, the leader's first action is to re-issue the cycle declaration. This is
the recovery path for "broken session, fresh restart" (SPEC workflow 3): a new
session_id misses the tracker file; re-declaring recreates it (server-side
idempotent via `AlreadyMatches`; client-side recreates `cycles/{session_key}.json`
via the PreToolUse interception → `cycles.writeCycle`).

## The line (normative wording, FR-24 / Integration Surface)

> On re-entering a broken session, the leader's first action is to re-issue
> `context_cycle(type:"start", topic:"{feature-id}")` (idempotent server-side —
> `AlreadyMatches`; recreates the client tracker).

## Insertion point (per file)

Each protocol already has an Init declaration near the top, e.g.
`uni-design-protocol.md:58-61` ("Before spawning any agents, call `context_cycle`
to declare the feature cycle") and an explicit Init line at
`uni-design-protocol.md:411` (`Init: context_cycle(type: "start", ...)`). Add the
re-declaration line adjacent to the existing declaration guidance in each file —
either as a note under the Init step or in a "resuming an interrupted session"
subsection if one exists. Pick the same relative location in all three for
consistency.

Files and the analogous Init anchor in each (delivery confirms exact lines):
- `uni-design-protocol.md` — beside the Init `context_cycle(type:"start"...)` (~:411 / :58).
- `uni-delivery-protocol.md` — beside its Init cycle declaration.
- `uni-bugfix-protocol.md` — beside its Init cycle declaration.

## Verification

Doc inspection: the exact re-declaration line is present in ALL THREE files. No
test harness; this is a Gate verification item (FR-24: "Verification: doc
inspection — line present in all three").

## Key Test Scenarios

- Grep each of the three protocol files for the re-declaration line; assert
  present in all three.

## Open Questions / Gaps

- Exact phrasing may be trimmed to match each protocol's house style as long as it
  carries the three load-bearing facts: (1) trigger = re-entering a broken
  session, (2) action = `context_cycle(type:"start", topic:"{feature-id}")`,
  (3) it is idempotent and recreates the client tracker. No semantic latitude
  beyond wording.
