# vnc-018 Test Plan: unimatrix-store db.rs — SQL Query Functions

## Component Scope

New functions added to `crates/unimatrix-store/src/db.rs`:
- `query_supersession_chain(pool, id, direction, depth_cap)` — SQL recursive CTE for
  chain and current modes
- `query_direct_neighbors(pool, id, edge_types, direction)` — SQL composite-index query
  for neighbors mode at depth=1
- 4 new index DDL in `create_tables_if_needed`
- Schema version literal bump to 27

Also tested here (schema cascade touch points):
- `db.rs::test_schema_version_initialized_to_current_on_fresh_db` → expects 27
- Index existence on fresh database (not just migrated)

---

## Unit Test Expectations

### `query_supersession_chain` — core behavior

All tests use a real SQLite in-memory database (not mocks). These are integration tests
within the `unimatrix-store` crate, exercising SQL against a real engine.

**Test: `test_query_supersession_chain_empty_db_returns_empty`** (R-01, R-05)

```rust
// Arrange: fresh migrated DB with no entries
// Act: query_supersession_chain(pool, 999999, ChainDirection::Both, 50)
// Assert: Ok(ChainQueryResult { entries: [], forward_capped: false, backward_capped: false })
// COMMENT: "Tests cold-start SQL path — no tick has run, in-memory graph is empty.
//          Result is correct because SQL CTE reads live DB, not in-memory graph."
```

**Test: `test_query_supersession_chain_single_entry`**

```rust
// Arrange: insert one active entry with id=42, no supersedes/superseded_by
// Act: query_supersession_chain(pool, 42, Both, 50)
// Assert: entries = [entry_42], no truncation
```

**Test: `test_query_supersession_chain_five_entry_chain_both`** (AC-01)

```rust
// Arrange: insert entries A, B, C, D, E with A→B→C→D→E supersession chain
// (A.superseded_by = B, B.superseded_by = C, etc.)
// Act: query_supersession_chain(pool, C.id, Both, 50)
// Assert: entries = [A, B, C, D, E] ordered oldest to newest
// Assert: forward_capped = false, backward_capped = false
```

**Test: `test_query_supersession_chain_direction_forward_only`** (AC-02)

```rust
// Same chain; call with ChainDirection::Forward
// Assert: entries = [C, D, E] (descendants only, no ancestors)
```

**Test: `test_query_supersession_chain_direction_backward_only`** (AC-02)

```rust
// Same chain; call with ChainDirection::Backward
// Assert: entries = [A, B, C] (ancestors only, no descendants)
```

**Test: `test_query_supersession_chain_50hop_cap_fires_forward`** (AC-03, R-05)

```rust
// Arrange: insert 55-entry forward chain from seed, 3-entry backward chain
// Act: query_supersession_chain(pool, seed.id, Both, 50)
// Assert: forward_capped == true
// Assert: backward_capped == false
// Assert: len(entries from forward direction) == 50
// Assert: len(entries from backward direction) == 3 (returned in full)
```

**Test: `test_query_supersession_chain_for_current_mode_active_status_filter`** (R-20, AC-06b)

```rust
// Arrange: insert entry D with status=Deprecated, superseded_by=NULL
// (orphaned deprecated terminal)
// Act: query_supersession_chain(pool, D.id, Forward, 50) — simulating current mode query
// Assert: entries is empty (status='Active' filter produces zero rows)
// COMMENT: "This test validates the AND e.status='Active' filter in the current-mode CTE.
//          Without this filter, D would appear as the terminal — a silent bug (R-20 Critical)."
// Note: current mode is a specialized use of query_supersession_chain — the exact
// SQL for current mode MUST include AND e.status='Active' in the WHERE clause.
```

**Test: `test_query_supersession_chain_nonexistent_id`** (AC-04)

```rust
// Act: query_supersession_chain(pool, 999999, Both, 50)
// Assert: Ok(ChainQueryResult { entries: [], forward_capped: false, backward_capped: false })
// (no error — empty result)
```

---

### `query_direct_neighbors` — core behavior

**Test: `test_query_direct_neighbors_outgoing_specific_type`** (AC-08)

```rust
// Arrange: insert entries X, Y, Z; insert GRAPH_EDGES rows: X→Y (Prerequisite), X→Z (Prerequisite)
// Act: query_direct_neighbors(pool, X.id, &["Prerequisite"], NeighborDirection::Outgoing)
// Assert: Ok, result = [RawEdgeRow{source=X,target=Y,rel="Prerequisite"}, {source=X,target=Z,...}]
```

**Test: `test_query_direct_neighbors_incoming_specific_type`** (AC-09)

```rust
// Arrange: insert GRAPH_EDGES Y→X (Supports), Z→X (Supports)
// Act: query_direct_neighbors(pool, X.id, &["Supports"], NeighborDirection::Incoming)
// Assert: result contains Y→X and Z→X rows
```

