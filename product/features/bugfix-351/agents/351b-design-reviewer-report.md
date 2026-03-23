# Agent Report: 351b-design-reviewer

**Feature**: bugfix-351 (extraction noise)
**Role**: Design Reviewer — proposed Fix 2 and Fix 3 only. No implementation.
**Date**: 2026-03-23

---

## Assessment: APPROVED WITH NOTES

Both fixes are architecturally sound and address real regressions. Neither introduces new hot-path
hazards. Four non-blocking notes refine the implementation approach; none require rework.

---

## Findings

### Fix 2 — `fetch_recent_observations_for_dead_knowledge`: Add `WHERE ts_millis >= ?` cutoff

**Verdict: APPROVED WITH NOTES**

**Hot-path risks**: None introduced. The function is called from `dead_knowledge_deprecation_pass`,
which is step 12 of `maintenance_tick` — an async background task running on a 15-minute interval,
not on any request-serving path. Adding a `WHERE` predicate reduces work; it cannot increase it.
The `idx_observations_ts` index exists (confirmed in `db.rs:704` and all migration tests), so the
predicate will be an index seek, not a scan. LIMIT 5000 is retained as a secondary guard, which is
correct. The function uses `write_pool_server()`, consistent with the existing pattern for
maintenance-path reads that must avoid read-pool contention.

