# crt-037 Test Plan: graph.rs (RelationType Extension)

**Component**: `crates/unimatrix-engine/src/graph.rs`
**Nature of change**: Additive — new `RelationType::Informs` variant; `as_str()` and
`from_str()` each gain one arm; module doc comment updated.
**Risks addressed**: R-01 (write succeeds), R-10 (penalty isolation), R-16 (regression).

---

## Unit Tests

### String Conversion Round-Trip

**Test**: `test_relation_type_informs_from_str_returns_some`
- Arrange: nothing
- Act: `RelationType::from_str("Informs")`
- Assert: equals `Some(RelationType::Informs)` — covers AC-01

**Test**: `test_relation_type_informs_as_str_returns_string`
- Arrange: nothing
- Act: `RelationType::Informs.as_str()`
- Assert: equals `"Informs"` — covers AC-02

**Test**: `test_relation_type_from_str_case_sensitive`
- Arrange: nothing
- Act: `RelationType::from_str("informs")`, `RelationType::from_str("INFORMS")`
- Assert: both return `None` — verifies case-sensitivity documented in IMPLEMENTATION-BRIEF.md

**Test**: `test_relation_type_informs_round_trip`
- Arrange: nothing
- Act: `RelationType::from_str(RelationType::Informs.as_str())`
- Assert: equals `Some(RelationType::Informs)` — string value matches variant name exactly

### Graph Construction with Informs Edges

**Test**: `test_build_typed_relation_graph_includes_informs_edge`
- Arrange: one `GraphEdgeRow` with `relation_type = "Informs"`, valid source/target IDs
- Act: `build_typed_relation_graph(&[row])`
- Assert: output graph contains exactly one edge; edge type is `RelationType::Informs`
  — covers AC-03

**Test**: `test_build_typed_relation_graph_informs_no_warn_log`
- Arrange: one `GraphEdgeRow` with `relation_type = "Informs"`; capture log output with
  `tracing_test` or equivalent
- Act: `build_typed_relation_graph(&[row])`
- Assert: zero `WARN`-level log entries mentioning `"Informs"` — covers AC-04
- Note: this verifies the "unknown relation type" warn branch does not fire for `Informs`

### Penalty Isolation (SR-01 / R-10)

**Test**: `test_graph_penalty_with_informs_only_returns_fallback`
- Arrange: `TypedRelationGraph` with two nodes and a single `Informs` edge A→B
- Act: `graph_penalty(graph, node_A)` (or equivalent penalty call on node A)
- Assert: return value equals `FALLBACK_PENALTY` constant — `Informs` contributes nothing
  — covers AC-24

**Test**: `test_find_terminal_active_with_informs_only_returns_empty`
- Arrange: same two-node `Informs`-only graph as above
- Act: `find_terminal_active(graph, node_A)` (or equivalent)
- Assert: result is empty — no `Informs` node is treated as "terminal"
  — covers AC-24 (second assertion)

**Test**: `test_graph_penalty_informs_plus_supersedes_uses_supersedes_only`
- Arrange: three-node graph: node A has one `Supersedes` edge to B and one `Informs` edge to C
- Act: `graph_penalty(graph, node_A)`
- Assert: penalty is derived only from the `Supersedes` edge (non-fallback value);
  the `Informs` edge does not alter the penalty

### Regression Safety

**Test**: `test_existing_relation_type_variants_unchanged`
- Arrange: iterate existing variants: `Supersedes`, `Contradicts`, `Supports`, `CoAccess`,
  `Prerequisite`
- Act: `as_str()` + `from_str()` round-trip for each
- Assert: all round-trips match pre-existing expected strings — adding `Informs` did not
  shift existing arm mappings

---

## Integration Notes

- These tests use no I/O. They run in `cargo test` without special fixtures.
- `tracing_test` crate (if available) is the preferred way to capture log output for
  AC-04. If not available, assert by examining the function's behavior: if `build_typed_relation_graph` does not warn, the AC-04 condition is met structurally.
- Tests for `graph_penalty` and `find_terminal_active` must use a graph containing *only*
  `Informs` edges — not a mixed graph — so that a passing test cannot mask traversal of
  `Informs` (R-10 coverage requirement from RISK-TEST-STRATEGY.md).

---

## Acceptance Criteria Covered

| AC-ID | Test Name |
|-------|-----------|
| AC-01 | `test_relation_type_informs_from_str_returns_some` |
| AC-02 | `test_relation_type_informs_as_str_returns_string` |
| AC-03 | `test_build_typed_relation_graph_includes_informs_edge` |
| AC-04 | `test_build_typed_relation_graph_informs_no_warn_log` |
| AC-24 | `test_graph_penalty_with_informs_only_returns_fallback`, `test_find_terminal_active_with_informs_only_returns_empty` |
