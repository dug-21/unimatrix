# Agent Report: crt-029-agent-4-store-helpers

## Task

Implement the Store query helpers component for crt-029 (Background Graph Inference).

File modified: `crates/unimatrix-store/src/read.rs`

## Work Done

Added two new async methods to the `Store` impl, placed after `query_graph_edges()` as thematic siblings (pseudocode §Component placement instruction):

### `query_entries_without_edges(&self) -> Result<Vec<u64>>`

Returns IDs of active entries with no non-bootstrap edge on either endpoint. Uses the SQL:

```sql
SELECT id FROM entries
WHERE status = 0
  AND id NOT IN (
    SELECT source_id FROM graph_edges WHERE bootstrap_only = 0
    UNION
    SELECT target_id FROM graph_edges WHERE bootstrap_only = 0
  )
```

Uses `read_pool()` (C-02 compliant).

### `query_existing_supports_pairs(&self) -> Result<HashSet<(u64, u64)>>`

Returns all non-bootstrap Supports edges as normalized `(min, max)` pairs. Uses:

```sql
SELECT source_id, target_id FROM graph_edges
WHERE relation_type = 'Supports' AND bootstrap_only = 0
```

Normalization applied at read time: `(a.min(b), a.max(b))`. Uses `read_pool()` (ADR-004, C-02 compliant).

## Tests

12 new unit tests added to the `#[cfg(test)]` module in `read.rs`, reusing existing `insert_test_entry` and `insert_test_edge` helpers:

| Test | Covers |
|------|--------|
| `test_query_entries_without_edges_empty_store` | AC-15: empty DB returns empty Vec |
| `test_query_entries_without_edges_no_edges` | AC-15: all 3 active entries returned when no edges |
| `test_query_entries_without_edges_with_edges` | AC-15: empty result when all entries covered |
| `test_query_entries_without_edges_partial_coverage` | AC-15: only uncovered IDs returned |
| `test_query_entries_without_edges_bootstrap_only_ignored` | AC-15: bootstrap edges don't count |
| `test_query_entries_without_edges_inactive_excluded` | AC-15: deprecated entries excluded |
| `test_query_existing_supports_pairs_empty` | R-04: empty HashSet on empty table |
| `test_query_existing_supports_pairs_supports_only` | R-04: single pair returned |
| `test_query_existing_supports_pairs_bootstrap_excluded` | R-04: bootstrap_only=1 excluded |
| `test_query_existing_supports_pairs_excludes_contradicts` | R-04: only Supports relation returned |
| `test_query_existing_supports_pairs_mixed_bootstrap` | R-04: mixed bootstrap/non-bootstrap |
| `test_query_existing_supports_pairs_normalization` | R-04: (higher, lower) normalizes to (lower, higher) |

## Test Results

```
test result: ok. 164 passed; 0 failed; 0 ignored
```

(12 new tests + 152 pre-existing, all green)

## Self-Check

- [x] `cargo build -p unimatrix-store` passes (zero errors)
- [x] `cargo test -p unimatrix-store` passes (164/164)
- [x] `cargo fmt -p unimatrix-store` — no changes
- [x] `cargo clippy -p unimatrix-store -- -D warnings` — zero warnings
- [x] No `todo!()`, `unimplemented!()`, `TODO`, `FIXME`, or `HACK`
- [x] Only `crates/unimatrix-store/src/read.rs` modified — within scope
- [x] Error handling uses `StoreError::Database(e.into())` pattern, no `.unwrap()` in implementation code
- [x] Both methods use `read_pool()` (C-02 compliant, R-06 satisfied)
- [x] Pair normalization in `query_existing_supports_pairs` matches Phase 4 contract
- [x] All 12 test cases match test-plan expectations (AC-15, R-04)
- [x] Note: workspace build error in `unimatrix-server` for missing `nli_detection_tick` module is expected — that file is another agent's deliverable; `unimatrix-store` itself compiles cleanly

## Issues / Blockers

None. The workspace-level `E0583` error for `nli_detection_tick` is pre-existing (another agent's component) and is not caused by this change.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entry #3028 confirming `read_pool()` is `pub(crate)` within the store crate (accessible from `read.rs`); entry #3659 confirming ADR-004 choice for `query_existing_supports_pairs`.
- Stored: entry #3661 "SQLite NOT IN over empty subquery returns all rows — no NULL guard needed" via `/uni-store-pattern` — confirms SQLite NOT IN handles empty subquery results correctly, no COALESCE guard required.
