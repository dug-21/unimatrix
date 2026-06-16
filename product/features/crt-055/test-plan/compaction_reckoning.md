# Test Plan — compaction_reread reckoning + compaction_events read accessor

**Component**: `unimatrix-observe` (gate) + `compaction_events` read accessor in `unimatrix-store`
**Risks**: R-08 (gate clock/unit mismatch + boundary selection — CRITICAL), R-05 (attribution silent no-op #4140 — CRITICAL), R-12 (producer-contract drift)
**ACs**: **AC-22** (clock/unit — INTEGRATION, mandated), AC-12 (gate + boundary), AC-11 (attribution count)

> Binding contract (ADR-006): the gate normalizes ALL timestamps to Unix **seconds** before comparison. Read `ts` is epoch millis (`observations.ts_millis: i64`, `ObservationRecord.ts: u64`) → `÷ 1000` (integer floor, `session_metrics.rs:115` convention); `compacted_at` is Unix seconds (untouched). Gate: `(ts_millis ÷ 1000) > compacted_at`. A millis-vs-seconds mismatch makes every read pass (millis ≫ seconds) or none — a believable-wrong-number. Pattern #4236: an epoch-migration gate needs a three-tier boundary suite; the ±500ms sub-second boundary exercises the ÷1000 floor (a ±1s window would pass even with a broken/absent floor).

## Unit tests

### Read accessor (R-12)
- `test_compaction_events_read_orders_by_compacted_at_asc` — `SELECT compacted_at FROM compaction_events WHERE session_id = ?1 ORDER BY compacted_at ASC` returns ascending seconds.
- `test_compaction_count_counts_attributed_rows` — `compaction_count == COUNT` of attributed rows.

### Gate comparator (R-08, AC-12) — seconds-vs-seconds
- `test_gate_normalizes_read_ts_millis_to_seconds` — a read at `ts_millis = T*1000 + 999` floors to `T` (integer floor); a read at `(T+1)*1000` floors to `T+1`.
- `test_gate_strictly_after_compacted_at_counts` — read with `read_ts_secs > compacted_at` counts; equal-to (`==`) does NOT (strictly-after).
- `test_gate_before_compacted_at_not_counted` — `read_ts_secs < compacted_at` → not counted.

### Boundary selection — multi-compaction (R-08, AC-12)
- `test_multi_compaction_gates_on_earliest_min` (ADR-006) — session with N>1 `compaction_events` rows → gate uses MIN(`compacted_at`); reads after the earliest boundary count.
- `test_reread_counted_at_most_once` — a read after multiple boundaries counts ONCE (no per-boundary double-count).
- `test_compaction_count_reports_all_boundaries` — `compaction_count` reports ALL rows even though the reread gate uses one (AC-11 vs AC-12 distinction).

### Attribution chain (R-05, AC-11) — the #4140 silent no-op
- `test_attribution_declared_session_counts` — `compaction_events` for a declared session (session→`feature_cycle` chain present) → counts toward the cycle.
- `test_attribution_undeclared_session_excluded` (#4140) — an undeclared/evicted session's rows do NOT mis-attribute to the cycle.
- `test_attribution_evicted_session_no_fabricated_zero` (R-05, #4140) — the #4140 condition (SM session drained before `context_cycle(start)`): the cycle surfaces "unavailable" / honest partial, NEVER a fabricated complete-looking zero.

## Integration tests (MCP harness — MANDATED)

### AC-22 — clock/unit consistency (the marquee mandate, cross-table)
- **`test_cycle_review_compaction_reread_seconds_boundary`** (AC-22) — Seed `compaction_events.compacted_at = T` (Unix seconds) and three `observations.ts_millis` reads overlapping the same file. Gate is floor + strict-`>`: `read_ts_secs = ts_millis ÷ 1000` (integer floor), count iff `read_ts_secs > compacted_at`:
  - `T*1000 + 1000` (+1s after) → floor `T+1`; gate `T+1 > T` true → **MUST count**
  - `T*1000 − 500` (−500ms before) → floor `T−1`; gate `T−1 > T` false → **MUST NOT count** (floor-catching guard: an unnormalized millis-vs-seconds gate would wrongly count this; this assertion catches that bug, preserving the #4236 intent)
  - `T*1000` (exact boundary, `ts_millis = T*1000`) → floor `T`; gate `T > T` false (strict `>`) → **NOT counted**
  Assert `compaction_reread_count == 1`. The −500ms case (floors to `T−1`) is the floor-catching tier: it is NOT counted by a correct floor+strict-`>` implementation, but WOULD be wrongly counted by a millis-vs-seconds-unnormalized gate (#4236 boundary-insertion tier). A ±1s window alone would pass even if the floor were absent/wrong.
- **`test_cycle_review_compaction_reread_unit_mismatch_guarded`** (AC-22) — inject an unnormalized millis `ts` compared against a seconds `compacted_at`; assert the comparison stays seconds-vs-seconds and does NOT flip to all-or-nothing (every-read-counts or zero-reads). The ~1000× mis-compare is prevented by the normalization. Cross-table (compaction_events × PostToolUse reads), NOT a comparator unit test.

### AC-11 / AC-12 — count + boundary end-to-end
- `test_cycle_review_compaction_count_vs_reread` (AC-11/12) — multi-compaction session: `compaction_count` reports all rows; `compaction_reread_count` gates on MIN, each read once.
- `test_cycle_review_compaction_attribution_declared_only` (AC-11) — seed declared + undeclared sessions' `compaction_events`; assert only declared rows count; undeclared → no fabricated zero.

## Edge cases (from RISK-TEST-STRATEGY §Edge Cases)
- Session that compacts but never re-reads → `compaction_count > 0`, `compaction_reread_count == 0` (genuine measured zero, distinct from "unavailable").
- Zero `compaction_events` for the cycle → `compaction_count == 0`, `compaction_reread` "unavailable" (no boundary to gate against).
- Read recorded exactly AT `compacted_at` (`==`) → NOT counted (strictly-after).
- `high_water` column present but unread (crt-055 v1 gates on `compacted_at` only — ADR-006) — assert the accessor does not read `high_water`.

## Expected behaviors / assertions summary
- Gate is seconds-vs-seconds: read `ts_millis ÷ 1000` (floor) > `compacted_at`; strictly-after.
- +1s counts (floors to T+1), −500ms does not (floors to T−1), exact boundary does not (strict `>`); unit mismatch caught (never all-or-nothing).
- Multi-compaction gates on MIN; each read counted once; `compaction_count` ≠ reread gate.
- Declared-only attribution; evicted/undeclared never fabricate a zero.
