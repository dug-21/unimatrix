# Component Test Plan: Rust Unit Test (`read.rs mod tests`)

## Component

**File**: `crates/unimatrix-store/src/read.rs`, appended to `mod tests` at line 1887+
**New tests**:
- `test_query_stale_prerequisite_edges_for_cycle_returns_pair`
- `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry`

Both are `#[tokio::test]` async functions. Both use the existing `open_test_store` +
raw `sqlx::query` against `store.write_pool` pattern from the same module.

---

## AC Coverage

| AC-ID | Description |
|-------|-------------|
| AC-09 | Rust unit test calls `query_stale_prerequisite_edges_for_cycle` directly; asserts `result.is_ok()`, `result.unwrap().len() == 1`, `result.unwrap()[0] == (A.id, B.id)` |

## Risk Coverage

| Risk ID | How This Component's Tests Address It |
|---------|--------------------------------------|
| R-03 | Positive-path test must contain all three sub-assertions — any one absent makes the test structurally insufficient |
| R-04 | Negative-path companion is required; it is the only way to detect a broken JOIN scoping (always-returns-all-edges regression) |
| R-05 | These tests call the store function directly, bypassing `unwrap_or_else` in `tools.rs`; SQL errors surface as `Err`, not `vec![]` |

---

## Positive-Path Test: `test_query_stale_prerequisite_edges_for_cycle_returns_pair`

### Arrange

Using `open_test_store(&dir)` and raw `sqlx::query` against `store.write_pool`:

1. Insert entry A into `entries` with `status = 1` (Deprecated). Capture `A_id` via `last_insert_rowid()`.
2. Insert entry B into `entries` with `status = 0` (Active). Capture `B_id`.
3. Insert `(feature_id = "test-cycle-pos", entry_id = A_id, phase = NULL)` into `feature_entries`.
4. Insert `(source_id = A_id, target_id = B_id, relation_type = 'Prerequisite')` into `graph_edges`.

### Act

```rust
let result = store.query_stale_prerequisite_edges_for_cycle("test-cycle-pos").await;
```

### Assert

All three sub-assertions must be present:

```rust
assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
let pairs = result.unwrap();
assert_eq!(pairs.len(), 1, "expected exactly one pair, got: {}", pairs.len());
assert_eq!(pairs[0], (A_id, B_id), "expected ({}, {}), got: {:?}", A_id, B_id, pairs[0]);
```

**CRITICAL**: Do not use `unwrap_or_else(|_| vec![])` or `unwrap_or_default()` anywhere in
this test. These would mask SQL errors and replicate the production bug inside the test itself.
The test must surface any `Err` as a test failure.

### Pre-Fix Behavior

Before the SQL fix is applied, this test fails with:
```
SqlxError: no such column: fe.feature_cycle
```
propagated as `Err(...)`, causing `result.is_ok()` to fail and the test to fail loudly.
This is the correct behavior — the test is a regression guard.

---

## Negative-Path Test: `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry`

### Arrange

Using the same `open_test_store` + raw `sqlx::query` pattern:

1. Insert entry A into `entries` with `status = 1` (Deprecated). Capture `A_id`.
2. Insert entry B into `entries` with `status = 0` (Active). Capture `B_id`.
3. Insert `(source_id = A_id, target_id = B_id, relation_type = 'Prerequisite')` into `graph_edges`.
4. Do NOT insert any row into `feature_entries` for any cycle.

### Act

```rust
let result = store.query_stale_prerequisite_edges_for_cycle("test-cycle-neg").await;
```

### Assert

```rust
assert!(result.is_ok(), "expected Ok, got: {:?}", result.err());
assert!(result.unwrap().is_empty(), "expected empty vec when no feature_entries row exists");
```

### What This Guards Against

An implementation of `query_stale_prerequisite_edges_for_cycle` that JOINs on `feature_entries`
but omits the `WHERE fe.feature_id = ?1` clause (returns all stale Prerequisite edges regardless
of cycle) would pass the positive test but fail this negative companion. Without this test,
such a regression is undetectable.

---

## Test Infrastructure Constraints

**C-08**: No new Rust test infrastructure. Use the existing pattern from
`test_query_graph_edges_returns_rows` (line 2056) as the structural reference.

Specifically:
- `use tempdir::TempDir` (or the existing import already present in `mod tests`)
- `open_test_store(&dir)` from `test_helpers.rs:13`
- `sqlx::query("INSERT INTO entries ...").execute(&store.write_pool).await.unwrap()` for seeding
- No new helper functions; inline all seed SQL

**C-11** (not directly applicable here): This test does not construct `UsageContext`. But note
that the test seeds `feature_entries` via raw SQL — it bypasses the analytics write path
entirely. This is correct and intentional: the unit test validates the SQL query, not the
write path.

---

## Test Module Placement

Both tests append to the `mod tests` block in `read.rs` starting at line 1887.
Co-location with the function under test is the established pattern in this crate.

No separate test file, no `#[cfg(test)]` at module level (already present), no new imports
beyond what is already in scope in the existing `mod tests` block.

---

## Cargo Test Command

```bash
cargo test -p unimatrix-store test_query_stale_prerequisite_edges_for_cycle 2>&1 | tail -30
```

Both test functions will be matched by the substring filter. Both must pass.

---

## Expected Cargo Output (After Fix)

```
test read::tests::test_query_stale_prerequisite_edges_for_cycle_returns_pair ... ok
test read::tests::test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry ... ok
```

---

## Expected Cargo Output (Before Fix — Regression Verification)

```
test read::tests::test_query_stale_prerequisite_edges_for_cycle_returns_pair ... FAILED
failures:
    read::tests::test_query_stale_prerequisite_edges_for_cycle_returns_pair
FAILED tests/... - 1 failed, 1 passed
```

The positive test must fail against un-fixed code. If it passes, the test is vacuous (R-03).
