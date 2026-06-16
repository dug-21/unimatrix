# Test Plan — activity collector (`activity_snapshots_for_feature`)

**Component**: `fn activity_snapshots_for_feature(&self, feature_cycle: &str) -> Vec<(String, ActivitySnapshot)>` on `SessionRegistry` — mirrors `take_transcripts_for_feature` (dedup-by-`Arc`, registered ∪ held, filtered by `feature_cycle`); calls `activity_snapshot()` only. Read by crt-055 at review.
**Pseudocode**: `pseudocode/activity-collector.md` · **Layer**: integration.
**Anchor ACs**: AC-06 (held-route reach — shared), **AC-12 (no fabricated zero)**, AC-03 (Surface A independence — shared). **Risks**: R-08, R-01 (shared).

## Selection / dedup — integration

`crates/unimatrix-server/src/infra/session.rs` tests / `transcript_hold_tests.rs`.

1. `test_collector_includes_registered_and_held` — Arrange: a declared `feature_cycle` with one registered session and one drained-but-held session, both folded. Act: `activity_snapshots_for_feature(cycle)`. Assert: both sessions appear, each with its own non-zero snapshot. (Registered ∪ held reach; supports AC-06.)
2. `test_collector_dedup_by_arc` — a session present in both registered and held maps (same `Arc`) appears exactly once (dedup-by-`Arc`, mirrors `take_transcripts_for_feature`). No double-count.
3. `test_collector_filters_by_feature_cycle` — sessions declared for a DIFFERENT `feature_cycle` do not appear in the result set for the queried cycle.

## Late-bind attribution honesty (AC-12, R-08) — INTEGRATION, CRITICAL for honesty

4. `test_undeclared_session_no_activity_entry` — Arrange: an UNDECLARED session (no `feature_cycle`) that folded bytes, then drained (its buffer purges at drain — fold dies). Act: `activity_snapshots_for_feature(any_cycle)`. Assert: the undeclared session contributes NO entry; its bytes do not appear; NO fabricated `0` is emitted on its behalf. Absence is signalled (the session is simply absent from the Vec), never a measured zero. (R-08, AC-12, ADR-004.)
5. `test_absence_distinguishable_from_measured_zero` — a cycle with a declared session that folded zero bytes (an empty but present session) is DISTINCT from a cycle whose session is absent: the former returns an entry with `bytes_total == 0`; the latter returns no entry. crt-054 never fabricates the absent case as a `0`. (AC-12.)

### Negative-mutation
- A collector that defaulted a missing/undeclared session to a zero-valued snapshot must fail `test_undeclared_session_no_activity_entry`.

## Cross-surface independence (AC-03 — shared with compaction-events-writer.md)
- The Surface A `compaction_events` row is written for the undeclared session regardless (writer plan, test `test_compaction_row_written_for_undeclared_session`); Surface B fold dies. The two paths are independent — asserted here (Surface B absence) + writer (Surface A presence).

## Fixtures
- Reuse the `take_transcripts_for_feature` selection-test harness + crt-052 hold fixtures. The collector is the read seam crt-055 consumes; its honesty contract (no fabricated zero) is the load-bearing assertion.
