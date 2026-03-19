# Agent Report: nan-007-agent-2-spec

**Agent**: Specification Writer
**Feature**: nan-007 — Evaluation Harness (W1-3)
**Output**: `product/features/nan-007/specification/SPECIFICATION.md`

---

## Deliverable

`/workspaces/unimatrix/product/features/nan-007/specification/SPECIFICATION.md`

---

## Key Requirements Decisions

**AC split (SR-04 applied)**: Acceptance criteria are divided into two independent
groups. Group 1 (AC-01 through AC-09, AC-15) covers D1–D4 with no daemon dependency.
Group 2 (AC-10 through AC-14) covers D5–D6 and requires the `daemon_server` pytest
fixture. AC numbering follows SCOPE.md exactly to preserve traceability.

**Analytics queue suppression elevated to FR-16 (SR-07 applied)**: The constraint
is promoted from a risk note to a functional requirement with explicit enforcement
language. `EvalServiceLayer::from_profile()` must disable the in-memory analytics
queue — an `AnalyticsMode::Disabled` variant or equivalent named design — not merely
rely on the read-only SQLite mode. The SQLite mode alone does not stop the in-memory
queue from being populated.

**Dual-mode P@K semantics confirmed (FR-20)**: query-log-sourced scenarios use
`baseline.entry_ids` as soft ground truth; hand-authored scenarios use `expected` as
hard labels. This was a pre-resolved decision from SCOPE.md and is now a numbered
testable requirement with verification method in AC-07.

**`ConfidenceWeights` error quality made testable (FR-18, SR-08)**: The invariant
violation error must be user-readable and name the actual sum. This converts SR-08
from a risk recommendation into a testable functional requirement.

**`SqlxStore` not modified (FR-24)**: The decision to use a raw `sqlx::SqlitePool`
with `SqliteConnectOptions::read_only(true)` rather than adding `SqlxStore::open_readonly()`
is codified as a constraint, keeping eval infrastructure self-contained.

**Snapshot anonymization explicitly removed from scope**: The SCOPE.md resolved this;
the spec carries it as an explicit NOT in scope item and constraint (C-12).

**Report subcommand has no CI gate logic (FR-29, C-07)**: Explicitly specified as
a constraint with SR-06 reference to prevent downstream features from adding automated
pass/fail logic to the `report` subcommand.

---

## Open Questions Forwarded to Architect

1. **OQ-1 (High)**: Hook socket path — does `ProjectPaths` expose the hook socket
   path, or does it need a new field? Must be resolved before D6 implementation.

2. **OQ-2**: `VACUUM INTO` sync vs. async — rusqlite or sqlx with `block_on`?

3. **OQ-3 (High)**: `kendall_tau()` accessibility — is it behind `#[cfg(test)]` or a
   feature flag? If so, how is it made available to the production eval binary?

4. **OQ-4**: Nested `eval` subcommand dispatch ordering relative to C-10 pre-async rule.

5. **OQ-5**: Vector index sharing across profiles — separate per profile or shared
   read-only? Architect to estimate memory at 50k entries.

---

## Self-Check

- [x] SPECIFICATION.md covers all 15 acceptance criteria from SCOPE.md (AC-01 through AC-15, all present)
- [x] Every functional requirement is testable
- [x] Non-functional requirements include measurable targets (60s snapshot, 5 scenarios/sec, 8 GB / 50k entries)
- [x] Domain Models section defines all key terms
- [x] NOT in scope section is explicit
- [x] Output file is in `product/features/nan-007/specification/` only
- [x] No placeholder or TBD sections — unknowns are flagged as Open Questions
- [x] Knowledge Stewardship report block included

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "eval harness snapshot scenario extraction ranking
  metrics acceptance criteria" — found entry #749 (Calibration Scenario Builder Pattern)
  and entry #425 (Three-Slot Model Registry). Entry #749 directly informed FR-22 (reuse
  directive) and OQ-3 (cfg(test) accessibility concern).
- Queried: `/uni-query-patterns` for "acceptance criteria verification method split
  offline live daemon fixture pattern" — found entry #1928 (daemon fixture) which
  grounded the Group 1 / Group 2 AC split and daemon_server fixture requirement.
