# Agent Report: crt-040-agent-5-write-graph-edge

**Task:** Add `write_graph_edge` as a new sibling function in `nli_detection.rs` for crt-040.

---

## Files Modified

- `crates/unimatrix-server/src/services/nli_detection.rs`

---

## Changes Made

### Production code

1. Updated module-level doc comment to list `write_graph_edge` and explain the sibling pattern (pattern #4025 / ADR-001).
2. Added `pub(crate) async fn write_graph_edge(store, source_id, target_id, relation_type, weight, created_at, source, metadata) -> bool` immediately after `write_nli_edge`.
   - SQL: `INSERT OR IGNORE` with `?6` bound twice (for `created_by` and `source`).
   - `Ok` arm: returns `query_result.rows_affected() > 0` (distinguishes insert from UNIQUE conflict — unlike `write_nli_edge` which returns `true` on any `Ok`).
   - `Err` arm: emits `tracing::warn!` with structured fields (`source_id`, `target_id`, `relation_type`, `source`, `error`), returns `false`.
3. `write_nli_edge` was NOT modified — SQL literal `'nli', 'nli'` unchanged, signature unchanged.

### Tests

Seven new `#[tokio::test]` cases added to `#[cfg(test)] mod tests`:

| TC | Function | Coverage |
|----|----------|----------|
| TC-01 | `test_write_graph_edge_writes_cosine_supports_source` | source/created_by/relation_type/weight columns verified |
| TC-02 | `test_write_nli_edge_still_writes_nli_source` | R-02 regression guard — write_nli_edge source='nli' unchanged |
| TC-03 | `test_write_graph_edge_and_write_nli_edge_distinct_sources` | Both functions write distinct source values |
| TC-04 | `test_write_graph_edge_duplicate_returns_false_no_warn` | UNIQUE conflict returns false; row count = 1 |
| TC-05 | `test_write_graph_edge_sql_error_returns_false` | SQLITE_READONLY via open_readonly triggers Err arm; returns false |
| TC-06 | `test_write_graph_edge_metadata_format` | metadata column exact string match (direct sqlx query) |
| TC-07 | `test_write_graph_edge_informs_relation_type` | Generic relation_type parameter |

---

## Test Results

```
test services::nli_detection::tests::test_format_nli_metadata_contains_required_keys ... ok
test services::nli_detection::tests::test_format_nli_metadata_is_valid_json ... ok
test services::nli_detection::tests::test_write_graph_edge_metadata_format ... ok
test services::nli_detection::tests::test_write_graph_edge_sql_error_returns_false ... ok
test services::nli_detection::tests::test_write_graph_edge_and_write_nli_edge_distinct_sources ... ok
test services::nli_detection::tests::test_write_graph_edge_informs_relation_type ... ok
test services::nli_detection::tests::test_write_nli_edge_still_writes_nli_source ... ok
test services::nli_detection::tests::test_write_graph_edge_writes_cosine_supports_source ... ok
test services::nli_detection::tests::test_write_graph_edge_duplicate_returns_false_no_warn ... ok

9 passed; 0 failed
```

Full workspace build: zero errors. No clippy issues in modified file.

File line count: 417 lines (under 500-line limit).

---

## Checklist

- [x] `write_nli_edge` is NOT modified — SQL literal `'nli', 'nli'` unchanged
- [x] `write_graph_edge` placed immediately after `write_nli_edge`
- [x] SQL uses `?6` bound twice (created_by and source)
- [x] `Err` branch emits `warn!` with structured fields
- [x] `Ok` branch returns `rows_affected() > 0`
- [x] Module doc comment updated
- [x] TC-01 through TC-07 pass
- [x] No `.unwrap()` in production code
- [x] No source file exceeds 500 lines

---

## Issues / Blockers

None. The `nli_detection_tick.rs` import extension (`write_graph_edge` added to the use statement) is the responsibility of the agent implementing Path C (agent-6), per the pseudocode checklist.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-001 (#4027, decision), pattern #4025 (write_nli_edge hardcodes source), pattern #3884 (INSERT OR IGNORE idempotent writes). All applied.
- Stored: entry #4037 "open_readonly SQL error injection for write helper unit tests: open_test_store first (creates test.db), then open_readonly on test.db" via /uni-store-pattern — the test.db vs store.db filename trap is invisible in source and would cause TC-05 to panic before exercising the Err branch.
