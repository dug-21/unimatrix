# Test Plan: edge_write.rs Helper Module

**Component**: `crates/unimatrix-server/src/mcp/edge_write.rs` (new file)
**Architecture ref**: Component 2
**Risk coverage**: R-02, R-03, R-04, R-15
**AC coverage**: AC-05, AC-06, AC-10, AC-18, AC-25, AC-26

---

## Key Constraints

- All functions in this module are `pub(crate)`. No external test crate can import them directly.
- Unit test coverage must be inline (within `edge_write.rs` or via test modules in the same crate).
- Integration-level coverage for the write path flows through `context_store`, `context_correct`,
  and `context_edge` MCP handlers.

---

## Unit Test Expectations

### Location: inline in `crates/unimatrix-server/src/mcp/edge_write.rs`

#### test_edge_source_agent_constant_value
- Assert: `EDGE_SOURCE_AGENT == "agent"`
- Note: simple value assertion; distinct from magic string usage (R-15 partial coverage)

#### test_edge_source_agent_distinctness
- Assert: `EDGE_SOURCE_AGENT != ""` and `EDGE_SOURCE_AGENT != "system"` and `EDGE_SOURCE_AGENT != "human"`
- Note: pattern #4046 — constant distinctness from sibling constants

#### test_edge_validation_error_variants_exist
- Assert: `EdgeValidationError::UnknownType { edge_type: "x".to_string() }` constructs without panic
- Assert: `EdgeValidationError::SelfReferential { id: 1 }` constructs without panic
- Assert: `EdgeValidationError::TargetNotFound { target_id: 1 }` constructs without panic
- Assert: `EdgeValidationError::TargetQuarantined { target_id: 1 }` constructs without panic

---

## Integration Test Expectations

These are exercised through MCP handlers in the `tools` and `lifecycle` suites.

### Write Path: validate_and_write_edges (via context_store)

#### test_store_with_edges_source_agent_attribution (R-15, AC-05, AC-18)
- Arrange: store entry with `edges: [{Supports, target_id}]`
- Act: call `context_store`
- Assert: `SELECT source FROM graph_edges WHERE source_id=? AND target_id=? AND relation_type='Supports'` returns `"agent"`
- Assert: No magic string `"system"` or `""` in the source column for agent-declared edges

#### test_validate_and_write_edges_idempotent_non_contradicts (R-03, AC-10)
- Arrange: store entry A; call `context_store` with `edges: [{Supports, B}]` → entry C
- Act: call again with same content + `edges: [{Supports, B}]` → expected duplicate, OR
  call `context_edge(mode: "add", ...)` on same triplet
- Assert: `SELECT COUNT(*) FROM graph_edges WHERE source_id=C.id AND target_id=B.id AND relation_type='Supports'` == 1
- Assert: no error returned on second write (INSERT OR IGNORE is not an error)
- Note: confirms the `false` return from UNIQUE conflict is treated as success, not error (R-03)

#### test_validate_and_write_edges_idempotent_contradicts (R-03)
- Arrange: store entry with `edges: [{Contradicts, target_id}]` → first write creates 2 rows (A→B, B→A)
- Act: assert same Contradicts edge again via `context_edge(mode: "add")`
- Assert: `SELECT COUNT(*) FROM graph_edges WHERE relation_type='Contradicts' AND ((source_id=A AND target_id=B) OR (source_id=B AND target_id=A))` == 2 (not 4)
- Assert: no error returned
- Note: confirms both INSERT OR IGNORE calls are treated as no-ops, not errors (R-03)

#### test_contradicts_both_directions_written (R-04, AC-06)
- Arrange: two existing entries A and B
- Act: call `context_store` with `edges: [{Contradicts, B.id}]` → creates entry C
  (and separately with `context_edge(mode: "add")` from A to B)
- Assert: both `(source=A, target=B, Contradicts)` AND `(source=B, target=A, Contradicts)` rows present
- Note: one test per surface (edges param path + context_edge path) to satisfy R-04 per-surface requirement

#### test_contradicts_both_directions_via_context_edge_add (R-04, AC-06)
- Arrange: existing entries A and B; agent with Write capability
- Act: `context_edge(mode: "add", source_id=A, edge_type="Contradicts", target_id=B)`
- Assert: `(A, B, Contradicts)` present AND `(B, A, Contradicts)` present in GRAPH_EDGES

