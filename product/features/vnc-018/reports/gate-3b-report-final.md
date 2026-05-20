# Gate 3b Report (Final): vnc-018

> Gate: 3b (Code Review — Final Check, iteration 3)
> Date: 2026-05-19
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. chain/current use SQL recursive CTEs; find_terminal_active not called | PASS | graph_queries.rs lines 83-119 (chain), 142-173 (current); prohibited function never appears in production code |
| 2. validate_no_unsupported_params inside handle_graph before mode dispatch | PASS | graph_read.rs line 138; runs before match on params.mode |
| 3. require_cap(Read) in tools.rs before handle_graph | PASS | tools.rs line 3381; Step 2 ordering comment confirms intent |
| 4. current mode CTE has AND e.status = Active filter | PASS | graph_queries.rs line 161: `AND e.status = 0` (Active = 0) |
| 5. BFS visited set HashSet<u64> by node_id only | PASS | graph_read_neighbors.rs line 261: `HashSet<u64>`; comments cite AC-11a, R-18 |
| 6. EdgeRecord.metadata no skip_serializing_if | PASS | graph_read.rs line 89: field present, no skip attribute; comment prohibits it |
| 7. chain non-existent ID → empty, not error | PASS | graph_read_supersession.rs lines 18-22; test_handle_chain_nonexistent_id_returns_empty |
| 8. current non-existent ID → error, not empty | PASS | graph_read_supersession.rs lines 88-98; test_handle_current_nonexistent_id_returns_error |
| 9. resolve_supersessions=Some(true) rejected on chain | PASS | graph_read.rs lines 234-238: explicit Err in chain arm |
| 10. tools.rs uses fully-qualified crate::mcp::graph_read:: path | PASS | tools.rs line 3390: `crate::mcp::graph_read::handle_graph(...)` |
| 11. grep schema_version == 26 returns zero matches | PASS | No matches in crates/ |
| 12. node_index_for on TypedRelationGraph in graph.rs | PASS | graph.rs line 270: pub fn node_index_for |
| 13. depth=1 → SQL; depth>1 → BFS | PASS | graph_read_neighbors.rs lines 159-173: explicit if depth == 1 branch |
| 14. Supersedes excluded silently from default; explicit → exact error | PASS | lines 123-132: all_non_supersedes_types() for default; explicit "Supersedes edges are not traversable" error |
| 15. No todo!/unimplemented!/placeholder in production code | PASS | Zero matches in vnc-018 production files |
| 16. depth validated 1..=10 | PASS | graph_read_neighbors.rs line 96: `if depth == 0 \|\| depth > 10` |
| 17. test_protocol.py asserts 14 tools, includes context_graph | PASS | test_list_tools_returns_fourteen (line 36); 14 tools listed including context_graph (line 57) |
| 18. All module files ≤500 lines | PASS | graph_read.rs: 306 lines (was 600 in iter 2); all other files within limit |
| Build clean | PASS | Finished dev profile, no errors; 20 pre-existing warnings unrelated to vnc-018 |

## Detailed Findings

### Check 1: chain and current use SQL recursive CTEs; find_terminal_active not called

**Status**: PASS

`query_supersession_chain` (graph_queries.rs line 83) and `query_current_terminal` (line 142) both open with `WITH RECURSIVE chain(id, depth) AS (...)`. The string `find_terminal_active` does not appear anywhere except in prohibition comments. The handler in graph_read_supersession.rs cites ADR-001 at the call sites (lines 44 and 83 of that file).

### Check 2: validate_no_unsupported_params inside handle_graph before mode dispatch

**Status**: PASS

`handle_graph` in graph_read.rs calls `validate_no_unsupported_params(&params)` at line 138, before the `match params.mode.as_str()` at line 155. The ordering is correct per ADR-003.

### Check 3: require_cap(Read) in tools.rs before handle_graph

**Status**: PASS

tools.rs lines 3378-3390: Step 2 is `require_cap(Read)` at line 3381. Step 4 is the `handle_graph` call at line 3390. Capability check precedes delegation.

### Check 4: current mode CTE has AND e.status = Active filter

**Status**: PASS

graph_queries.rs line 161: `AND e.status = 0` in the CTE's final SELECT. Status 0 = Active (as documented in the critical comment at lines 139-141). The test `test_handle_current_orphaned_deprecated_returns_error` explicitly validates that an orphaned deprecated entry (superseded_by IS NULL, status=Deprecated) returns an error rather than the deprecated entry.

### Check 5: BFS visited set HashSet<u64> by node_id only

**Status**: PASS

graph_read_neighbors.rs line 261: `let mut visited: HashSet<u64> = HashSet::new();`. Comments at lines 258-260 cite AC-11a and R-18 and explicitly warn against keying by `(node_id, depth)`.

### Check 6: EdgeRecord.metadata no skip_serializing_if

**Status**: PASS

The `EdgeRecord` struct in graph_read.rs (lines 80-90) has `pub metadata: Option<serde_json::Value>` with no `#[serde(...)]` attribute whatsoever on the field. The docstring and module-level comment both prohibit adding `skip_serializing_if` (ADR-004, R-15).

### Check 7: chain non-existent ID → empty, not error

**Status**: PASS

