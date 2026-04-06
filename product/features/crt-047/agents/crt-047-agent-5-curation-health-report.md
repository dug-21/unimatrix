# Agent Report: crt-047-agent-5-curation-health

**Feature**: crt-047 — Curation Health Metrics  
**Component**: `services/curation_health.rs`  
**Agent ID**: crt-047-agent-5-curation-health

---

## Files Created / Modified

- `crates/unimatrix-server/src/services/curation_health.rs` — created (492 lines production code + 1064 lines tests)
- `crates/unimatrix-server/src/services/mod.rs` — `pub(crate) mod curation_health` added (by Phase 7c agent before this agent ran; confirmed present)
- `crates/unimatrix-server/src/services/status.rs` — added `curation_health: None` to StatusReport struct literal (one-line cascade fix)
- `crates/unimatrix-server/src/mcp/tools.rs` — added `..Default::default()` to CycleReviewRecord literal and integrated curation snapshot Step 8a (wave 2 commit)
- `crates/unimatrix-observe/src/types.rs` — curation types (CurationSnapshot, CurationBaselineComparison, CurationHealthSummary, CurationHealthBlock, TrendDirection) — wave 2 commit
- `crates/unimatrix-observe/src/lib.rs`, `report.rs`, `phase_narrative.rs` — re-exports and RetrospectiveReport field — wave 2 commit
- `crates/unimatrix-server/src/mcp/response/retrospective.rs`, `response/mod.rs` — response struct updates — wave 2 commit

---

## Implementation Notes

### Architecture Discovery: Types Live in unimatrix-observe

The spawn prompt described types as living in `services/curation_health.rs`. During implementation, discovered that Wave 1 (cycle_review_index and migration agents) placed curation types in `unimatrix_observe::types` to serve as the canonical serialization boundary. `curation_health.rs` uses `unimatrix_observe::{CurationSnapshot, CurationBaselineComparison, ...}` and re-exports them. This is consistent with ADR-005.

### SQL Status Values Are Integers

The pseudocode specified `WHERE status = 'deprecated'` but the ENTRIES table stores Status as `INTEGER` (Active=0, Deprecated=1, Proposed=2, Quarantined=3). The string comparison silently returned zero rows — all three deprecation/orphan tests failed with count=0 until fixed to `WHERE status = 1`. Stored as pattern #4187.

### read_pool() Cross-Crate Access

`SqlxStore::read_pool()` is `pub(crate)` in unimatrix-store — not accessible from unimatrix-server. Used `write_pool_server()` (which is `pub`) for the ENTRIES SQL reads in `compute_curation_snapshot()`. This matches the existing pattern in `status.rs` (entry #3028 in Unimatrix).

### Phase 7c Agent Pre-Committed curation_health.rs

Commit `73c15b09` from the Phase 7c (context_status) agent already included `curation_health.rs` with the full implementation. This agent validated and refined that implementation (fixing the SQL status integer bug and schema import issue in tests).

---

## Test Results

**All tests pass:**

```
test result: ok. 2820 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Curation health tests specifically:

```
test services::curation_health::tests::... (44 tests) — all ok
```

Tests implemented per test plan:
- CH-U-01 through CH-U-07: `compute_curation_snapshot` async tests (trust_source bucketing, window filtering, orphan logic, fallback)
- CH-U-08 through CH-U-15: `compute_curation_baseline` pure tests (boundary, legacy rows, NaN guards)
- CH-U-16 through CH-U-17: `compare_to_baseline` sigma tests
- CH-U-18 through CH-U-22: `compute_trend` boundary and direction tests
- Additional `compute_curation_summary` tests
- Constants assertions (AC-16)
- Cold-start boundary suite (2/3/5/6/10 rows)

---

## Issues Encountered

1. **Types in unimatrix-observe**: The spawn prompt described self-contained types in `curation_health.rs`. Wave 1 had placed them in `unimatrix-observe`. Adjusted to import from there — no deviation from architecture intent (ADR-005).

2. **SQL integer status**: Pseudocode used `status = 'deprecated'` (string). Actual schema stores integer. Fixed to `status = 1`. Three tests caught this immediately.

3. **Cascade compilation fixes**: Two other files needed minor fixes to compile:
   - `status.rs` missing `curation_health: None` in one struct literal
   - `tools.rs` missing `..Default::default()` on `CycleReviewRecord` literals

---

## Knowledge Stewardship

- **Queried**: `mcp__unimatrix__context_briefing` — surfaced entries #4184 (ADR-005), #3028 (read_pool pub(crate)), #2151 (read/write pool split). All applied.
- **Stored**: entry #4187 "SQLite Status enum is INTEGER — query deprecated entries with status = 1, not status = deprecated string" via `/uni-store-pattern`
