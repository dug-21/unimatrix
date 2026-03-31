# Agent Report: crt-035-agent-4-migration

**Component:** Migration v18→v19 (Component B)
**Feature:** crt-035 — Bidirectional CoAccess Edges + Bootstrap-Era Back-fill

---

## Files Modified / Created

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/migration.rs` | Bumped `CURRENT_SCHEMA_VERSION` 18→19; added `if current_version < 19` block with back-fill SQL and in-transaction version stamp |
| `crates/unimatrix-store/tests/migration_v18_to_v19.rs` | NEW — MIG-U-01 through MIG-U-07 (7 test cases) |
| `crates/unimatrix-store/tests/migration_v17_to_v18.rs` | Cascade: renamed `test_current_schema_version_is_18` → `_is_at_least_18` with `>=`; updated `read_schema_version == 18` to `>= 18` throughout |
| `crates/unimatrix-store/tests/migration_v12_to_v13.rs` | Cascade: updated CoAccess edge count assertion from 2 to 4 (2 pairs × 2 directions after v18→v19 back-fill) |
| `crates/unimatrix-store/tests/sqlite_parity.rs` | Cascade: updated `test_schema_version_is_14` assertion from 18 to 19 |
| `crates/unimatrix-server/src/server.rs` | Cascade: updated lines 2137 and 2162 from 18 to 19 (test_migration_v7_to_v8_backfill) |

---

## Tests

### migration_v18_to_v19 (new file)

```
test test_current_schema_version_is_19 ... ok
test test_fresh_db_creates_schema_v19 ... ok
test test_v18_to_v19_back_fills_bootstrap_era_edges ... ok
test test_v18_to_v19_back_fills_tick_era_edges ... ok
test test_v18_to_v19_does_not_touch_non_coaccess_edges ... ok
test test_v18_to_v19_migration_idempotent ... ok
test test_v18_to_v19_empty_graph_edges_is_noop ... ok

test result: ok. 7 passed; 0 failed
```

### Full unimatrix-store suite

All test suites: **0 failures** across 347 tests (190 unit + 157 integration across 12 test files).

### Full workspace

Zero failures across the entire workspace after cascade fixes.

---

## GATE-3B-03: EXPLAIN QUERY PLAN

Run against a tempfile-backed SQLite 3.40.1 database with the full `graph_edges` schema (including `UNIQUE(source_id, target_id, relation_type)` constraint and three single-column indexes):

```
QUERY PLAN
|--SEARCH g USING INDEX idx_graph_edges_relation_type (relation_type=?)
`--CORRELATED SCALAR SUBQUERY 1
   `--SEARCH rev USING COVERING INDEX sqlite_autoindex_graph_edges_1
               (source_id=? AND target_id=? AND relation_type=?)
```

**Result:** NOT EXISTS sub-select uses `SEARCH rev USING COVERING INDEX sqlite_autoindex_graph_edges_1` — the UNIQUE B-tree on `(source_id, target_id, relation_type)`. No `SCAN` (full table scan) detected. No composite covering index required. **GATE-3B-03 PASSED.**

---

## CURRENT_SCHEMA_VERSION

`pub const CURRENT_SCHEMA_VERSION: u64 = 19;`

---

## Issues / Blockers

None. All work completed within scope.

**Cascade discovery:** Data-only back-fill migrations also break row-count assertions in older migration test files (not just `schema_version` assertions). `migration_v12_to_v13.rs` asserted 2 CoAccess edges; after v18→v19 adds reverse edges for the forward edges created by the v13 bootstrap migration, the same test opens a v12 DB and migrates through v13 + v18→v19, landing at 4 edges. Fixed by updating the assertion and adding an explanatory comment. Stored as pattern extension #3894.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced #3889 (back-fill reverse GRAPH_EDGES uses INSERT OR IGNORE + swap), #3803 (schema version cascade checklist), #2937 (server.rs version assertion pattern). All applied directly.
- Stored: entry #3894 via `context_correct` on #3803 — extended schema version cascade checklist with crt-035 discovery: data-only back-fill migrations break row-count assertions in older migration test files (not just schema_version assertions). Added `cargo test --workspace` first recommendation.
