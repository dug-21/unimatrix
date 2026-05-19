# Test Plan: query_incoming_edges

**Component**: `unimatrix-store/src/read.rs`
**Function**: `pub async fn query_incoming_edges(&self, target_id: u64) -> Result<Vec<IncomingEdgeRow>>`

---

## Unit Test Expectations

All tests use an in-memory SQLite database (same pattern as other `read.rs` tests in `unimatrix-store`). Tests call `query_incoming_edges` directly on the `Store` instance.

---

### AC-05: Basic return contract — exact 3-tuple match

**Arrange**: Seed 4 rows in `graph_edges`:
- `(source_id=10, target_id=99, relation_type="Prerequisite", created_at=1000)`
- `(source_id=20, target_id=99, relation_type="Contradicts", created_at=2000)`
- `(source_id=30, target_id=99, relation_type="Prerequisite", created_at=3000)`
- `(source_id=40, target_id=77, relation_type="Prerequisite", created_at=4000)` ← different target

**Act**: `store.query_incoming_edges(99).await`

**Assert**:
- Returns `Ok(vec)` where `vec.len() == 3`
- Each returned `IncomingEdgeRow` has `source_id`, `relation_type`, and `created_at` matching exactly the 3 seeded rows for `target_id=99`
- The row for `target_id=77` is NOT present
- `relation_type` values are the exact strings seeded (no coercion or case normalization)

**Test name**: `test_query_incoming_edges_returns_matching_rows_only`

---

### R-02 (Critical): Supersedes exclusion is at SQL level

**Arrange**: Seed 2 rows in `graph_edges` for `target_id=99`:
- `(source_id=10, target_id=99, relation_type="Supersedes", created_at=1000)`
- `(source_id=20, target_id=99, relation_type="Supersedes", created_at=2000)`

**Act**: `store.query_incoming_edges(99).await`

**Assert**:
- Returns `Ok(vec)` where `vec.len() == 0` (empty — SQL filter removed both rows)
- This distinguishes SQL-level exclusion from loop-level: if the function returned 2 rows, the exclusion is at loop level (violation of ADR-002)

**Test name**: `test_query_incoming_edges_excludes_supersedes_at_sql_level`

---

### R-03: High-cardinality filter correctness

**Arrange**: Seed 1000 rows in `graph_edges` with `target_id=NOISE` (a different target), then seed 3 rows with `target_id=42` (Prerequisite, Contradicts, Prerequisite).

**Act**: `store.query_incoming_edges(42).await`

**Assert**:
- Returns exactly 3 rows
- Each row has `target_id` that is implicitly 42 (confirmed by source_id values matching the seeded ones)
- The 1000 noise rows are not included

**Purpose**: Validates that the `WHERE target_id = ?1` bind parameter is correctly wired and the query planner uses the `idx_graph_edges_target_id` index path (or at minimum filters correctly at any cardinality).

**Test name**: `test_query_incoming_edges_high_cardinality_filters_correctly`

---

### R-07 / AC-11 (Supersedes-only path): Zero-edge behavior

**Arrange**: Seed 1 Supersedes row for `target_id=99`. Seed no other rows for `target_id=99`.

**Act**: `store.query_incoming_edges(99).await`

**Assert**:
- Returns `Ok(vec)` where `vec.len() == 0`
- No error; empty vec is a valid result

**Test name**: `test_query_incoming_edges_supersedes_only_returns_empty`

---

### Empty target: Zero rows

**Arrange**: No rows in `graph_edges` for `target_id=99`.

**Act**: `store.query_incoming_edges(99).await`

**Assert**:
- Returns `Ok(vec)` where `vec.len() == 0`

**Test name**: `test_query_incoming_edges_no_rows_returns_empty`

---

### Mixed Supersedes and non-Supersedes

**Arrange**: Seed for `target_id=99`:
- `(source_id=10, relation_type="Supersedes")`
- `(source_id=20, relation_type="Prerequisite")`
- `(source_id=30, relation_type="Contradicts")`

**Act**: `store.query_incoming_edges(99).await`

**Assert**:
- Returns exactly 2 rows
- Neither row has `source_id=10` (the Supersedes row is excluded)
- Source IDs 20 and 30 are present with correct `relation_type` values

**Test name**: `test_query_incoming_edges_mixed_excludes_supersedes_only`

---

## Struct Contract Assertions

The `IncomingEdgeRow` struct is verified by AC-05's assertion on `source_id`, `relation_type`, and `created_at` field access. The struct must be `pub` (callable from `unimatrix-server`). No separate test needed for pub visibility — a compilation failure would surface immediately.

---

## Integration Test Expectations

`query_incoming_edges` is not exercised directly through the MCP interface. Its behavior is observable indirectly through the redirect loop: when the integration tests for AC-06, AC-07, AC-10 seed edge rows pointing at the original entry, the fact that those edges are (or are not) redirected confirms that `query_incoming_edges` correctly returned (or excluded) them.

The Supersedes exclusion (R-02) is validated end-to-end by `test_correct_leaves_supersedes_edges_unchanged` in `test_lifecycle.py`.

---

## Edge Cases

- **`target_id` that has never been an entry** — valid query; returns empty vec (graph_edges has no FK enforcement that targets must be entries — confirmed by schema).
- **Concurrent writes to `graph_edges` during query** — not testable deterministically; accepted per R-14 posture.
- **`relation_type` values with unusual casing** — `query_incoming_edges` stores and returns the string as-is from the database; no normalization occurs. The test for R-02 uses exact string `"Supersedes"` matching the stored value.

---

## Pool Accessor Code Review Gate

The implementation MUST use `read_pool()` (not `write_pool_server()`). A comment must appear at the call site citing `db.rs:294` (write_pool_server and read_pool currently alias the same underlying pool). This is not testable via automated tests but is a required code review assertion per C-07.
