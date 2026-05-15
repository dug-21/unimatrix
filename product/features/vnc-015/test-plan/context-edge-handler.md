# Test Plan: context_edge Handler (tools.rs)

**Component**: `crates/unimatrix-server/src/mcp/tools.rs`
**Architecture ref**: Component 9 (13th MCP tool)
**Risk coverage**: R-05, R-06, R-13
**AC coverage**: AC-15, AC-19, AC-20, AC-21, AC-22, AC-23, AC-24, AC-25, AC-26

---

## Key Constraints

- `context_edge` is the 13th MCP tool. Any test asserting exact tool count must be updated
  from 12 to 13 (OQ-4 — locate in Stage 3b).
- Validation pipeline is strictly ordered: capability → source fetch → source status →
  self-ref → edge type → target validation. Tests must verify each step fires before the next.
- No ownership check exists (by design — AC-22). `OwnershipViolation` error must NOT exist.
- `redirect_graph_edge` uses RAII transaction (`pool.begin()`) — covered by edge-write.md.
- `new_target_id` is required for redirect, rejected for add/remove.
- Pure graph operation: no embedding, no confidence, no duplicate detection.

---

## Unit Test Expectations

### Location: `crates/unimatrix-server/src/mcp/tools.rs` or `mcp/tests/`

#### test_edge_params_deserializes_valid_add
- Arrange: JSON `{"mode": "add", "source_id": 1, "edge_type": "Supports", "target_id": 2}`
- Act: `serde_json::from_str::<EdgeParams>(...)`
- Assert: all fields correct; `new_target_id == None`

#### test_edge_params_deserializes_valid_redirect
- Arrange: JSON with `"mode": "redirect"` and `"new_target_id": 3`
- Assert: `new_target_id == Some(3)`

#### test_edge_params_mode_strings
- Assert: `"add"`, `"remove"`, `"redirect"` are valid mode values
- Note: invalid modes (e.g., `"delete"`, `""`) must be rejected by the handler

---

## Integration Test Expectations

All tests in infra-001 `test_tools.py` unless noted.

### Tool Registration (AC-19)

#### test_context_edge_tool_registered (AC-19)
- Act: MCP initialize + tools/list
- Assert: response contains exactly 13 tools
- Assert: tool named `"context_edge"` is present
- Assert: `context_edge` schema includes parameters: `mode`, `source_id`, `edge_type`, `target_id`, `new_target_id`
- Assert: `new_target_id` is marked optional in schema

### Validation Pipeline Tests

#### test_context_edge_requires_write_capability (R-06, AC-15, AC-21)
- Arrange: enroll agent without `Capability::Write`
- Act: call `context_edge(mode: "add", ...)`
- Assert: permission error (existing gate behavior)
- Note: same gate as `context_store` — not a new implementation

#### test_context_edge_source_not_found
- Arrange: agent with Write; `source_id` references non-existent entry (e.g., 999999)
- Act: call `context_edge`
- Assert: error returned (SourceNotFound or equivalent)
- Assert: no GRAPH_EDGES mutation

#### test_context_edge_source_frozen_quarantined (R-06, AC-23)
- Arrange: store entry E; quarantine E via `context_quarantine` (admin required)
- Act: `context_edge(mode: "add", source_id=E.id, ...)`
- Assert: `SourceFrozen` error returned
- Assert: GRAPH_EDGES unchanged (no row written)

#### test_context_edge_source_frozen_deprecated (R-06, AC-23)
- Arrange: store entry E; deprecate E via `context_correct`
- Act: `context_edge(mode: "redirect", source_id=E.id, ...)` or any mode
- Assert: `SourceFrozen` error returned
- Assert: no mutation occurred

#### test_context_edge_active_source_succeeds_baseline (R-06)
- Arrange: store active entry E; agent with Write
- Act: `context_edge(mode: "add", source_id=E.id, edge_type="Supports", target_id=other_id)`
- Assert: success returned (active source is operatable)
- Note: This positive baseline is required — all three modes must have an active-source success test

#### test_context_edge_self_referential_rejected (AC-08)
- Arrange: store entry E with `id=N`
- Act: `context_edge(mode: "add", source_id=N, edge_type="Supports", target_id=N)`
- Assert: SelfReferential error returned
- Assert: no GRAPH_EDGES row written
- Note: self-ref check runs pre-operation in context_edge (source_id is known)

#### test_context_edge_unknown_edge_type_rejected
- Act: `context_edge(mode: "add", ..., edge_type="BogusType")`
- Assert: UnknownType error returned
- Assert: no GRAPH_EDGES mutation

#### test_context_edge_target_not_found (AC-24)
- Act: `context_edge(mode: "add", ..., target_id=999999)`
- Assert: TargetNotFound error

#### test_context_edge_quarantined_target_rejected (AC-24)
- Arrange: quarantine target entry T
- Act: `context_edge(mode: "add", ..., target_id=T.id)`
- Assert: TargetQuarantined error

#### test_context_edge_deprecated_target_allowed (AC-24)
- Arrange: deprecate target entry T
- Act: `context_edge(mode: "add", ..., target_id=T_deprecated.id)`
- Assert: success; GRAPH_EDGES row written

