# Agent Report — crt-055 Component 3: Aggregate Reckoning (rank 1/2/3)

**Agent**: crt-055-agent-3-aggregate_reckoning
**Crate**: unimatrix-observe
**Wave**: 2

## Summary

Implemented rank-1/2/3 durable aggregate reckoning in a new module
`cycle_aggregates.rs`. All values are `i64`; ratios are num/den pairs (never
pre-divided). The module produces values **into** a `CycleAggregates`; it does NOT
write the `cycle_review_index` table (Component 2's single `store_cycle_review` writer
owns persistence).

## Files modified

- `crates/unimatrix-observe/src/cycle_aggregates.rs` (new — reckoning functions)
- `crates/unimatrix-observe/src/cycle_aggregates/tests.rs` (new — 21 unit tests)
- `crates/unimatrix-observe/src/lib.rs` (registered module + public re-exports)
- `crates/unimatrix-observe/src/fail_loud_guard.rs` (added the missing
  `phase_total_duration_secs: i64` field to the shared `CycleAggregates` struct + its
  test helper — see Issue 1)

## What was implemented

- **Rank-1** `reckon_phase_aggregates(&[CycleEventRecord]) -> PhaseAggregates`:
  `phase_count` (distinct names), `phase_transition_count` (cycle_phase_end events),
  `phase_rework_count` (re-entries), `phase_unclosed_count` (#556, open after walk with
  no cycle_stop), `phase_total_duration_secs` (Σ closed-window seconds). A
  closed-then-reopened phase is rework, not a new phase. A `cycle_stop` closes the final
  phase so it is not a false never-closed (R-14/R-15).
- **Rank-2** `reckon_rework_ratio(&[S: SessionOutcome]) -> (i64, i64)`: returns the
  `(rework, total)` PAIR. `is_rework_outcome` reuses the EXACT tools.rs Step 15 classifier
  (`contains("result:rework") || contains("result:failed")`, case-insensitive).
  `SessionOutcome` is impl'd for `unimatrix_store::SessionRecord`.
- **Rank-3** `reckon_knowledge_reuse_served(&[QueryLogRecord], &[InjectionLogRecord]) -> i64`:
  size of the UNION of `query_log.result_entry_ids` (JSON array) ∪ `injection_log.entry_id`,
  deduped via `HashSet<u64>` (entry in both logs counts once). Mirrors the established
  `compute_knowledge_reuse_for_sessions` union path in tools.rs.
- `populate_rank_1_2_3(...)` convenience that writes all 8 rank-1/2/3 fields onto a
  `CycleAggregates` for the review pipeline.

## Source resolution (Open Q1 — table/column names)

Resolved against the live schema, NOT invented:
- `query_log(session_id, result_entry_ids TEXT /* JSON array */, ...)` — migration.rs:265.
- `injection_log(session_id, entry_id INTEGER, ...)` — migration.rs:1738.
- Neither log carries a feature/cycle column; "served to this cycle" is resolved by the
  handler through the cycle's attributed `session_id`s (via `scan_query_log_by_sessions` /
  `scan_injection_log_by_sessions`), then passed to the pure reckoner.
- Negative-control test `test_rank3_wrong_table_name_yields_silent_zero_guard` asserts a
  seeded non-empty union is non-zero, failing loudly if the load path is misnamed.

## Tests

`cargo test -p unimatrix-observe --lib`: **551 passed, 0 failed**
(21 new cycle_aggregates tests; rest pre-existing, none regressed.)

Coverage maps to plan: AC-04 (#556 unclosed + false-positive guard), AC-05 (rework vs
new-phase, num/den pair, duration), AC-06 (#320 union + both-logs dedup + same-log dedup +
silent-zero negative control), AC-11 (evicted-session honest-partial zero). Plus
order-independence, malformed-JSON robustness, and auto_close coupling.

`cargo build -p unimatrix-observe`: clean. `cargo clippy`: no findings in the new file
(only pre-existing warnings elsewhere). `cargo fmt`: applied.

## Issues / blockers

1. **`CycleAggregates` was missing `phase_total_duration_secs`** (a v5 column per
   OVERVIEW.md line 67). The Wave-1 `CycleAggregates` in `fail_loud_guard.rs` did not carry
   it. I added the field (+ updated the one struct-literal test helper). No other crate
   constructs `CycleAggregates` literally, so this is contained. **Flag for Component
   1/2/7**: the schema column, store_cycle_review bind list, and presentation must include
   `phase_total_duration_secs` too.
2. **Pseudocode deviation (flagged, not silent)**: the pseudocode assumed a
   `cycle_phase_start` event type. The live `cycle_events` model has none — phases are
   declared via `next_phase` on `cycle_start`/`cycle_phase_end`. Implemented to the real
   model (mirrors `compute_phase_stats`/`build_phase_narrative`); a literal match on
   `cycle_phase_start` would have silently produced `phase_count=0`. The pseudocode itself
   instructed verifying the literals against the live writer, so this is the
   intended resolution.
3. **No git operations performed** (Delivery Leader owns git, per spawn prompt).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search -- surfaced ADR-004 (#5039,
  column shapes), pattern #4178 (derived aggregates belong on cycle_review_index),
  lesson #4140 (evicted-session attribution loss → honest partial). All applied.
- Stored: entry #5063 "Rank-1/2/3 cycle aggregate reckoning: next_phase-driven phase model,
  no cycle_phase_start literal" via /uni-store-pattern.
