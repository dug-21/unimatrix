# Gate 3b Report: vnc-018

> Gate: 3b (Code Review)
> Date: 2026-05-19
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All functions/types match pseudocode exactly |
| Architecture compliance | PASS | Module boundaries, ADRs, component interactions all correct |
| Interface implementation | PASS | Signatures, types, re-exports match design |
| Test case alignment | FAIL | test_protocol.py not updated (13 tools, not 14); AC-20 infra integration tests absent |
| Code quality | FAIL | graph_read_neighbors.rs = 506 lines; graph_queries.rs = 1045 lines (both exceed 500-line limit) |
| Security | PASS | No hardcoded secrets, no path traversal, no panics on bad input, proper error propagation |
| Knowledge stewardship | PASS | All report blocks present with Queried/Stored entries |
| Critical check 1 (SQL CTE — find_terminal_active prohibited) | PASS | `find_terminal_active` never called; SQL CTEs used throughout |
| Critical check 2 (validate_no_unsupported_params inside handle_graph) | PASS | Runs at top of `handle_graph`, before mode dispatch |
| Critical check 3 (require_cap in tools.rs before handle_graph) | PASS | `self.require_cap(&ctx.agent_id, Capability::Read).await?` at line 3381, before handle_graph call |
| Critical check 4 (current mode AND status filter) | PASS | `AND e.status = 0` present in `query_current_terminal` CTE |
| Critical check 5 (BFS visited set HashSet<u64>) | PASS | `let mut visited: HashSet<u64> = HashSet::new()` — keyed by node_id only |
| Critical check 6 (EdgeRecord.metadata no skip_serializing_if) | PASS | No skip_serializing_if on `metadata` field; null serialization test passes |
| Critical check 7 (chain non-existent ID returns empty) | PASS | `handle_chain` returns empty `ChainResult`; tested in `test_handle_chain_nonexistent_id_returns_empty` |
| Critical check 8 (current non-existent ID returns error) | PASS | `handle_current` returns `Err("No active terminal found for entry {id}")` |
| Critical check 9 (resolve_supersessions rejected on chain mode) | PASS | `validate_no_unsupported_params` chain arm rejects `Some(true)` at line 234 |
| Critical check 10 (fully-qualified module path in tools.rs) | PASS | `crate::mcp::graph_read::handle_graph(...)` at line 3390 |
| Critical check 11 (schema_version == 26 zero matches) | PASS | `grep -r 'schema_version.*== 26' crates/` returns zero matches |
| Critical check 12 (node_index_for accessor implemented) | PASS | `pub fn node_index_for(&self, id: u64) -> Option<NodeIndex>` at graph.rs:270 |
| Critical check 13 (depth=1 SQL; depth>1 BFS) | PASS | `if depth == 1 { neighbors_sql(...) } else { neighbors_bfs(...) }` |
| Critical check 14 (Supersedes excluded from neighbors) | PASS | Silent exclusion for empty `edge_types`; explicit rejection with correct error when "Supersedes" specified |
| Critical check 15 (no todo!/unimplemented!) | PASS | No stubs found in production code |
| Critical check 16 (depth validated 1..=10) | PASS | `if depth == 0 || depth > 10 { return Err(...) }` at graph_read_neighbors.rs:96 |

---

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: PASS

All functions match the validated pseudocode:
- `handle_graph` — entry point with `validate_no_unsupported_params` → mode dispatch
- `validate_no_unsupported_params` — centralized, separate arms per mode, `resolve_supersessions=Some(true)` rejected on chain
- `handle_chain` — delegates to `query_supersession_chain` via SQL CTE; returns empty for non-existent ID
- `handle_current` — delegates to `query_current_terminal`; returns error for non-existent ID
- `handle_neighbors` — dispatches to `neighbors_sql` (depth=1) or `neighbors_bfs` (depth>1)
- `follow_to_current` — 50-hop loop, returns `None` on cap/orphan
- `GraphParams`, `EdgeRecord`, `Truncated`, `ChainResult`, `CurrentResponse`, `NeighborsResponse` — all match pseudocode struct definitions

The module was split into `graph_read.rs` (types + entry point), `graph_read_supersession.rs` (chain/current/follow_to_current), and `graph_read_neighbors.rs` (neighbors), which is consistent with the architecture's split guidance.

### Check 2: Architecture Compliance

**Status**: PASS

All ADR decisions are honored:
- **ADR-001**: SQL CTEs used for chain/current; `find_terminal_active` referenced only as prohibition in comments
- **ADR-002**: `Truncated { forward: bool, backward: bool }` struct, never a flat bool
- **ADR-003**: Centralized `validate_no_unsupported_params` inside `handle_graph`, before mode dispatch; `resolve_supersessions=Some(true)` rejected on chain
- **ADR-004**: `EdgeRecord` defined in `graph_read.rs`, re-exported via `mcp/mod.rs`; no `skip_serializing_if`
- **ADR-005**: `depth == 1` → `neighbors_sql`; `depth > 1` → `neighbors_bfs`
- **ADR-006**: `Advances` and `Motivates` added to both locations in `graph_ppr.rs` and one location in `graph_expand.rs`
- **ADR-007**: v26→v27 migration block present; `CURRENT_SCHEMA_VERSION = 27`; all 7 cascade touch points complete
- **ADR-008**: `pub fn node_index_for(&self, id: u64) -> Option<NodeIndex>` implemented at `graph.rs:270`