### Mode-Specific: new_target_id Validation (R-13)

#### test_context_edge_add_rejects_new_target_id (R-13)
- Act: `context_edge(mode: "add", ..., new_target_id=5)`
- Assert: error returned (new_target_id not valid for add mode)

#### test_context_edge_remove_rejects_new_target_id (R-13)
- Act: `context_edge(mode: "remove", ..., new_target_id=5)`
- Assert: error returned

#### test_context_edge_redirect_requires_new_target_id
- Act: `context_edge(mode: "redirect", ...) ` without `new_target_id`
- Assert: error returned (new_target_id required for redirect)

#### test_context_edge_invalid_mode_rejected
- Act: `context_edge(mode: "delete", ...)` (not a valid mode)
- Assert: error returned

### No Ownership Check (AC-22)

#### test_context_edge_no_ownership_check (AC-22)
- Arrange: enroll agents A and B; A stores entry E; B has Capability::Write
- Act: B calls `context_edge(mode: "add", source_id=E.id, ...)`
- Assert: success returned — no OwnershipViolation, no error
- Assert: GRAPH_EDGES row written

### No Side Effects (AC-20)

#### test_context_edge_no_side_effects (AC-20)
- Arrange: store entry; add edge via `context_edge`
- Act: observe server logs and response
- Assert: no embedding job logged (no "embedding" or "recompute" in server output)
- Assert: no confidence delta in response
- Assert: no duplicate detection log entry
- Note: this is best verified via server log inspection during the test run

### Add Mode (AC-24)

#### test_context_edge_add_basic_non_contradicts (AC-24)
- Arrange: store entries A and B; agent with Write
- Act: `context_edge(mode: "add", source_id=A.id, edge_type="Supports", target_id=B.id)`
- Assert: success
- Assert: GRAPH_EDGES row present: `(A.id, B.id, "Supports")`
- Assert: `source = 'agent'`

#### test_context_edge_add_contradicts_bidirectional (R-04, AC-06, AC-24)
- Arrange: entries A and B
- Act: `context_edge(mode: "add", source_id=A.id, edge_type="Contradicts", target_id=B.id)`
- Assert: `(A.id, B.id, "Contradicts")` present AND `(B.id, A.id, "Contradicts")` present

#### test_context_edge_add_idempotent (AC-24)
- Act: same `context_edge add` call twice
- Assert: exactly 1 GRAPH_EDGES row after second call (INSERT OR IGNORE)
- Assert: no error on second call

### Remove Mode (AC-25)

#### test_context_edge_remove_non_contradicts (AC-25)
- Arrange: add `(A, B, Supports)` via context_edge add
- Act: `context_edge(mode: "remove", source_id=A.id, edge_type="Supports", target_id=B.id)`
- Assert: success; row absent
- Assert: only that specific row removed (no collateral deletion)

#### test_context_edge_remove_contradicts_both_directions (R-04, AC-25)
- Arrange: add Contradicts edge (A↔B both directions)
- Act: `context_edge(mode: "remove", source_id=A.id, edge_type="Contradicts", target_id=B.id)`
- Assert: `(A, B, Contradicts)` ABSENT AND `(B, A, Contradicts)` ABSENT

#### test_context_edge_remove_idempotent_non_existent (AC-25)
- Arrange: no Supports edge between A and B
- Act: `context_edge(mode: "remove", source_id=A.id, edge_type="Supports", target_id=B.id)`
- Assert: success returned (0 rows affected is not an error)

### Redirect Mode (R-05, AC-26)

#### test_context_edge_redirect_non_contradicts_atomic (R-05, AC-26)
- Arrange: add `(A, B, Supports)` edge
- Act: `context_edge(mode: "redirect", source_id=A.id, edge_type="Supports", target_id=B.id, new_target_id=C.id)`
- Assert: `(A, B, Supports)` ABSENT
- Assert: `(A, C, Supports)` PRESENT
- Note: both assertions in one observation block — atomicity

#### test_context_edge_redirect_contradicts_all_four_rows (R-02, AC-26)
- Arrange: add Contradicts A↔B
- Act: redirect A's Contradicts from B to C
- Assert: `(A, B)` ABSENT, `(B, A)` ABSENT, `(A, C)` PRESENT, `(C, A)` PRESENT

#### test_context_edge_redirect_rollback_on_invalid_new_target (R-05, AC-26)
- Arrange: add `(A, B, Supports)` edge
- Act: `context_edge(mode: "redirect", ..., new_target_id=999999)` (non-existent)
- Assert: TargetNotFound error
- Assert: `(A, B, Supports)` row STILL PRESENT (original edge survives the failed redirect)
- Note: Primary R-05 rollback coverage — target validation fires before transaction,
  so original edge is never deleted. This confirms the correct ordering.

#### test_context_edge_redirect_quarantined_new_target_fails (R-05, AC-26)
- Arrange: add `(A, B, Supports)` edge; quarantine C
- Act: redirect to C
- Assert: TargetQuarantined error
- Assert: `(A, B, Supports)` still present
