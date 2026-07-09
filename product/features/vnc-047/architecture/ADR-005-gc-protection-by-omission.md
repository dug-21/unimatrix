## ADR-005 vnc-047: GC durability is by **omission** from retention DELETE paths, not membership in a "protected set" — plus a regression-test extension

### Context

AC-04 / SR-09 require `cycle_tags` to survive the cycle-based telemetry GC. The SCOPE (and issue #940)
phrase this as "register `cycle_tags` in the retention protected-table set." Reading `retention.rs`
at HEAD, **there is no protected-table set data structure to register in.** Protection is by
construction:

- `gc_cycle_activity` (retention.rs:116) deletes an **explicit closed list** — `observations`,
  `query_log`, `injection_log`, `sessions` — joined through `sessions.feature_cycle`.
- `gc_unattributed_activity` (retention.rs:202) deletes the same four tables for unattributed sessions.
- `gc_audit_log` is a no-op (audit_log is trigger-protected).

A table is "protected" **iff no DELETE statement in `retention.rs` names it.** `cycle_events`,
`cycle_review_index`, `entries`, `observation_phase_metrics` are protected purely because nothing
deletes them. The load-bearing guarantee is enforced only by `test_gc_protected_tables_regression`
(retention.rs:521-643), which snapshots before/after counts of those tables and asserts equality.

### Decision

`cycle_tags` is GC-protected **by omission**:

1. **Do NOT add `cycle_tags` to any DELETE path** in `retention.rs` (neither `gc_cycle_activity` nor
   `gc_unattributed_activity`). No positive registration step exists or is needed — the table is
   protected the moment it is not referenced by a delete.
2. **Extend `test_gc_protected_tables_regression`** (retention.rs:521): insert `cycle_tags` rows for a
   `feature_cycle` whose sessions are purged, snapshot the `cycle_tags` count before GC, run the full
   GC pass, and assert the count is unchanged after — mirroring the existing `cycle_events` /
   `cycle_review_index` assertions (:622-636). This test is the only thing that will catch a future
   change that accidentally starts deleting `cycle_tags`.

This corrects the SCOPE/#940 mental model: the deliverable is "prove omission with a test," not
"register in a set." No `feature_cycle` FK exists (ADR-001), so tags are not CASCADE-deleted when
`sessions` rows go — reinforcing that omission alone is sufficient.

### Consequences

- Easier: nothing to wire — the table is protected by default; the risk is only a *future* regression.
- Harder / load-bearing: the protection is invisible in the code (an absence), so the regression test
  is the sole guardrail (SR-09) — it must be added, or the guarantee is unverified.
- The test must exercise a purged cycle (sessions actually deleted) to be meaningful, not an untouched
  one; reuse the existing purgeable-cycle fixture in the same test.
- Cross-ref ADR-001 (no FK → no CASCADE path either).
