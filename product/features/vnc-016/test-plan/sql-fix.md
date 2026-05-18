# Component Test Plan: SQL Fix (`read.rs`)

## Component

**File**: `crates/unimatrix-store/src/read.rs`, line 1618
**Function**: `query_stale_prerequisite_edges_for_cycle`
**Change**: `fe.feature_cycle = ?1` → `fe.feature_id = ?1` (single token)

---

## AC Coverage

| AC-ID | Description |
|-------|-------------|
| AC-04 | `fe.feature_id` in WHERE clause; `fe.feature_cycle` absent from entire file at that location |

## Risk Coverage

| Risk ID | How This Component's Tests Address It |
|---------|--------------------------------------|
| R-03 | Rust unit test assertions catch any regression where the fix is absent or wrong |
| R-05 | Store-layer unit test is the sole regression guard against future column renames |

---

## Unit Test Expectations

The SQL fix has no unit tests of its own — it is verified by the Rust unit tests in Component 2
(rust-unit-test.md) which call the function directly against a live SQLite fixture. Those tests
are the primary regression guard.

The SQL fix is a prerequisite for those tests to pass.

---

## Verification Method: Code Inspection (AC-04)

AC-04 is verified by grep, not a test:

```bash
# Must have results (fix applied):
grep -n 'fe\.feature_id' crates/unimatrix-store/src/read.rs

# Must have NO results (bug removed):
grep -n 'fe\.feature_cycle' crates/unimatrix-store/src/read.rs
```

The fix is on line 1618. No other line in `read.rs` should reference `fe.feature_cycle`.

---

## Compile-Time Verification

The fix does not produce a compile error in either the before or after state (SQLite column
errors are runtime, not compile-time). Verification requires running the Rust unit test
(`cargo test -p unimatrix-store test_query_stale_prerequisite_edges_for_cycle_returns_pair`).

---

## Constraints

**C-10**: The `feature_entries.feature_id` column name is assumed stable. The fix depends on
it. If a future schema migration renames it, the unit test will catch the regression immediately.

**No signature change**: The function signature `pub async fn query_stale_prerequisite_edges_for_cycle(&self, feature_cycle: &str) -> Result<Vec<(u64, u64)>>` is unchanged. The parameter name `feature_cycle` is the application-level name for the value passed in; it is not the column name.

---

## Expected Behavior After Fix

Given a database seeded with:
- `entries` row A: `status = 1` (Deprecated)
- `entries` row B: `status = 0` (Active)
- `feature_entries` row: `(feature_id = "test-cycle", entry_id = A.id, phase = NULL)`
- `graph_edges` row: `(source_id = A.id, target_id = B.id, relation_type = 'Prerequisite')`

Calling `query_stale_prerequisite_edges_for_cycle("test-cycle")` must return:
```
Ok(vec![(A.id, B.id)])
```

Before the fix, it returns:
```
Err(SqlxError("no such column: fe.feature_cycle"))
```
which is swallowed by `unwrap_or_else` in `tools.rs:2169` and becomes `vec![]`.

---

## Integration Test Expectations

Through the MCP interface, the SQL fix enables `context_cycle_review` to return a non-empty
`hotspots` array with a `dependency_on_deprecated` finding. This is verified by
`test_dependency_on_deprecated_e2e` in `test_tools.py`.

The SQL fix is the most load-bearing change in this feature. Without it, the integration test
positive path is a vacuous pass (empty hotspots, wrong assertion logic) or a wrong-assertion
failure.

---

## Edge Cases from Risk Strategy

**SQLite TEXT case sensitivity**: `feature_id` comparison uses `=` (case-sensitive in SQLite
TEXT). The cycle ID string must be identical in `context_store` call and `context_cycle_review`
call. No normalization applied by the server.

**`relation_type` casing**: The WHERE clause uses `ge.relation_type = 'Prerequisite'` (exact).
`context_edge("add", ...)` must pass exactly `"Prerequisite"` — any variant (e.g., lowercase)
causes a JOIN miss.

**`phase = NULL`**: The `feature_entries` write path includes `phase` but it is nullable. The
Rust unit test seeds with `phase = NULL`. If `phase` ever becomes NOT NULL, the seed SQL will
fail — not the fix itself.
