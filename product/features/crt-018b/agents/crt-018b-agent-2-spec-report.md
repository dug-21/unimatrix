# Agent Report: crt-018b-agent-2-spec

## Output

- `/workspaces/unimatrix/product/features/crt-018b/specification/SPECIFICATION.md`

## Key Requirements Added or Expanded Beyond SCOPE

### SR-04 — Combined Scoring Formula (FR-06)
The SCOPE presented the scoring formula piecewise. The specification consolidates all active
signals into a single combined formula showing how `base_score`, `utility_delta`,
`co_access_boost`, `provenance_boost`, and `status_penalty` interact simultaneously. The spec
verifies that ±0.05 utility delta remains non-dominant across the full crt-019 spread range
(confidence_weight 0.15–0.25), including a concrete worked example at sim=0.95/conf=0.60 to
confirm penalized Ineffective entries still surface in results.

### SR-07 — Tick Error Semantics (FR-09, FR-13)
The SCOPE left the failure mode of `compute_report()` errors underspecified. The spec
mandates hold-on-error semantics: counters are not incremented, not reset, previous state is
retained. A structured `operation = "tick_skipped"` audit event is emitted with the error
string so operators can observe skipped ticks. This prevents false auto-quarantine from
accumulating on stale data.

### SR-03 — Auto-Quarantine Audit Event Schema (FR-11)
The SCOPE noted `operation`, `agent_id`, and `reason`. The spec expands the required audit
event to a nine-field schema: adds `entry_title`, `entry_category`, `classification`,
`consecutive_cycles`, and `threshold`. Each field is justified by the operator recovery
workflow (Workflow 4) so a falsely-quarantined entry can be identified and restored without
querying the database manually.

### SR-06 — BriefingService Constructor (FR-02, Constraint 6)
The SCOPE said "wired into BriefingService via constructor." The spec makes this a hard
requirement: `EffectivenessStateHandle` is a required (non-optional) parameter, not
`Option<EffectivenessStateHandle>`. Incomplete wiring is a compile error. This is elevated
to a named Constraint.

### SR-05 — SETTLED_BOOST Magnitude Constraint (FR-04, Constraint 5)
The spec adds an explicit invariant: `SETTLED_BOOST (0.01) < co-access boost max (0.03)`.
This is stated in FR-04, in the constants table, and as Constraint 5 to ensure the Settled
signal does not displace co-access as the dominant query-time differentiator.

### SR-08 — crt-019 Integration Test Prerequisite (AC-17, Constraint 11)
AC-17 adds a fourth integration test requirement: the fixture must confirm non-zero confidence
spread before exercising utility delta behavior. This ensures the crt-019 adaptive weight
dependency is actually exercised, not bypassed by a flat-confidence test fixture.

### AC-09 — Counter Entry Removal
The SCOPE described increment and reset but did not specify what happens when an entry
disappears from the active set (already quarantined). The spec adds a third case: entry absent
from active classification set is removed from `consecutive_bad_cycles` entirely.

### FR-14 — StatusReport Visibility Field
The SCOPE resolved `auto_quarantined_this_cycle: Vec<u64>` as a field to add. The spec
promotes this to a named functional requirement (FR-14) with a verification method.

## Open Questions

None. All SCOPE open questions were resolved before this specification was written. The
SCOPE-RISK-ASSESSMENT items have been fully addressed as expansions above.

## Knowledge Stewardship

- Queried: /uni-query-patterns for "effectiveness re-ranking utility signal" — #724 pattern
  (Behavior-Based Ranking Tests: Assert Ordering Not Scores) directly informs AC-05 and AC-16
  verification language; #485 (ADR-005 penalty multipliers) confirms the additive/multiplicative
  pattern precedent
- Queried: /uni-query-patterns for "background tick consecutive counter error semantics" —
  #1542 (Background Tick Writers: Define Error Semantics for Consecutive Counters Before
  Implementation) directly matched SR-07 and was the basis for the hold-on-error decision
  in FR-09
- Queried: /uni-query-patterns for "quarantine audit event operator recovery" — vnc-010
  quarantine ADRs (#600, #601) confirmed the restore path and pre_quarantine_status field,
  informing FR-11 field selection
- Queried: /uni-query-patterns for "ConfidenceState Arc RwLock shared state" — #1480
  (parameter-passing over shared state) confirmed the non-optional constructor pattern
