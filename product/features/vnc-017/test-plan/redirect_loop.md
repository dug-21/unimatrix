# Test Plan: redirect_loop

**Component**: `crates/unimatrix-server/src/mcp/tools.rs` — redirect block in `context_correct` handler (Step 8c)
**Constant**: `REDIRECT_CEILING: usize = 50`
**Dependencies**: `query_incoming_edges` (store), `redirect_graph_edge` (edge_write.rs), `store.get` (status check)

---

## Unit Test Strategy

The redirect loop runs inline in the `context_correct` handler. Unit tests exercise the full handler with an in-memory SQLite database. `redirect_graph_edge` is called for real (it uses a write transaction on the same in-memory store); source validation uses `store.get`.

For partial-failure scenarios (AC-04, AC-13), a test double or a dedicated test that seeds a corrupt edge row that causes a SQL error is used. The exact mechanism (trait injection or in-test corruption) is the implementer's choice; the assertion target is the same either way.

---

## R-01 (Critical): Compile-time return contract structural test

**What**: Verify that the redirect loop match arm handles `Ok(())` and `Err(EdgeRedirectError)` and that no `Ok(bool)` arm exists.

**How**: This is enforced by the Rust compiler. If the implementer writes `Ok(true)` or `Ok(false)` match arms, the code will not compile against the `Result<(), EdgeRedirectError>` return type. The test is the build itself.

**Explicit behavioral test**: Seed edge `C → A` (any non-Supersedes relation). Call `context_correct(A → B)`. Assert `redirected == 1`, `failed == 0`. The `Ok(bool)` case is unreachable — confirming the loop correctly matches `Ok(())`.

**Assert also**: No `tracing::warn!` log is emitted for the success case.

**Test name**: `test_redirect_loop_ok_unit_increments_redirected_not_failed`

---

## AC-14: End-to-end redirect via in-memory SQLite

**Arrange**:
- Insert entries A (Active) and B (placeholder for new entry).
- Insert `graph_edges` row: `(source_id=C_id, target_id=A_id, relation_type="Prerequisite", created_at=now)`.
- Call `context_correct` with `original_id=A_id` and new content (produces new entry B).

**Act**: `context_correct(A → B)` handler runs.

**Assert**:
- `graph_edges` row `(source_id=C_id, target_id=A_id)` no longer exists.
- `graph_edges` row `(source_id=C_id, target_id=B_id, relation_type="Prerequisite")` exists.
- Response is well-formed (contains `deprecated_original` and `corrected_entry` fields).
- Response text contains `"Redirected 1 incoming edges"`.

**Test name**: `test_redirect_loop_end_to_end_moves_edge_to_new_target`

---

## AC-03: Terminal-active is always new_entry.id

**Arrange**: Create correction chain A → B (B is Deprecated, superseded by C). Now call `context_correct(A' → B)` where A' is a fresh Active entry being corrected to B. Note: context_correct can only be called on Active entries, so B must be Active for the call to proceed. To test the structural assertion:

**Alternate framing**: Seed two corrections sequentially: `context_correct(A → B)`, then `context_correct(X → Y)` where X had an edge pointing at A. After the first correction, an edge `E → A` is redirected to `E → B`. No second-hop traversal to C (beyond the current correction) is attempted.

**Assert**:
- The redirect target is always the ID of the entry just created (`correct_result.corrected_entry.id`).
- No call to `find_terminal_active` is present in the code path (code review structural gate).
- No read lock on `TypedGraphState` is acquired in the redirect loop (code review structural gate).

**Test name**: `test_redirect_loop_targets_new_entry_not_chain_traversal`

---

## AC-04: Correction succeeds even when redirect_graph_edge returns Err

**Arrange**:
- Insert entry A (Active), one incoming edge `C → A`.
- Configure a mechanism to cause `redirect_graph_edge` to return `Err(EdgeRedirectError::TransactionError(...))` for that edge.

**Act**: Call `context_correct(A → B)`.

**Assert**:
- Handler returns a well-formed success response (not an error).
- Response contains `deprecated_original` field with A's data.
- Response contains `corrected_entry` field with B's data.
- Response text contains `"Redirected 0 incoming edges (1 failed, see logs)"`.
- `tracing::warn!` was emitted for the failed edge.
- Entry A has status `Deprecated` in the database.
- Entry B has status `Active` in the database.

**Test name**: `test_redirect_loop_correction_succeeds_when_redirect_fails`

---

## AC-08: Quarantined source — skipped, not failed

**Arrange**:
- Insert entry A (Active), entry B_src (Quarantined, the source).
- Insert `graph_edges` row: `(source_id=B_src_id, target_id=A_id, relation_type="Contradicts")`.

**Act**: Call `context_correct(A → C)`.

**Assert**:
- Edge row `(source_id=B_src_id, target_id=A_id)` is UNCHANGED in `graph_edges`.
- No edge `(source_id=B_src_id, target_id=C_id)` was inserted.
- Response text contains `"Redirected 0 incoming edges (1 skipped — invalid source, 0 failed, see logs)"`.
- `tracing::warn!` was emitted referencing `B_src_id` and "quarantined".
- `failed == 0`, `skipped == 1`.

**Test name**: `test_redirect_loop_quarantined_source_skipped_not_failed`

---

## R-06 variant: Deprecated source — skipped, not failed

**Arrange**: Same as AC-08 but `B_src` has status `Deprecated`.

**Assert**:
- Same assertions as AC-08 variant — edge unchanged, `skipped == 1`, `failed == 0`, `tracing::warn!` with "deprecated".

**Test name**: `test_redirect_loop_deprecated_source_skipped_not_failed`

---

## R-06 (Critical): Mixed-status fan-in — Active + Quarantined sources