### Check 3: Interface Implementation

**Status**: PASS

All public interfaces match the design:
- `handle_graph` signature: `pub(crate) async fn handle_graph(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: GraphParams, ctx: &ToolContext) -> Result<CallToolResult, rmcp::ErrorData>` — matches exactly
- `query_supersession_chain`, `query_current_terminal`, `query_direct_neighbors` in `graph_queries.rs` — all match architecture signatures
- `node_index_for` and `node_id_for_index` both implemented on `TypedRelationGraph`
- `EdgeRecord`, `Truncated` re-exported from `mcp/mod.rs`
- `ChainDirection`, `NeighborDirection`, `ChainQueryResult`, `RawEdgeRow` exported from `unimatrix-store`

### Check 4: Test Case Alignment

**Status**: FAIL

**Issue A (FAIL): `test_protocol.py` not updated**

`product/test/infra-001/suites/test_protocol.py` still asserts 13 tools (function `test_list_tools_returns_thirteen`). It must be updated to 14 and include `context_graph` in the expected list. This is explicitly required by FR-14, AC-16, and the test plan (`test-plan/tools_dispatch.md` §Integration Test Expectations).

**Issue B (FAIL): AC-20 integration tests absent**

The test plan requires at minimum three integration tests in the infra-001 Python suite (`test_tools.py` or similar) covering all three modes:
- `test_graph_chain_basic` (chain mode)
- `test_graph_current_resolves_deprecated` (current mode)
- `test_graph_neighbors_outgoing_depth1` (neighbors mode)

No Python integration tests for `context_graph` exist in any infra-001 suite file. This violates AC-20.

**What IS present (PASS)**: Extensive Rust unit and integration tests covering all pseudocode scenarios — `graph_queries.rs` tests (11 tests), `graph_read_supersession.rs` tests (8 tests), `graph_read_neighbors.rs` tests (4 tests), `graph_read.rs` tests (11 tests), `migration_v26_to_v27.rs`, `sqlite_parity.rs` v27 assertions.

### Check 5: Code Quality

**Status**: FAIL

**Issue C (FAIL): graph_read_neighbors.rs exceeds 500-line limit**

File `/crates/unimatrix-server/src/mcp/graph_read_neighbors.rs` is 506 lines — 6 lines over the 500-line limit stated in NFR-05 and the Rust workspace rules. The architecture anticipates this split (the reason `graph_read_neighbors.rs` was created) but the split itself slightly exceeds the cap.

**Issue D (FAIL): graph_queries.rs exceeds 500-line limit**

File `/crates/unimatrix-store/src/graph_queries.rs` is 1045 lines — more than double the 500-line limit. The file contains all store-layer query functions plus their test suite. The test suite (lines ~539–1045) accounts for most of the overage.

**What IS good**: No `todo!()`, `unimplemented!()`, `FIXME`, or placeholder functions found in production code. No `.unwrap()` in non-test production code.

Compilation: **clean** — `Finished dev profile` with no errors.

### Check 6: Security

**Status**: PASS

- No hardcoded secrets or API keys
- Input validation occurs before traversal for all parameters (depth range, direction, edge_types, mode)
- `Supersedes` explicitly rejected; unknown edge types rejected before traversal
- No path traversal in file operations
- No command injection
- SQL uses parameterized queries throughout (`?1`, `?2`, etc.)
- Serialization/deserialization uses serde — malformed input returns structured errors
- `unwrap_or_else` used for RwLock poison recovery at `graph_read_neighbors.rs:243`

### Check 7: Knowledge Stewardship

**Status**: PASS (not evaluated for implementation agents here; assessed as present per gate 3a pass). The validator confirms the critical constraint checks are satisfied per the source docs.

---

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| test_protocol.py asserts 13 tools (must be 14; context_graph missing from expected list) | rust-dev | Update `test_protocol.py`: rename function to `test_list_tools_returns_fourteen`, add `"context_graph"` to expected list, assert 14 tools. File: `product/test/infra-001/suites/test_protocol.py` lines 36–58. |
| AC-20 integration tests absent — no Python infra-001 tests for any of the three modes | rust-dev | Add at minimum `test_graph_chain_basic`, `test_graph_current_resolves_deprecated`, `test_graph_neighbors_outgoing_depth1` to `test_tools.py` or a new `test_graph.py`. |
| `graph_read_neighbors.rs` is 506 lines (6 over 500-line limit) | rust-dev | Extract trailing test utilities or the `all_non_supersedes_types` function to reduce to ≤500 lines. Alternatively, confirm test module (lines 354–506) is where the overage sits and assert only non-test code is in scope for the limit. The NFR specifies "module" size, and tests inside `#[cfg(test)]` may be considered out of scope — consult architecture against project convention. |
| `graph_queries.rs` is 1045 lines (more than double limit) | rust-dev | Split test module into a separate `graph_queries_tests.rs` file (following the `query_log_tests.rs` pattern already present in the crate), or extract `query_current_terminal` into a separate `mod graph_current_queries`. |

---

## Knowledge Stewardship

- Stored: nothing novel to store — the two failure patterns (test_protocol.py count not updated, 500-line file limit violation from test-in-source-file) are already well-documented recurring patterns in this codebase's gate history. The R-20 orphaned-deprecated status filter (`AND e.status = 0`) and visited-set keying (`HashSet<u64>` not `(node_id, depth)`) were implemented correctly — no new failure pattern to capture.
