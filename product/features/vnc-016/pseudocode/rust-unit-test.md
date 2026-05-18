# Component 2: Rust Unit Test — `read.rs mod tests`

## Purpose

Provide a permanent store-layer regression guard for `query_stale_prerequisite_edges_for_cycle`
that would have caught the column-name bug (`fe.feature_cycle` vs `fe.feature_id`) at `cargo test`
time without requiring a full MCP server. Two tests: one positive path, one negative companion.

## File

`crates/unimatrix-store/src/read.rs`, append to the existing `#[cfg(test)] mod tests` block
starting at line 1887. Insert after the last existing test in the module.

## Pattern Reference

These tests follow the pattern established by `test_query_graph_edges_returns_rows` (line 2056):
- `tempfile::TempDir::new()` for isolation
- `open_test_store(&dir).await` from `crate::test_helpers`
- `sqlx::query(...).execute(&store.write_pool).await.expect("...")` for seeding
- Direct function call on `store`
- `assert_eq!` / `assert!(...)` on `Result` contents

Imports already present in the `mod tests` block:
- `use super::*;`
- `use crate::test_helpers::open_test_store;`

The `write_pool` field is accessed via `store.write_pool` (same as line 2070 in the reference test).

## Test 1: Positive Path

```rust
#[tokio::test]
async fn test_query_stale_prerequisite_edges_for_cycle_returns_pair() {
    // -- Setup ---------------------------------------------------------------
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;
    let cycle = "vnc016-test-cycle";

    // Seed entry A: Deprecated (status = 1)
    // Uses raw sqlx against write_pool to bypass the domain layer.
    // 'created_at' is required NOT NULL; use a fixed epoch value.
    // 'category', 'topic', 'content', 'title' are required NOT NULL.
    let id_a: i64 = sqlx::query_scalar(
        "INSERT INTO entries (title, content, topic, category, tags, source, status, \
                              created_by, created_at, trust_source) \
         VALUES ('entry-a', 'content-a', 'test', 'pattern', '[]', '', 1, 'test', 0, 'agent') \
         RETURNING id",
    )
    .fetch_one(&store.write_pool)
    .await
    .expect("insert entry A");

    // Seed entry B: Active (status = 0) — the target of the Prerequisite edge
    let id_b: i64 = sqlx::query_scalar(
        "INSERT INTO entries (title, content, topic, category, tags, source, status, \
                              created_by, created_at, trust_source) \
         VALUES ('entry-b', 'content-b', 'test', 'pattern', '[]', '', 0, 'test', 0, 'agent') \
         RETURNING id",
    )
    .fetch_one(&store.write_pool)
    .await
    .expect("insert entry B");

    // Seed feature_entries: associate entry A with the test cycle.
    // Column name is 'feature_id' — NOT 'feature_cycle'. This is the fix target.
    sqlx::query(
        "INSERT INTO feature_entries (feature_id, entry_id, phase) VALUES (?1, ?2, NULL)",
    )
    .bind(cycle)
    .bind(id_a)
    .execute(&store.write_pool)
    .await
    .expect("insert feature_entries row");

    // Seed graph_edges: Prerequisite edge from A (source) to B (target).
    // relation_type must match the SQL literal 'Prerequisite' exactly (case-sensitive).
    // created_at, created_by, source, bootstrap_only are NOT NULL DEFAULT; provide them.
    sqlx::query(
        "INSERT INTO graph_edges \
             (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) \
         VALUES (?1, ?2, 'Prerequisite', 1.0, 0, 'test', 'test', 0)",
    )
    .bind(id_a)
    .bind(id_b)
    .execute(&store.write_pool)
    .await
    .expect("insert graph_edges row");

    // -- Exercise ------------------------------------------------------------
    // Call directly on store — errors surface through Result, not swallowed.
    let result = store
        .query_stale_prerequisite_edges_for_cycle(cycle)
        .await;

    // -- Assert --------------------------------------------------------------
    // (a) Must not be an error. If this fails, the SQL column name is still wrong.
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());

    let pairs = result.unwrap();

    // (b) Must contain exactly one pair.
    assert_eq!(
        pairs.len(),
        1,
        "expected exactly 1 stale edge pair, got: {:?}",
        pairs
    );

    // (c) Must contain the exact (A, B) pair.
    // id_a and id_b are i64 from RETURNING; cast to u64 to match return type.
    assert_eq!(
        pairs[0],
        (id_a as u64, id_b as u64),
        "expected pair ({id_a}, {id_b}), got: {:?}",
        pairs[0]
    );
}
```