**Arrange**:
- Entry A (Active, target).
- Entry SRC_VALID (Active, source of edge 1).
- Entry SRC_BAD (Quarantined, source of edge 2).
- Insert `graph_edges`:
  - `(source_id=SRC_VALID_id, target_id=A_id, relation_type="Contradicts")`
  - `(source_id=SRC_BAD_id, target_id=A_id, relation_type="Contradicts")`

**Act**: Call `context_correct(A → B)`.

**Assert**:
- Edge from SRC_VALID is redirected: `(SRC_VALID → B)` and `(B → SRC_VALID)` both exist (Contradicts bidirectionality).
- Edge from SRC_BAD is unchanged: `(SRC_BAD → A)` still exists.
- No `(SRC_BAD → B)` edge was inserted.
- `redirected == 1`, `skipped == 1`, `failed == 0`.
- `tracing::warn!` emitted for SRC_BAD only.

**Test name**: `test_redirect_loop_mixed_status_redirects_valid_skips_invalid`

---

## AC-09 / R-09: UNIQUE conflict counts as success (idempotent redirect)

**Arrange**:
- Insert entry A (Active, original), entry B (result of correction, Active).
- Insert `graph_edges`:
  - `(source_id=C_id, target_id=A_id, relation_type="Prerequisite")` ← will be redirected
  - `(source_id=C_id, target_id=B_id, relation_type="Prerequisite")` ← already exists (conflict)

**Act**: Call the redirect loop for `A → B` (context_correct or direct invocation against in-memory store).

**Assert**:
- `redirected == 1` (the `Ok(())` from UNIQUE-conflict INSERT OR IGNORE is counted as success).
- `failed == 0`.
- No `tracing::warn!` emitted.
- `graph_edges` contains exactly one row `(C_id → B_id, Prerequisite)`.

**Test name**: `test_redirect_loop_unique_conflict_counts_as_success`

---

## AC-11: Zero incoming edges — no append, no log

**Arrange**: Insert entry A (Active) with no `graph_edges` rows with `target_id=A_id`.

**Act**: Call `context_correct(A → B)`.

**Assert**:
- Response text does NOT contain the substring "Redirected".
- No `tracing::info!` summary log is emitted.
- Response is otherwise identical to the current `format_correct_success` output.

**Test name**: `test_redirect_loop_no_incoming_edges_no_append_no_log`

---

## R-05: Fan-in ceiling — 55 edges, truncate at 50

**Arrange**: Insert entry A (Active). Insert 55 `graph_edges` rows with `target_id=A_id`, all with Active source entries and `relation_type="Prerequisite"`. Insert rows in deterministic order (source IDs 1..55).

**Act**: Call `context_correct(A → B)`.

**Assert**:
- Exactly 50 edges are redirected to B (50 rows with `target_id=B_id` in `graph_edges`).
- Exactly 5 edges remain pointing at A (5 rows with `target_id=A_id` and `relation_type != 'Supersedes'`).
- `tracing::warn!` is emitted with `total_found=55` (truncation warning).
- Response text contains `"(truncated from 55, see logs)"`.
- `redirected == 50`, `failed == 0`.

**Test name**: `test_redirect_loop_ceiling_truncates_at_50_and_warns`

---

## R-05 variant: Exactly 50 edges — no truncation

**Arrange**: Insert entry A, 50 incoming edges with Active sources.

**Act**: Call `context_correct(A → B)`.

**Assert**:
- All 50 edges redirected.
- No `tracing::warn!` for truncation.
- Response text does NOT contain "truncated".
- `redirected == 50`.

**Test name**: `test_redirect_loop_exactly_at_ceiling_no_truncation`

---

## R-10: Phase B + redirect loop double-write — no duplicate rows

**Arrange**:
- Insert entry A (Active).
- Entry B is the new correction target (will be created by `context_correct`).
- Insert `graph_edges`: `(source_id=C_id, target_id=A_id, relation_type="Prerequisite")`.
- In the `context_correct` call, include `C → B` as a declared Phase B edge (via `params.edges`).

**Act**: Call `context_correct(A → B)` with Phase B declaring `C → B`.

**Assert**:
- `graph_edges` contains exactly ONE row for `(source_id=C_id, target_id=B_id, relation_type="Prerequisite")`.
- No duplicate row.
- `failed == 0` (INSERT OR IGNORE absorbs the duplicate silently).

**Test name**: `test_redirect_loop_phase_b_collision_no_duplicate_row`

---

## Integration Test Expectations

The redirect loop's end-to-end behavior is verified in `test_lifecycle.py`. The integration tests assert:
- `graph_edges` table state via MCP tools that expose edge data (or direct DB query via test harness SQLite access if available).
- `CallToolResult` text from the `context_correct` response containing the exact redirect summary format string.

Specific integration test behaviors are planned in OVERVIEW.md. The unit tests cover all stub-based and error-injection scenarios that cannot be triggered through the MCP interface.

---

## Code Review Gates (Non-Test)

These are structural assertions that must be verified in code review, not via automated tests:

1. **R-14**: No lock is held on `TypedGraphState` between `store.get(source_id)` and `redirect_graph_edge(...)` (no lock introduces TOCTOU; this is accepted degraded state per ADR-003).
2. **C-07**: Comment at the `write_pool_server()` call site in `redirect_graph_edge` invocation notes the shared pool implementation detail.
3. **NFR-09**: `redirect_graph_edge`, `write_graph_edge`, `build_typed_relation_graph`, `TypedGraphState`, and `context_edge` handler are not modified.
4. **ADR-001**: No call to `find_terminal_active` in the redirect loop.
5. **NFR-01**: No `tokio::spawn` or fire-and-forget task for the redirect loop.
