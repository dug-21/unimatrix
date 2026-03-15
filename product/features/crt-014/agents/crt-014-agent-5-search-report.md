# Agent Report: crt-014-agent-5-search

**Feature**: crt-014 — Topology-Aware Supersession
**Component**: `search.rs` (MODIFIED) + `pipeline_retrieval.rs` (MODIFIED)
**Branch**: `feature/crt-014`
**Commit**: `0c3e34f`

---

## Summary

Implemented all search.rs changes for crt-014 per pseudocode and test plan. Both files modified, all tests pass, AC-14 satisfied.

---

## Files Modified

- `/workspaces/unimatrix-crt-014/crates/unimatrix-server/src/services/search.rs`
- `/workspaces/unimatrix-crt-014/crates/unimatrix-engine/tests/pipeline_retrieval.rs`

---

## Changes Made

### search.rs

1. **Import line**: Added `use unimatrix_engine::graph::{FALLBACK_PENALTY, GraphError, build_supersession_graph, find_terminal_active, graph_penalty}`. Added `CoreError` to `unimatrix_core` imports.

2. **Graph construction (before Step 6a)**: Added `spawn_blocking` block that queries all four status variants (Active, Deprecated, Proposed, Quarantined) via `store.query_by_status()` to build `all_entries`, then calls `build_supersession_graph`. Cycle detection sets `use_fallback = true` with `tracing::error!`.

3. **Step 6a (Flexible mode)**: Replaced two-branch penalty logic (`SUPERSEDED_PENALTY` / `DEPRECATED_PENALTY`) with unified condition `entry.superseded_by.is_some() || entry.status == Status::Deprecated` dispatching to `graph_penalty()` or `FALLBACK_PENALTY` (IR-02).

4. **Step 6b (successor injection)**: Replaced single-hop `entry.superseded_by` lookup with `find_terminal_active()` multi-hop traversal. Fallback mode restores single-hop behavior per ADR-005.

5. **Tests migrated (8 tests)**:
   - T-SP-01, T-SP-05, T-SP-07: `DEPRECATED_PENALTY` → `ORPHAN_PENALTY`
   - T-SP-02, T-SP-06: `SUPERSEDED_PENALTY` → `CLEAN_REPLACEMENT_PENALTY`
   - T-SP-04: renamed `superseded_harsher_than_orphan_deprecated`, asserts `CLEAN_REPLACEMENT_PENALTY < ORPHAN_PENALTY`
   - T-SP-08: updated to three-tier topology ordering
   - crt-018b tests: `DEPRECATED_PENALTY` → `ORPHAN_PENALTY`, `SUPERSEDED_PENALTY` → `CLEAN_REPLACEMENT_PENALTY`

6. **New tests added (3)**:
   - `penalty_map_uses_graph_penalty_not_constant` (AC-12)
   - `cycle_fallback_uses_fallback_penalty` (AC-16)
   - `unified_penalty_guard_covers_superseded_active_entry` (IR-02)

### pipeline_retrieval.rs

- Removed local `const DEPRECATED_PENALTY` and `const SUPERSEDED_PENALTY` shims
- Added import: `use unimatrix_engine::graph::{CLEAN_REPLACEMENT_PENALTY, FALLBACK_PENALTY, ORPHAN_PENALTY, PARTIAL_SUPERSESSION_PENALTY}`
- T-RET-02: updated to use `ORPHAN_PENALTY` / `CLEAN_REPLACEMENT_PENALTY` with explanatory comment
- T-RET-05: updated deprecated score to use `ORPHAN_PENALTY`
- Added `test_topology_penalty_behavioral_ordering` (new — documents constant ordering semantics)

---

## Deviation from Pseudocode

**IR-01 / graph construction**: The pseudocode instructed `Store::query(QueryFilter::default())` claiming it "returns all entries regardless of status". This is incorrect — `QueryFilter::default()` with all-None fields triggers `effective_status = Some(Status::Active)` in the implementation, returning only Active entries.

**Resolution**: Used `store.query_by_status(status)` for each of the four `Status` variants and combined results. This correctly satisfies IR-01 without schema changes. Pattern stored as #1588 in Unimatrix.

**ServiceError**: Pseudocode referenced `ServiceError::Internal(String)` which does not exist. Used `ServiceError::EmbeddingFailed` for `spawn_blocking` JoinError (following the existing embed step pattern) and `ServiceError::Core(CoreError::Store(e))` for store errors.

---

## Test Results

**All tests pass. Zero failures.**

```
cargo test --workspace: all test result lines show 0 failed
```

Total across workspace: 2,526+ tests passing (matching pre-wave count + new tests added).

---

## AC-14 Verification

```bash
grep -rn "DEPRECATED_PENALTY|SUPERSEDED_PENALTY" crates/ --include="*.rs" \
  | grep -v "#\[cfg(test)\]" | grep -v "// "
```

Result: Lines 1188/1192 in search.rs are string literals inside test assertion messages (inside `#[cfg(test)]` block), not identifier references to removed constants. Zero production-code references.

---

## Knowledge Stewardship

- **Queried**: `/uni-query-patterns` for `unimatrix-server search service` — no directly relevant patterns found for all-status store load pattern
- **Stored**: entry #1588 "Store::query(QueryFilter::default()) returns Active-only — use query_by_status per variant for full-store reads" via `/uni-store-pattern`

This is a critical runtime gotcha: code compiles and runs correctly with `QueryFilter::default()` for Active-only scenarios, but silently drops Deprecated entries when full-store coverage is needed. The next agent needing all-status reads will find this pattern.
