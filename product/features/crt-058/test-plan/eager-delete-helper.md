# Test Plan — eager-delete-helper (`edge_write.rs`)

**Unit under test:** `async fn delete_agent_edges_for_entry(store: &Store, entry_id: u64) -> Result<Vec<RemovedEdge>, EdgeDeleteError>` and `struct RemovedEdge { source_id, target_id, relation_type }`.
**LOCKED predicate:** `DELETE FROM graph_edges WHERE (source_id = ?1 OR target_id = ?1) AND source = ?2 RETURNING source_id, target_id, relation_type` (`?2` = `EDGE_SOURCE_AGENT`, `write_pool_server()`).

**Placement:** new DB-integration `mod` (`#[cfg(test)] #[path="edge_write_delete_agent_tests.rs"]`) in `edge_write.rs`; keep the existing pure `mod tests` (`:420`). Seed via the shared `insert_graph_edge_with_source` helper (background.rs test mod pattern) — one shared seeding helper across this file and the subset test (R-02 fixture identity). If unreachable, promote it to `pub(crate)`; do not copy.

## Test Expectations

### AC-01 / FR-01 — both directions
- `test_delete_agent_edges_for_entry_removes_inbound_and_outbound_returns_ok`
  - Arrange: entry E; seed inbound (`target_id=E`, `source='agent'`) and outbound (`source_id=E`, `source='agent'`) edges.
  - Act: `delete_agent_edges_for_entry(&store, E)`.
  - Assert: `Ok(tuples)`, `tuples.len() == 2`; post-call `SELECT` by `target_id=E AND source='agent'` and `source_id=E AND source='agent'` → 0 rows each.

### AC-04(a) / FR-02 / R-09 — per-source removal matrix
- `test_delete_agent_edges_only_removes_agent_source`
  - Arrange: seed exactly one edge of EACH `source` touching E: `agent`, `nli`, `co_access`, `cosine_supports`, `S1`, `S2`, `S8` (mix inbound/outbound).
  - Act: delete helper on E.
  - Assert: returned tuples == exactly the one `agent` edge; every machine source still present in `graph_edges`. A newly-added source surfaces as "not removed" (documents enumeration-bound + subset-safe completeness).

### R-03 — atomic single-statement RETURNING (delivery-time closure)
- `test_delete_returning_is_single_statement_capture`
  - Assert (structural + behavioral): the helper performs ONE `fetch_all` on the `DELETE ... RETURNING` — no delete-then-separate-select. Verify by: (a) pinning that the count returned equals the number of rows actually removed (`tuples.len()` == pre-count of matching rows) in one call; (b) code-review assertion that there is no second `SELECT` between delete and capture. There must be no window where rows are deleted but tuples are lost.
- `test_count_source_of_truth_is_tuples_len_not_rows_affected`
  - Assert the reported count derives from `tuples.len()` (needed for audit), and equals the number of removed rows. For a single RETURNING they agree — pin `tuples.len()` as the single source.

### R-07 — zero-row tolerance (concurrent-tick already swept)
- `test_delete_agent_edges_empty_match_returns_ok_empty`
  - Arrange: entry E with only machine edges (or none).
  - Act: delete helper.
  - Assert: `Ok(vec![])`, `len()==0`, no panic. (Feeds the `Some(0)` advisory + no-audit-on-empty path.)

### R-10 — self-loop / high-degree (delivery-time closure)
- `test_self_loop_agent_edge_removed_and_counted_once`
  - Arrange: seed one agent edge with `source_id == target_id == E`.
  - Assert: removed; `tuples.len() == 1` (the `OR` matches once, not doubled).
- `test_high_degree_entry_all_agent_edges_removed`
  - Arrange: seed many (e.g. 50) agent edges touching E, mixed directions, plus a few machine edges.
  - Assert: all agent edges in `tuples`, all gone from `graph_edges`; machine edges remain; count == number seeded.

### NFR-01 / NFR-02 / AC-08 — statement shape & pool (grep/structural)
- `test_helper_uses_write_pool_and_single_statement`
  - Assert (grep/structural over `edge_write.rs`): helper calls `store.write_pool_server()` (not read pool); exactly one `DELETE` statement, no per-edge loop; `source` bound as `?2` to `EDGE_SOURCE_AGENT` constant, not user input; no relation-type clause.
- `test_predicate_targets_graph_edges_no_schema_change` — grep confirms target table `graph_edges`, no new migration.

### Edge cases (from Risk Strategy §Edge Cases)
- Two entries sharing one agent edge, deprecated in sequence → first removes it, second's RETURNING omits it (already gone); count attributed to the first. Covered by `test_shared_edge_removed_by_first_deprecation` (may live in deprecate-handler if it needs the handler; helper-level version asserts second call returns empty).

## Notes for delivery
- `RemovedEdge` fields are `u64` (ids) + `String` (relation_type); ensure `RETURNING` columns deserialize without an intermediate `i64`→`u64` panic on the boundary.
- Leave the code-adjacency comment (C-11, SR-05) linking the helper to `run_orphaned_edge_compaction` as the backstop — grep-assert its presence if cheap.
