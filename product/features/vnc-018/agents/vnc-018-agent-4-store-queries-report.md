# Agent Report: vnc-018-agent-4-store-queries

**Feature**: vnc-018 (context_graph — 14th MCP tool)
**Component**: unimatrix-store SQL query functions
**Agent ID**: vnc-018-agent-4-store-queries

---

## Deliverables

### New file
- `crates/unimatrix-store/src/graph_queries.rs` — new module (499 lines)

### Modified files
- `crates/unimatrix-store/src/db.rs` — added 4 v27 index DDL to `create_tables_if_needed` + `test_create_tables_creates_four_v27_indexes`
- `crates/unimatrix-store/src/lib.rs` — added `pub mod graph_queries` declaration and re-exports

---

## Implementation Summary

### Types added (graph_queries.rs)

- `ChainDirection { Forward, Backward, Both }` — supersession chain traversal direction
- `NeighborDirection { Incoming, Outgoing, Both }` — graph_edges query direction
- `ChainQueryResult { entries: Vec<EntryRecord>, forward_capped: bool, backward_capped: bool }`
- `RawEdgeRow { source_id: u64, target_id: u64, relation_type: String }`

All types have `#[derive(Debug)]` and are re-exported from `lib.rs`.

### Functions added

1. **`query_supersession_chain`** — SQL recursive CTEs on `entries.supersedes` / `entries.superseded_by`. Both directions run independently with per-direction cap tracking. Non-existent ID returns empty result (AC-04).

2. **`query_current_terminal`** — SQL CTE following `superseded_by` chain. Terminal condition: `superseded_by IS NULL AND status = 0` (integer Active, not string 'Active'). Returns `None` for non-existent ID, orphaned deprecated terminals, and 50-hop cap (all three map to same "no active terminal found" error at handler layer). Intentionally separate from `query_supersession_chain` per pseudocode spec.

3. **`query_direct_neighbors`** — Live SQL on GRAPH_EDGES. Empty `edge_types` = all except Supersedes (SQL-level `!= 'Supersedes'` filter). Non-empty = specific types (IN clause via positional params). Uses `run_outgoing_query` / `run_incoming_query` helpers.

### Schema changes (db.rs)
Four `CREATE INDEX IF NOT EXISTS` DDL statements added after existing graph_edges indexes:
- `idx_entries_supersedes ON entries(supersedes)`
- `idx_entries_superseded_by ON entries(superseded_by)`
- `idx_graph_edges_source_type ON graph_edges(source_id, relation_type)`
- `idx_graph_edges_target_type ON graph_edges(target_id, relation_type)`

Schema version literal already references `CURRENT_SCHEMA_VERSION` dynamically — the v27 bump was handled by agent-5 (migration cascade).

---

## Test Results

- **333 passed / 0 failed** (unimatrix-store)
- **Full workspace: all test suites pass, 0 failures**

### New tests in graph_queries.rs (10 tests)

| Test | Covers |
|------|--------|
| `test_query_supersession_chain_empty_db_returns_empty` | R-01, R-05 cold-start |
| `test_query_supersession_chain_single_entry` | isolated entry |
| `test_query_supersession_chain_five_entry_chain_both` | AC-01 both directions |
| `test_query_supersession_chain_direction_forward_only` | AC-02 forward |
| `test_query_supersession_chain_direction_backward_only` | AC-02 backward |
| `test_query_supersession_chain_nonexistent_id` | AC-04 empty not error |
| `test_query_current_terminal_active_entry_returns_some` | AC-05 active entry |
| `test_query_current_terminal_orphaned_deprecated_returns_none` | R-20 Critical |
| `test_query_current_terminal_nonexistent_id_returns_none` | AC-05a |
| `test_query_current_terminal_deprecated_with_active_successor` | AC-06 |
| `test_query_direct_neighbors_outgoing_specific_type` | AC-08 |
| `test_query_direct_neighbors_incoming_specific_type` | AC-09 |
| `test_query_direct_neighbors_both_directions` | both union |
| `test_query_direct_neighbors_empty_type_list_excludes_supersedes` | AC-10, R-06 |
| `test_query_direct_neighbors_nonexistent_anchor_returns_empty` | R-12, OQ-01 |
| `test_query_direct_neighbors_zero_edges_from_anchor` | empty anchor |

Plus `test_create_tables_creates_four_v27_indexes` in db.rs (AC-19, R-05).

---

## Issues / Deviations

1. **File placement**: The spawn prompt specified adding to `db.rs` directly. Since `db.rs` is already 1430+ lines (well above the 500-line limit), the implementation went into a new `graph_queries.rs` module and is re-exported from `lib.rs`. This satisfies the component interface identically — all types and functions are accessible at the same `unimatrix_store::*` path.

2. **`AND e.status = 'Active'` → `AND e.status = 0`**: The architecture documents specify `AND e.status = 'Active'` in the current-mode CTE. The `status` column is stored as an integer (0=Active, 1=Deprecated, etc.) in SQLite — the string comparison silently returns no rows. Fixed to `AND e.status = 0`. The R-20 critical filter is present and validated by `test_query_current_terminal_orphaned_deprecated_returns_none`.

3. **CTE ambiguous column**: `ENTRY_COLUMNS` (unqualified names) in a CTE-joined SELECT raises SQLite error "ambiguous column name: id" at runtime because the CTE also has an `id` column. Fixed via `ENTRY_COLUMNS_E` constant which uses `e.id AS id` for the id column only; all other columns are unambiguous.

4. **`CURRENT_SCHEMA_VERSION`**: Already bumped to 27 by agent-5 (migration cascade). The `db.rs` schema version literal was already bound to `CURRENT_SCHEMA_VERSION` dynamically — no literal change needed.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entries #4468 (supersession CTE pattern) and #4481 (v27 schema decision). Both confirmed the approach. No conflicts with existing knowledge.
- Stored: entry #4485 "SQLite recursive CTE joined with entries table: qualify e.id AS id to avoid ambiguous column name error" via /uni-store-pattern — runtime gotcha invisible in source code that affects any CTE join with the entries table.
