# Agent Report: crt-047-agent-3-cycle-review-index

**Feature:** crt-047 — Curation Health Metrics
**Component:** schema / cycle_review_index
**Agent ID:** crt-047-agent-3-cycle-review-index

---

## Task Summary

Extended `CycleReviewRecord` with 7 new curation health fields, replaced
`INSERT OR REPLACE` with a two-step upsert preserving `first_computed_at`,
added `get_curation_baseline_window()`, added `CurationBaselineRow` struct,
bumped `SUMMARY_SCHEMA_VERSION` from 1 to 2, and wrote all test cases from
the component test plan.

---

## Files Modified

- `crates/unimatrix-store/src/cycle_review_index.rs` — primary implementation
- `crates/unimatrix-store/src/lib.rs` — re-export `CurationBaselineRow`
- `crates/unimatrix-store/src/retention.rs` — added `..Default::default()` to
  `CycleReviewRecord` struct literal to accommodate the 7 new fields

---

## Changes Implemented

### `CycleReviewRecord` (7 new fields)
All `i64`, defaulting to `0` via `#[derive(Default)]`:
- `corrections_total`, `corrections_agent`, `corrections_human`, `corrections_system`
- `deprecations_total`, `orphan_deprecations`, `first_computed_at`

`Default` was added to the struct so existing callers throughout the workspace
(retention.rs, status.rs, tools.rs) can use `..Default::default()` for the
new fields rather than requiring full struct literal updates.

### `CurationBaselineRow` (new struct)
Slim projection for baseline computation: `corrections_total`, `corrections_agent`,
`corrections_human`, `deprecations_total`, `orphan_deprecations`, `schema_version`.
Re-exported from `lib.rs`.

### `store_cycle_review()` — two-step upsert (ADR-001)
Replaced `INSERT OR REPLACE` with:
1. `SELECT first_computed_at ... fetch_optional` on the write connection
2. `None` → `INSERT` with `record.first_computed_at` (caller-supplied)
3. `Some(_)` → `UPDATE` with `first_computed_at` excluded from SET clause

Pre-crt-047 rows with `first_computed_at = 0` (migration DEFAULT) keep `0` on
force=true rewrites — this is intentional per ADR-001. Not "fixed".

### `get_cycle_review()` — extended SELECT
Now selects all 12 columns and maps positions 5–11 to the 7 new fields.

### `get_curation_baseline_window(n: usize)`
New method using `read_pool()`:
```sql
SELECT corrections_total, corrections_agent, corrections_human,
       deprecations_total, orphan_deprecations, schema_version
FROM cycle_review_index
WHERE first_computed_at > 0
ORDER BY first_computed_at DESC
LIMIT ?1
```

### `SUMMARY_SCHEMA_VERSION`
Bumped from `1` to `2`. Triggers stale-record advisory on all pre-v24 rows
when `context_cycle_review force=false` is called. Designed behavior per
crt-033 ADR-002.

---

## Test Results

**263 passed, 0 failed** (`cargo test -p unimatrix-store`)

New tests added covering all CRS-V24-U-* and EC-04 scenarios from the test plan:

| Test | Scenario |
|------|----------|
| `test_summary_schema_version_is_two` | CRS-V24-U-01: constant = 2 |
| `test_cycle_review_record_v24_round_trip` | CRS-V24-U-02: all 7 fields round-trip |
| `test_store_cycle_review_preserves_first_computed_at_on_overwrite` | CRS-V24-U-03: R-07 regression |
| `test_store_cycle_review_first_write_sets_first_computed_at` | CRS-V24-U-04 |
| `test_get_curation_baseline_window_excludes_zero_first_computed_at` | CRS-V24-U-05 |
| `test_get_curation_baseline_window_ordered_by_first_computed_at_desc` | CRS-V24-U-06 |
| `test_get_curation_baseline_window_ordering_verified` | CRS-V24-U-06b: ordering via distinct corrections_total |
| `test_get_curation_baseline_window_caps_at_n` | CRS-V24-U-07: LIMIT boundary |
| `test_force_true_historical_does_not_perturb_baseline_window_order` | CRS-V24-U-08 |
| `test_corrections_system_round_trips_through_store` | CRS-V24-U-09: R-09 guard |
| `test_get_curation_baseline_window_empty_when_no_qualifying_rows` | CRS-V24-U-10: FM-03 |
| `test_concurrent_force_true_preserves_first_computed_at` | EC-04: concurrent force=true |

`test_summary_schema_version_is_one` renamed to `test_summary_schema_version_is_two`.

---

## Cross-Crate Impact Note

`CycleReviewRecord` is constructed with full struct literals in
`unimatrix-server/src/services/status.rs` and `unimatrix-server/src/mcp/tools.rs`.
Those files will not compile until those struct literals are updated with
`..Default::default()`. That work is in scope for the server-layer agents
(agent-5 tools.rs, agent-6 status.rs). The `Default` derive on `CycleReviewRecord`
makes the fix trivial in each case.

---

## Issues / Blockers

None. All constraints from ADR-001 (two-step upsert), ADR-002 (`corrections_system`
stored), and ADR-003 (ENTRIES-only orphan attribution) are correctly implemented.
The `get_curation_baseline_window()` WHERE clause and ORDER BY match the spec exactly.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #4178 (cycle_review_index
  column placement pattern) and entry #4185 (two-step upsert test pattern), both
  directly applicable. Confirmed no prior implementation pattern for the two-step
  upsert itself existed.
- Stored: entry #4186 "Two-step upsert for first-write-wins column preservation:
  SELECT on write connection, then INSERT or UPDATE" via `/uni-store-pattern`.
  Captures the implementation pattern (SELECT on write connection → INSERT or UPDATE)
  as distinct from the existing test pattern entry #4185. Future agents adding
  immutable-after-first-write columns won't need to rediscover the DELETE+INSERT
  behavior of `INSERT OR REPLACE`.