### Why All Three Sub-Assertions Are Required

- `result.is_ok()` — surfaces the column-name error as a test failure rather than swallowing it.
  Before the SQL fix, this assertion fails with "no such column: fe.feature_cycle". Using
  `unwrap_or_default()` or `unwrap_or_else(|_| vec![])` here replicates the production bug.
- `pairs.len() == 1` — confirms the JOIN finds exactly the seeded row.
- `pairs[0] == (id_a, id_b)` — confirms the correct `(source_id, target_id)` ordering.

Without all three, the assertion is structurally weaker and risks passing against a broken
implementation (e.g., a query that returns all edges ignoring `feature_id`, or returns the
wrong tuple ordering).

## Test 2: Negative Path (Companion)

```rust
#[tokio::test]
async fn test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry() {
    // -- Setup ---------------------------------------------------------------
    let dir = tempfile::TempDir::new().expect("tempdir");
    let store = open_test_store(&dir).await;
    let cycle = "vnc016-absent-cycle";

    // Seed entry A: Deprecated (status = 1) — same as positive test
    let id_a: i64 = sqlx::query_scalar(
        "INSERT INTO entries (title, content, topic, category, tags, source, status, \
                              created_by, created_at, trust_source) \
         VALUES ('entry-a-neg', 'content', 'test', 'pattern', '[]', '', 1, 'test', 0, 'agent') \
         RETURNING id",
    )
    .fetch_one(&store.write_pool)
    .await
    .expect("insert entry A neg");

    // Seed entry B: Active (status = 0)
    let id_b: i64 = sqlx::query_scalar(
        "INSERT INTO entries (title, content, topic, category, tags, source, status, \
                              created_by, created_at, trust_source) \
         VALUES ('entry-b-neg', 'content', 'test', 'pattern', '[]', '', 0, 'test', 0, 'agent') \
         RETURNING id",
    )
    .fetch_one(&store.write_pool)
    .await
    .expect("insert entry B neg");

    // Seed graph_edges: same Prerequisite edge A -> B — intentionally present.
    // NO feature_entries row is inserted for any cycle. This isolates the JOIN
    // on feature_entries as the scoping mechanism under test.
    sqlx::query(
        "INSERT INTO graph_edges \
             (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only) \
         VALUES (?1, ?2, 'Prerequisite', 1.0, 0, 'test', 'test', 0)",
    )
    .bind(id_a)
    .bind(id_b)
    .execute(&store.write_pool)
    .await
    .expect("insert graph_edges row neg");

    // -- Exercise ------------------------------------------------------------
    let result = store
        .query_stale_prerequisite_edges_for_cycle(cycle)
        .await;

    // -- Assert --------------------------------------------------------------
    // Must return Ok (not an error) — verifies the query runs without error
    // even when no feature_entries rows match.
    assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());

    // Must return an empty vec — the JOIN on feature_entries filters out all
    // edges whose source is not registered to this cycle. This is the scoping
    // guarantee that prevents cross-cycle contamination.
    assert!(
        result.unwrap().is_empty(),
        "expected empty result when no feature_entries row exists for cycle"
    );
}
```

### Why This Test Is Not Optional

A broken implementation that returns all stale Prerequisite edges globally (ignoring the
`feature_id` JOIN clause) would pass the positive test while producing cross-cycle false
positives in production. Only the negative companion validates that the `WHERE fe.feature_id = ?1`
scoping clause does its job.

## Error Handling

- Both tests use `expect("...")` for seeding operations — seeding failures are test setup bugs,
  not the code under test. `expect` messages clearly identify the failing operation.
- The function under test is called with `.await` and the `Result` is examined directly.
  Never `unwrap()` without first asserting `is_ok()`, to produce a legible failure message
  when the SQL error is present.
- `assert!(result.is_ok(), "expected Ok, got: {:?}", result.err())` uses the `{:?}` format
  to display the full `StoreError` including the SQLite column error text.

## Constraints

- C-08: No new test infrastructure. Uses `open_test_store` + raw `sqlx::query` against
  `store.write_pool` — identical to the `test_query_graph_edges_returns_rows` pattern.
- NFR-07 alignment: Tests must NOT use `unwrap_or_else(|_| vec![])` on the function result.
  Errors must surface as test failures.
- Both tests use `TempDir::new()` (distinct temp directory per test) — no shared state.
- `id_a` / `id_b` are `i64` from the `RETURNING id` scalar query; cast to `u64` when
  comparing to the function's return type `Vec<(u64, u64)>`.
