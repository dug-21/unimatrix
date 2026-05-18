# Test Plan: EdgeInput / StoreParams / CorrectParams Extension

**Component**: `crates/unimatrix-server/src/mcp/tools.rs`
**Architecture ref**: Component 1
**Risk coverage**: R-08, R-09, R-12
**AC coverage**: AC-01, AC-02, AC-07, AC-08, AC-09, AC-15

---

## Unit Test Expectations

### Location: `crates/unimatrix-server/src/mcp/tools.rs` (inline tests)

#### test_edge_input_deserializes_valid_json
- Arrange: JSON `{"edge_type": "Supports", "target_id": 42}`
- Act: `serde_json::from_str::<EdgeInput>(...)`
- Assert: `edge_type == "Supports"`, `target_id == 42`

#### test_store_params_edges_field_defaults_to_none
- Arrange: JSON for StoreParams without `edges` field
- Act: `serde_json::from_str::<StoreParams>(...)`
- Assert: `params.edges.is_none()` — backward compatible (AC-01)

#### test_correct_params_edges_field_defaults_to_none
- Arrange: JSON for CorrectParams without `edges` field
- Act: `serde_json::from_str::<CorrectParams>(...)`
- Assert: `params.edges.is_none()` — backward compatible (AC-02)

#### test_store_params_accepts_edges_vec
- Arrange: StoreParams JSON with `"edges": [{"edge_type": "Supports", "target_id": 5}]`
- Act: deserialize
- Assert: `params.edges.unwrap().len() == 1`, correct field values

#### test_store_params_accepts_empty_edges_vec
- Arrange: `"edges": []`
- Act: deserialize
- Assert: `params.edges == Some(vec![])` — treated as no edges in handler, not `None`

---

## Integration Test Expectations

All integration tests run through the MCP JSON-RPC interface (`tools` suite in infra-001).

### test_store_without_edges_backward_compatible (AC-01)
- Arrange: enroll agent with Write; prepare `context_store` call without `edges` field
- Act: call `context_store`
- Assert: response matches pre-vnc-015 baseline (entry stored, no GRAPH_EDGES rows for this entry)
- Note: This is a regression guard — the omitted `edges` field must not change existing behavior

### test_store_with_edges_writes_graph_rows (AC-05)
- Arrange: store entry A; enroll agent with Write; prepare call with `edges: [{edge_type: "Supports", target_id: A.id}]`
- Act: call `context_store` → entry B
- Assert: `SELECT COUNT(*) FROM graph_edges WHERE source_id=B.id AND target_id=A.id AND relation_type='Supports'` returns 1
- Assert: `source = 'agent'`, `created_by = 'agent'` (AC-18)

### test_store_with_edges_target_not_found_fails_call (AC-07)
- Arrange: target_id referencing non-existent entry (e.g., 999999)
- Act: call `context_store` with `edges: [{edge_type: "Supports", target_id: 999999}]`
- Assert: call returns error (TargetNotFound)
- Assert: no entry written (entry count unchanged)
- Assert: no GRAPH_EDGES rows written

### test_store_with_edges_quarantined_target_fails_call (AC-07)
- Arrange: store entry Q, quarantine it (requires admin); store a new call targeting Q
- Act: call `context_store` with `edges: [{edge_type: "Supports", target_id: Q.id}]`
- Assert: call returns error (TargetQuarantined)
- Assert: no entry written; no GRAPH_EDGES rows

### test_store_with_edges_deprecated_target_succeeds (AC-07)
- Arrange: store entry D; deprecate D via `context_correct`
- Act: call `context_store` with `edges: [{edge_type: "Prerequisite", target_id: D_deprecated.id}]`
- Assert: call succeeds; entry created; GRAPH_EDGES row present

### test_store_with_edges_first_error_aborts_remaining (R-08)
- Arrange: edges slice of 3: [valid, invalid (non-existent target), valid]
- Act: call `context_store`
- Assert: call returns error referencing the second edge
- Assert: 0 GRAPH_EDGES rows written (no partial writes)
- Assert: no entry written (Phase A abort before insert)
- Note: This verifies first-error-abort, not validate-all-collect-all (R-08 coverage requirement)

### test_store_with_edges_self_referential_post_insert (R-09, AC-08)
- Arrange: determine the next auto-increment ID by inserting a probe entry and noting its ID;
  the target for the self-referential test must equal source_id, which is assigned post-insert
- Act: call `context_store` with `edges: [{edge_type: "Supports", target_id: <next_id>}]`
  where `<next_id>` is the ID the new entry will receive
- Assert: returns SelfReferential error
- Assert: no entry written (per ADR-001: self-ref check post-insert with assigned ID)
- Note: Must use actual auto-increment ID, not 0 or u64::MAX — arbitrary values trivially pass

### test_store_with_edges_duplicate_skips_edge_writes (R-12, AC-09)
- Arrange: store entry X (no edges); store again with identical content but `edges: [{Supports, Y}]`
- Act: second call
- Assert: response indicates duplicate (duplicate_of is set)
- Assert: GRAPH_EDGES row count for this source_id unchanged (no new rows for the duplicate)
- Note: Edge writes must be skipped when duplicate guard fires (duplicate_of.is_some())

### test_correct_with_edges_attaches_to_new_entry (AC-02)
- Arrange: store entry A; call `context_correct` on A with `edges: [{Supports, B.id}]`
- Act: correction completes; corrected entry C created
- Assert: GRAPH_EDGES row references `source_id = C.id`, not `A.id`
- Assert: A (deprecated) has no new outgoing edges from this call

### test_store_capability_gate_still_enforced (AC-15)
- Arrange: agent without Capability::Write
- Act: call `context_store` with or without edges
- Assert: permission error returned (backward-compatible gate enforcement)