**Test: `test_query_direct_neighbors_both_directions`**

```rust
// Arrange: X→Y (Supports, outgoing) AND Z→X (Informs, incoming)
// Act: query_direct_neighbors(pool, X.id, &["Supports", "Informs"], NeighborDirection::Both)
// Assert: result includes both edges; direction field correct per row
```

**Test: `test_query_direct_neighbors_empty_type_list_excludes_supersedes`** (AC-10, R-06)

```rust
// Arrange: X→Y (Supports), X→Z (Supersedes) in GRAPH_EDGES
// Act: query_direct_neighbors(pool, X.id, &[], NeighborDirection::Both)
// Assert: Y in result; Z NOT in result (Supersedes excluded from "all types" query)
// Assert: no error; result is not empty
```

**Test: `test_query_direct_neighbors_nonexistent_anchor_returns_empty`** (R-12, OQ-01)

```rust
// Act: query_direct_neighbors(pool, 999999, &[], NeighborDirection::Both)
// Assert: Ok(empty Vec) — no error
// COMMENT: "OQ-01 resolved: non-existent anchor returns empty, consistent with chain mode (AC-04)"
```

**Test: `test_query_direct_neighbors_zero_edges_from_anchor`**

```rust
// Arrange: entry X exists but has no GRAPH_EDGES rows
// Assert: Ok(empty Vec)
```

---

### Schema cascade: `create_tables_if_needed`

**Test: `test_create_tables_creates_four_indexes`** (AC-19, R-05)

```rust
// Arrange: fresh in-memory SQLite
// Act: call create_tables_if_needed (or run schema initialization)
// Assert: sqlite_master contains all four index names:
//   "idx_entries_supersedes"
//   "idx_entries_superseded_by"
//   "idx_graph_edges_source_type"
//   "idx_graph_edges_target_type"
let count: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM sqlite_master WHERE type='index' AND name IN (?,?,?,?)"
)
.bind("idx_entries_supersedes")
.bind("idx_entries_superseded_by")
.bind("idx_graph_edges_source_type")
.bind("idx_graph_edges_target_type")
.fetch_one(pool).await?;
assert_eq!(count, 4, "All four v27 indexes must be created on fresh DB");
```

**Test: `test_schema_version_initialized_to_27_on_fresh_db`** (R-05, cascade touch point #7)

```rust
// Arrange: fresh DB via create_tables_if_needed
// Assert: schema_version counter == 27
// This test was previously test_schema_version_initialized_to_current_on_fresh_db
// and expected 26 — it must be updated to expect 27.
```

---

## Integration Test Expectations (Rust cross-crate)

The SQL query functions are exercised indirectly by the infra-001 Python suite through
the full MCP dispatch chain. Direct Rust integration tests cover the SQL functions in
isolation (store layer only):

1. `query_supersession_chain` with a fresh database (no ticks) — proves cold-start
   correctness and validates R-01 scenario 3 (unit test on store function against
   zero-tick DB)
2. `query_direct_neighbors` with a real GRAPH_EDGES table populated via `INSERT`
   (not via `context_edge` tool) — proves the SQL is correct independent of the
   tool layer

---

## Index Usage Verification

The four new indexes must be used by the SQL queries. Verify via SQLite query planner
(`EXPLAIN QUERY PLAN`) in at least one test:

```rust
// For chain query:
// EXPLAIN QUERY PLAN SELECT ... WHERE supersedes = ? → expect "USING INDEX idx_entries_supersedes"
// For neighbor query:
// EXPLAIN QUERY PLAN SELECT ... WHERE source_id = ? AND relation_type = ?
//   → expect "USING INDEX idx_graph_edges_source_type"
```

This confirms the indexes are not just present (AC-19) but actively used by the queries.

---

## Edge Cases

| Edge Case | Test | Assertion |
|-----------|------|-----------|
| Chain where seed has no ancestors or descendants (isolated entry) | `test_query_supersession_chain_single_entry` | entries=[seed], no truncation |
| Neighbors with all 15 valid types explicitly listed (no Supersedes) | `test_query_direct_neighbors_all_15_valid_types` | No error; equivalent to empty type list |
| Neighbor query with depth=1 returning 500+ edges (scale boundary) | `test_query_direct_neighbors_large_result` | Returns all rows within reasonable time |

---

## Risks Specifically Addressed in This Component

- R-01: Cold-start test (empty DB, no ticks) proves SQL path is used, not in-memory graph
- R-05: Schema cascade — 4 index assertions + schema_version literal assertion
- R-06: `edge_types=[]` path excludes Supersedes silently (tested at SQL level)
- R-20: CTE `AND e.status='Active'` filter validated via orphaned-deprecated test
- R-12: OQ-01 resolved — non-existent anchor returns empty per spec
