# Agent Report: bugfix-523-agent-2-spec

**Role**: Specification Writer
**Feature**: bugfix-523
**Output**: `product/features/bugfix-523/specification/SPECIFICATION.md`

## Deliverable

SPECIFICATION.md written and covers all four items of the hardening batch.

## Key Decisions Made

**SD-01 (SR-03 mitigation)**: AC-04 and AC-01 log-level testability is unresolved in SCOPE.md.
The spec offers two options (Option A: `tracing-test` crate assertion; Option B: behavioral
coverage only) and marks Option A as preferred. The architect must choose between them in the
IMPLEMENTATION-BRIEF before delivery begins. This is the only decision deferred downstream.

**FR-04 warn! message**: The spec prescribes the exact warn! message for the rework arm as
`"UDS: RecordEvent (rework_candidate) rejected: invalid session_id"` — differing from the
general RecordEvent arm only in the `(rework_candidate)` qualifier. This follows the pattern
from entry #3921 and keeps arm identification consistent in log output.

**FR-03 loop structure**: The fusion weight fields (12–17) and phase weight fields (18–19)
are guarded inside their existing loop bodies rather than by inlining 8 separate `let v =`
statements. This matches the least-diff approach and keeps the existing loop structure intact.

## Gaps or Ambiguities vs. SCOPE.md

**None that require upstream resolution.** One forward-looking note:

- SCOPE.md §Assumptions states: "any fields added after PR #516 are not covered." The field
  checklist in FR-03 is based on source code read of config.rs at the time of writing. If
  any float fields were added to `InferenceConfig` between PR #516 and the current HEAD that
  are not in the 19-field list, they are out of scope per SCOPE.md but would represent a
  latent NaN gap. The architect should confirm the field list is complete against current HEAD
  before assigning implementation.

## Self-Check

- [x] SPECIFICATION.md covers all 29 acceptance criteria from SCOPE.md (AC-01 through AC-29)
- [x] Every functional requirement is testable (with SD-01 decision on log-level option)
- [x] Non-functional requirements include measurable targets (O(1) sanitize, no latency regression)
- [x] Domain Models section defines Path A / Path B / Path C, `nli_enabled` semantics, `category_map` miss, `sanitize_session_id` contract, NaN trap
- [x] NOT in scope section is explicit
- [x] Output file is in `product/features/bugfix-523/specification/` only
- [x] No placeholder or TBD sections — SD-01 is an explicit decision, not a TBD
- [x] Knowledge Stewardship report block included

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entries #4132 (NaN trap lesson, confirms
  pattern and error variant), #3902 (sanitize_session_id omission lesson), #3461
  (operator-togglable debug logging pattern) were most relevant. Entry #3921 is deprecated;
  content confirmed valid against source code.