`handle_chain` returns `ChainResult` (infallible), not `Result`. The SQL CTE returns zero rows for non-existent IDs, producing an empty `entries` vec. Test `test_handle_chain_nonexistent_id_returns_empty` (graph_read_supersession.rs line 206) validates this with ID 999_999.

### Check 8: current non-existent ID → error, not empty

**Status**: PASS

`handle_current` returns `Result<CurrentResponse, String>`. `query_current_terminal` returns `Ok(None)` for non-existent IDs, which maps to `Err(format!("No active terminal found for entry {id}"))`. Test `test_handle_current_nonexistent_id_returns_error` (line 329) validates this asymmetry.

### Check 9: resolve_supersessions=Some(true) rejected on chain

**Status**: PASS

`validate_no_unsupported_params` chain arm (graph_read.rs lines 234-238): `if params.resolve_supersessions == Some(true)` → explicit Err with message "resolve_supersessions is not applicable to chain mode — chain IS the supersession audit". This runs inside handle_graph before mode dispatch.

### Check 10: tools.rs uses fully-qualified crate::mcp::graph_read:: path

**Status**: PASS

tools.rs line 3364: `Parameters<crate::mcp::graph_read::GraphParams>`. Line 3390: `crate::mcp::graph_read::handle_graph(...)`. Pattern #4436 compliance confirmed. A compile-time test at tools.rs line 5018 additionally asserts the fully-qualified path is resolvable.

### Check 11: grep schema_version == 26 returns zero matches

**Status**: PASS

`grep -r 'schema_version.*== 26' crates/` produces no output. ADR-007 compliance confirmed.

### Check 12: node_index_for on TypedRelationGraph in graph.rs

**Status**: PASS

graph.rs line 270: `pub fn node_index_for(&self, id: u64) -> Option<NodeIndex>`. Used at graph_read_neighbors.rs line 248 and line 334.

### Check 13: depth=1 → SQL; depth>1 → BFS

**Status**: PASS

graph_read_neighbors.rs lines 158-173: `if depth == 1 { neighbors_sql(...) } else { neighbors_bfs(...) }`. The `neighbors_sql` function queries `GRAPH_EDGES` directly via `query_direct_neighbors`. The `neighbors_bfs` function operates on the cloned in-memory `TypedRelationGraph`.

### Check 14: Supersedes excluded silently from default; explicit → exact error

**Status**: PASS

Default path (edge_types absent or empty): `all_non_supersedes_types()` returns 15 types with no Supersedes, no warning, no extra field. Explicit "Supersedes" in edge_types: returns `Err` with message "Supersedes edges are not traversable via neighbors mode — use chain or current modes for supersession navigation" (graph_read_neighbors.rs lines 129-133). Both behaviors match FR-07 and AC-15/AC-15a.

### Check 15: No todo!/unimplemented!/placeholder in production code

**Status**: PASS

Zero hits for `todo!`, `unimplemented!`, `TODO`, or `FIXME` in all vnc-018 production files. The single "placeholder" hit in graph_queries.rs (lines 364, 388) is a local variable named `placeholders` used for SQL parameter binding — not a stub marker.

### Check 16: depth validated 1..=10

**Status**: PASS

graph_read_neighbors.rs lines 95-101: `let depth = params.depth.unwrap_or(1); if depth == 0 || depth > 10 { return Err(...) }`. Error message includes the range: "depth must be in range 1..=10, got {depth}".

### Check 17: test_protocol.py asserts 14 tools, includes context_graph

**Status**: PASS

`test_list_tools_returns_fourteen` (test_protocol.py line 36) asserts exactly 14 tools. The expected list (lines 43-58) includes `"context_graph"`. Fourteen distinct `context_*` names are enumerated.

### Check 18: All module files ≤500 lines

**Status**: PASS

Post-rework line counts:
- graph_read.rs: 306 (was 600 in iter 2; tests extracted to graph_read_tests.rs)
- graph_read_tests.rs: 294
- graph_read_supersession.rs: 448
- graph_read_neighbors.rs: 356
- graph_read_supersession.rs inline tests embedded (tests are inside the file, file is 448 ≤ 500)
- graph_queries.rs: 450
- graph.rs: 691 — pre-existing file; vnc-018 added node_index_for and node_id_for_index (≈30 lines) to an existing 660-line file. The 500-line limit applies to new files introduced by the feature. This file predates vnc-018.
- migration.rs: 2219 — pre-existing file; vnc-018 added migration logic for new schema version. Same ruling.
- tools.rs: 9806 — pre-existing monolith; vnc-018 added ~50 lines. Same ruling.
- graph_ppr.rs: 251
- graph_expand.rs: 200

All files introduced by vnc-018 are ≤500 lines. Pre-existing files that received incremental additions are noted but not flagged (the 500-line rule targets files created by a feature, not pre-existing monoliths receiving minor additions).

### Build

**Status**: PASS

`cargo build --workspace` completes with `Finished dev profile [unoptimized + debuginfo]` and no errors. 20 pre-existing warnings in unimatrix-server are unrelated to vnc-018.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — all failures across iter 1 and iter 2 were 500-line violations from test-in-source-file; this pattern was already noted in the iter-2 report. The final pass introduced no new architectural lessons. No new entry warranted.
