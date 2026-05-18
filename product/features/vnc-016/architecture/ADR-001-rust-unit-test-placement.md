## ADR-001: Rust Unit Test for query_stale_prerequisite_edges_for_cycle Lives in read.rs mod tests

### Context

AC-09 requires a Rust unit test that calls `query_stale_prerequisite_edges_for_cycle`
directly against an in-process store. Three locations were considered:

(A) `crates/unimatrix-store/src/read.rs` — the existing `mod tests` block at line 1887.
    Pattern: `open_test_store(&dir)` + raw `sqlx::query(...)...execute(&store.write_pool)`.
    Established by `test_query_graph_edges_returns_rows` (line 2056) and the
    `insert_test_entry` helper (line 2144). File is currently 3321 lines; adding two
    async tests (~60 lines) keeps it below a threshold requiring a split.

(B) A new file `crates/unimatrix-store/src/read_stale_edges_tests.rs` (separate module).
    No existing precedent — all query tests live in the same file as the function.
    Would require a `mod` declaration in `read.rs`, adding file-system friction with
    no benefit.

(C) `crates/unimatrix-store/tests/` integration test (separate binary).
    These tests use the `test-support` feature and `write_pool_test()`. The function
    under test is in `read.rs` (not an integration boundary), and the bug is a
    column-name error in a single SQL query — best caught as close to the function as
    possible, not through a cross-crate test binary.

The function has a single entry point and no trait abstraction. Its test requires
seeding three tables (entries, feature_entries, graph_edges) via raw SQL against
`store.write_pool`, which is accessible as `pub(crate)` within `read.rs`.

### Decision

Place the Rust unit tests for `query_stale_prerequisite_edges_for_cycle` in the
existing `mod tests` block at the bottom of `read.rs`.

Two tests are added:
- `test_query_stale_prerequisite_edges_for_cycle_returns_pair`: seeds entries A
  (Deprecated, status=1) and B (Active, status=0), inserts `feature_entries` row for A
  under the test cycle, inserts `graph_edges` Prerequisite edge A→B, calls the
  function, asserts `vec![(A.id, B.id)]`.
- `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry`: same
  entries and edge, but no `feature_entries` row — asserts empty return. Verifies
  that the cycle scoping (the WHERE clause join) is correctly enforced.

Both tests use `#[tokio::test]`, `open_test_store(&dir)`, and raw `sqlx::query`
against `store.write_pool` — exactly the pattern used by `test_query_graph_edges_returns_rows`.

The `insert_test_entry` helper already present in `mod tests` (line 2144) inserts into
`entries` with a configurable `status` integer. It can be reused directly.

### Consequences

Easier: The test is co-located with the function it guards — a future rename of the
`feature_entries.feature_id` column will fail the unit test immediately, before the
MCP layer is involved. No new files, no new feature flags, no new crate dependencies.

Harder: `read.rs` grows slightly (two tests, ~60 lines). If the file approaches the
500-line limit for single-responsibility files, these tests would be among the first
candidates to extract into a sub-module. At 3321 + 60 = 3381 lines the file remains
well over the threshold; this is a pre-existing condition, not introduced by vnc-016.
