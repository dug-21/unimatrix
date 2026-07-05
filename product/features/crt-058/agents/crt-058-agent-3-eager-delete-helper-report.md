# Agent Report — crt-058-agent-3-eager-delete-helper

## Scope
Wave A: `eager-delete-helper` in `crates/unimatrix-server/src/mcp/edge_write.rs` — new
`RemovedEdge` struct + `delete_agent_edges_for_entry`, plus its DB-integration tests.

## Files Modified
- `crates/unimatrix-server/src/mcp/edge_write.rs` — added `#[derive(Debug, serde::Serialize)] pub(crate) struct RemovedEdge`; added `pub(crate) async fn delete_agent_edges_for_entry(store, entry_id) -> Result<Vec<RemovedEdge>, EdgeDeleteError>` beside `delete_graph_edge`; declared `#[cfg(test)] #[path="edge_write_delete_agent_tests.rs"] mod delete_agent_tests`.
- `crates/unimatrix-server/src/mcp/edge_write_delete_agent_tests.rs` — NEW test file (10 tests).
- `crates/unimatrix-server/src/background.rs` — promoted `insert_graph_edge_with_source` out of `#[cfg(test)] mod tests` to module-level `#[cfg(test)] pub(crate) async fn` so the edge_write tests and the later eager ⊆ tick subset test seed from ONE shared helper (no copy). Existing background tests unchanged (resolve it via `use super::*`).

## Implementation Notes
- LOCKED predicate implemented verbatim: `DELETE FROM graph_edges WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING source_id, target_id, relation_type`, `?1 = entry_id as i64`, `?2 = EDGE_SOURCE_AGENT`, on `write_pool_server()`.
- SINGLE atomic statement: one `fetch_all` on the `DELETE … RETURNING` (R-03). Count derives from `tuples.len()` (single source of truth for both inline count and audit), never `rows_affected()`.
- No relation_type widening; no runtime `superseded_by` clause. Code-adjacency doc comment links the helper to `run_orphaned_edge_compaction` as the backstop and states the single-caller contract (C-10/C-11/SR-05).
- i64→u64 cast on RETURNING marshal; `use sqlx::Row;` scoped in the fn.

## Tests — 10 new, all passing (20/20 in `edge_write` filter)
- `test_delete_agent_edges_for_entry_removes_inbound_and_outbound_returns_ok` (AC-01/FR-01)
- `test_delete_agent_edges_only_removes_agent_source` (AC-04a per-source matrix: agent/nli/co_access/cosine_supports/S1/S2/S8 → only agent removed)
- `test_delete_returning_is_single_statement_capture` (R-03: captured == pre-count matched, 0 remain)
- `test_count_source_of_truth_is_tuples_len_not_rows_affected` (R-03)
- `test_delete_agent_edges_empty_match_returns_ok_empty` + `..._no_edges_at_all_...` (R-07)
- `test_self_loop_agent_edge_removed_and_counted_once` (R-10 — OR matches once)
- `test_high_degree_entry_all_agent_edges_removed` (R-10 — 50 mixed-direction agent edges)
- `test_shared_edge_removed_by_first_deprecation` (edge case — second call returns empty)
- `test_helper_predicate_and_pool_are_locked` (R-02 predicate pin via `include_str!`; asserts WHERE terminates at `?2` so no clause can be appended)

Command: `cargo test -p unimatrix-server edge_write` → `20 passed; 0 failed`.

## Issues / Flags
- **ADJACENT BREAKAGE (out of my scope — NOT fixed, flagging per instructions):** as of my final clippy run the crate lib fails to compile with 2×E0061 in `crates/unimatrix-server/src/mcp/tools.rs:1443` and `:1478` — `format_deprecate_success` now takes 4 args (the response-formatter agent's `mutations.rs` signature change adding `edges_removed: Option<u64>` has landed) but these two `context_deprecate` call sites still pass 3 args. This is the deprecate-handler's Wave B wiring, not mine. My edge_write tests passed when the lib was last consistent; my files (`edge_write.rs`, `background.rs`) produce zero clippy warnings/errors. No action needed from me; the handler agent must thread `Some(count)`/`None` into these two call sites (and step-5 early-return passes `None`, per OVERVIEW).
- The shared worktree is being edited concurrently by Wave A/B agents; the lib toggled from compiling (my test run) to E0061 (my later clippy) mid-session. Leader: run a final `cargo build -p unimatrix-server` after all waves land.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern, "graph_edges DELETE RETURNING write_pool sqlx") — surfaced #3883 (tick writes use `write_pool_server()` directly), #4124 (ADR-006: behavioral edge writes use write_pool, not analytics drain), #4465 (graph_edges test-helper column try_get). Applied: used `write_pool_server()` directly and the named-column `row.get::<T,_>("col")` marshal.
- Stored: entry #5467 "Eager subset-delete of graph_edges: DELETE...RETURNING via fetch_all is atomic capture; pin the WHERE terminator, share the seed helper" via context_store (pattern, topic `unimatrix-server`).