### Delete Path: delete_graph_edge (via context_edge remove)

#### test_delete_graph_edge_non_contradicts_only_one_row (AC-25)
- Arrange: write `(A, B, Supports)` via context_edge add
- Act: `context_edge(mode: "remove", source_id=A, edge_type="Supports", target_id=B)`
- Assert: `(A, B, Supports)` row gone
- Assert: no other rows deleted (only the specified triplet)

#### test_delete_graph_edge_contradicts_both_directions (AC-25)
- Arrange: write `(A, B, Contradicts)` — both directions present
- Act: `context_edge(mode: "remove", source_id=A, edge_type="Contradicts", target_id=B)`
- Assert: `(A, B, Contradicts)` gone AND `(B, A, Contradicts)` gone
- Note: both rows deleted before handler returns (not deferred to tick)

#### test_delete_graph_edge_idempotent (AC-25)
- Arrange: no edge exists for `(A, B, Supports)`
- Act: `context_edge(mode: "remove", source_id=A, edge_type="Supports", target_id=B)`
- Assert: success returned (0 rows affected is not an error)
- Act: call again
- Assert: still success

### Redirect Path: redirect_graph_edge (via context_edge redirect) — R-02, R-05, AC-26

#### test_redirect_graph_edge_non_contradicts_atomic (R-02, R-05, AC-26)
- Arrange: write `(A, B, Supports)` edge
- Act: `context_edge(mode: "redirect", source_id=A, edge_type="Supports", target_id=B, new_target_id=C)`
- Assert: `(A, B, Supports)` row ABSENT
- Assert: `(A, C, Supports)` row PRESENT
- Assert: both in a single assertion block (atomic observation)

#### test_redirect_graph_edge_contradicts_all_four_rows (R-02, AC-26)
- Arrange: write Contradicts edge A↔B (both directions)
- Act: `context_edge(mode: "redirect", source_id=A, edge_type="Contradicts", target_id=B, new_target_id=C)`
- Assert: `(A, B, Contradicts)` ABSENT, `(B, A, Contradicts)` ABSENT
- Assert: `(A, C, Contradicts)` PRESENT, `(C, A, Contradicts)` PRESENT
- Note: all 4 row assertions in one block (R-02 Coverage Requirement — Contradicts 4-row atomic case)

#### test_redirect_graph_edge_rollback_on_bad_new_target (R-02, R-05, AC-26)
- Arrange: write `(A, B, Supports)` edge; new_target_id=999999 (non-existent)
- Act: `context_edge(mode: "redirect", ..., new_target_id=999999)`
- Assert: error returned (TargetNotFound)
- Assert: `(A, B, Supports)` row still PRESENT (original edge survived — ROLLBACK confirmed)
- Note: This is the critical rollback-on-failure test for R-05. Uses TargetNotFound to simulate
  validation failure before transaction; if redirect implementation validates AFTER opening
  transaction, a different inject mechanism may be needed but the key assertion is row survival.

#### test_redirect_graph_edge_same_target_noop (AC-26 edge case)
- Arrange: write `(A, B, Supports)` edge
- Act: `context_edge(mode: "redirect", source_id=A, edge_type="Supports", target_id=B, new_target_id=B)`
- Assert: success returned (no error)
- Assert: `(A, B, Supports)` row present (DELETE + re-INSERT = net no change)

---

## Code Review Gates

These are structural properties that cannot be verified by test execution alone.
Stage 3c must grep the implementation for these patterns.

### R-02: RAII Transaction Gate
- `redirect_graph_edge` body must contain `pool.begin().await` (or equivalent RAII)
- Must NOT contain `sqlx::query("BEGIN")` or `"COMMIT"` as SQL string literals
- All 4 SQL statements for Contradicts redirect must execute against `&mut *txn`

### R-03: Three-Case Contract Gate
- `validate_and_write_edges` write loop must handle `bool` return from `write_graph_edge`
- Must NOT treat `false` (UNIQUE conflict) as an error that aborts the loop
- Must log (not surface) true SQL errors

### R-15: EDGE_SOURCE_AGENT Constant Usage Gate
- All `write_graph_edge` calls in `edge_write.rs` must pass `EDGE_SOURCE_AGENT` for the `source`
  and `created_by` parameters
- No inline `"agent"` string literals at call sites