**Blast radius if subtly wrong**: The worst case is that the cutoff timestamp arithmetic is off
by a sign or unit (millis vs. seconds), causing the predicate to return zero rows. In that case
`observations.is_empty()` returns true at line 940 and the pass exits cleanly with 0 deprecations.
No data loss; the next tick would retry. A cutoff computed too far in the past widens the window
(reduces the fix's effectiveness) rather than causing incorrect deprecations. Risk is bounded.

**Note 1 (non-blocking) — Named constant for the 7-day cutoff.**
The 7-day duration should be a named constant in `background.rs`, not an inline literal. The file
already has `DEAD_KNOWLEDGE_MIGRATION_CAP` and `DEAD_KNOWLEDGE_MIGRATION_V1_KEY` as module-level
constants. Suggest:

```rust
const DEAD_KNOWLEDGE_OBSERVATION_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000; // 7 days in millis
```

Caller: `let cutoff = now_ms - DEAD_KNOWLEDGE_OBSERVATION_WINDOW_MS;`

This makes the intent legible at the call site and keeps the magic number out of the SQL call.
No strong case for making it configurable — 7 days is not a tunable operational parameter, it is a
detection heuristic tied to "recent enough to matter," and dead knowledge candidates already require
5 distinct sessions to qualify. Hardcoded named constant is appropriate here.

**Note 2 (non-blocking) — 7-day window vs. 5-session window alignment.**
The detection window (`window = 5` sessions) operates on whatever observations are in the slice.
If the 7-day cutoff produces fewer than 5 distinct sessions (low-activity deployment), the function
returns `None` at line 55 of `dead_knowledge.rs` and the pass exits cleanly. This is the correct
and safe behaviour — the existing guard already handles sparse data. The 7-day bound is a
reasonably conservative upper bound for a 5-session window; a typical session spans minutes to
hours, so 7 days covers hundreds of sessions at normal usage rates. No misalignment risk.

**Note 3 (non-blocking) — Confirm `cutoff_millis` parameter type consistency.**
The existing observation fetch uses `row.get::<i64, _>(0)` for `ts_millis`. The proposed parameter
type `i64` is consistent with this. Ensure the `now_millis` computation in the caller uses
`SystemTime::now().duration_since(UNIX_EPOCH)...as_millis() as i64` (or equivalent), not
`as_secs()` — a seconds-vs-millis mismatch would produce a cutoff in 1970.

---

### Fix 3 — `existing_entry_with_title`: Replace `query_by_topic` with `title_exists_in_topic` EXISTS query

**Verdict: APPROVED WITH NOTES**

**Hot-path risks**: None introduced. `existing_entry_with_title` is called from `RecurringFrictionRule::evaluate`,
which is an `ExtractionRule` running inside the extraction tick — a background path, not a
request-serving path. The extraction tick already uses `block_in_place` / `spawn_blocking`
for synchronous store operations (confirmed by the existing test flavor `multi_thread` requirement).
The new EXISTS query replaces a full materialized topic scan + tag vector load for every candidate
title. It is strictly cheaper. The existing `read_pool()` usage in `query_by_topic` should be
preserved in the new method.

Unimatrix entry #3299 confirms this is a known anti-pattern: "query_by_topic as dedup guard is an
anti-pattern — use EXISTS(topic, title) instead of full topic load + Rust filter."

**Blast radius if subtly wrong**: If the EXISTS query returns an incorrect `false` (e.g., empty
result due to a bug), the dedup guard fails open — a duplicate entry is inserted. This is the same
fail-open default as the current code's `Err(_) => false` path. If it returns incorrect `true`
(false positive), a valid proposal is suppressed for one tick, then re-evaluated next tick. In both
cases the blast radius is bounded and self-correcting.

**Note 4 (non-blocking, most important) — `title_exists_in_topic` must filter on `status = 'active'`.**
`query_by_topic` returns all entries regardless of status (its SQL is `WHERE topic = ?1` with no
status filter, confirmed in `read.rs:192`). The current `.any(|e| e.title == title)` check
therefore suppresses proposals when a deprecated entry with the same title exists, even if that
deprecated entry is stale and should be replaced.

The proposed `title_exists_in_topic` is the right place to fix this latent bug. The correct SQL is:

```sql
SELECT EXISTS(
    SELECT 1 FROM entries
    WHERE topic = ?1 AND title = ?2 AND status = 'active'
)
```

Without the `status = 'active'` filter, a deprecated or quarantined entry with the same title
permanently blocks re-creation — which is incorrect behaviour. This is not hypothetical: the
one-shot migration in `run_dead_knowledge_migration_v1` bulk-deprecates old lesson-learned entries,
and those deprecated entries share titles with the ones `RecurringFrictionRule` would re-generate.
Without an active-status filter the dedup guard would suppress all future proposals for those
exact titles.

**Placement in `read.rs`**: Correct. All narrow-predicate, index-seeking read helpers live in
`read.rs`. The method is not part of the `EntryStore` trait surface (it is an existence check
specific to the extraction pipeline, not a general query primitive), so it should be a concrete
`impl SqlxStore` method in `read.rs`, not added to the trait.

---

## Security Surface

No new trust boundaries. Both fixes use parameterized queries (`?1`, `?2`), continuing the
project convention (ADR-004: Mandatory Named Parameters for Multi-Column SQL). No string
interpolation into SQL. No new input sources — both operate on internally-derived values
(`cutoff_millis` computed from `SystemTime::now()`, `title` from a statically-constructed format
string). No injection vectors.

---

## Summary of Required Changes to Proposed Fixes

| # | Fix | Change | Severity |
|---|-----|--------|----------|
| 1 | Fix 2 | Extract 7-day duration as `DEAD_KNOWLEDGE_OBSERVATION_WINDOW_MS` constant | Non-blocking |
| 2 | Fix 2 | Verify caller computes cutoff in millis, not seconds | Non-blocking |
| 3 | Fix 2 | 7-day window is appropriate given 5-session detection guard | Confirmation only |
| 4 | Fix 3 | `title_exists_in_topic` SQL must include `AND status = 'active'` | Non-blocking but important |

None of these block approval. The implementer should incorporate Note 4 and Note 1 in the same
commit as the fixes themselves.

---

## Knowledge Stewardship

**Queried:**
- `database query hot path spawn_blocking background maintenance` — surfaced #1688, #735, #1366, #819 (spawn_blocking and tick loop patterns)
- `SQL query read layer store pattern existence check` — surfaced #3299 (EXISTS anti-pattern confirmation), #1588 (query status filter gotcha)
- `dead knowledge detection observation window sessions tick interval` — surfaced #3252, #3254, #1542 (extraction rule vs maintenance action boundary)
- `named constant configuration cutoff window time predicate SQL index` — surfaced #3298 (time-window lesson for this exact function)

**Stored:** Declined. The findings in this review are consistent with existing Unimatrix entries
(#3299, #3298, #3252, #3254). No new generalizable pattern emerged that is not already captured.
Note 4 (status filter in EXISTS) is a feature-specific refinement to the Fix 3 approach, not a
recurring pattern worth storing independently.
