# Test Plan: stale_dependency_edges (read.rs)

**Component**: `crates/unimatrix-store/src/read.rs`
**Architecture ref**: Component 5
**Risk coverage**: R-14
**AC coverage**: AC-11

---

## Unit Test Expectations

### Location: `crates/unimatrix-store/src/read.rs` (inline tests)

#### test_graph_cohesion_metrics_has_stale_dependency_field
- Assert: `GraphCohesionMetrics` struct has field `stale_dependency_edges: u64`
- This is a compile-time check; any test that constructs `GraphCohesionMetrics` validates the field

#### test_stale_dependency_edges_zero_when_no_edges
- Arrange: empty database (no GRAPH_EDGES rows)
- Act: `compute_graph_cohesion_metrics(&store)`
- Assert: `metrics.stale_dependency_edges == 0`

#### test_stale_dependency_edges_zero_when_no_deprecated_sources (R-14)
- Arrange: write a Prerequisite edge `(A, B)` where A is Active (status=0)
- Act: `compute_graph_cohesion_metrics(&store)`
- Assert: `metrics.stale_dependency_edges == 0`
- Note: confirms `status = 1` filter, not `status = 0` — the critical R-14 correctness check

#### test_stale_dependency_edges_counts_deprecated_source (R-14)
- Arrange: write a Prerequisite edge `(A, B)` where A is Deprecated (status=1)
- Act: `compute_graph_cohesion_metrics(&store)`
- Assert: `metrics.stale_dependency_edges == 1`

#### test_stale_dependency_edges_counts_multiple (R-14)
- Arrange: write 3 Prerequisite edges where all 3 sources are Deprecated
- Act: `compute_graph_cohesion_metrics(&store)`
- Assert: `metrics.stale_dependency_edges == 3`

#### test_stale_dependency_edges_prerequisite_only_not_other_types (R-14)
- Arrange: write edges:
  - `(A_deprecated, B, Prerequisite)` — A is deprecated
  - `(C_deprecated, D, Advances)` — C is deprecated, but Advances is not Prerequisite
  - `(E_deprecated, F, Supports)` — E is deprecated, but Supports is not Prerequisite
- Act: `compute_graph_cohesion_metrics(&store)`
- Assert: `metrics.stale_dependency_edges == 1` (only the Prerequisite edge counts)
- Note: confirms the query filters on `relation_type = 'Prerequisite'` correctly

#### test_stale_dependency_edges_quarantined_source_not_counted
- Arrange: write a Prerequisite edge `(A, B)` where A is Quarantined (status=2)
- Act: `compute_graph_cohesion_metrics(&store)`
- Assert: `metrics.stale_dependency_edges == 0`
- Note: only Deprecated (status=1) counts; quarantined is a different state

#### test_stale_dependency_edges_active_deprecated_mix
- Arrange:
  - `(A_active, B, Prerequisite)` — A is Active
  - `(C_deprecated, D, Prerequisite)` — C is Deprecated
- Act: `compute_graph_cohesion_metrics(&store)`
- Assert: `metrics.stale_dependency_edges == 1` (only the deprecated source edge counts)

---

## Integration Test Expectations

### Location: infra-001 `test_lifecycle.py`

#### test_stale_dependency_appears_in_context_status (AC-11)
- Arrange:
  1. Store entry A (will become source)
  2. Store entry B (will be target)
  3. Add Prerequisite edge A → B via `context_edge(mode: "add")`
  4. Deprecate A via `context_correct` (creating deprecated version)
- Act: call `context_status`
- Assert: response JSON contains `stale_dependency_edges >= 1`
- Note: this is the primary MCP-level acceptance test for AC-11

#### test_stale_dependency_not_incremented_by_active_prerequisite
- Arrange: Add a Prerequisite edge between two Active entries
- Act: call `context_status`
- Assert: `stale_dependency_edges == 0` (or unchanged from baseline)

#### test_stale_dependency_decrements_not_literal_but_tracks_state
- Note: stale_dependency_edges is a live count, not a delta. If a deprecated entry is
  not actually possible to un-deprecate (no revert operation in this system), this test
  verifies the count is computed fresh each call (not cached from previous calls).
- Arrange: Add Prerequisite edge from deprecated source; call `context_status` → count=1
- Arrange: Add another Prerequisite edge from active source; call `context_status`
- Assert: `stale_dependency_edges` is still 1 (active source doesn't increment; live count)

---

## SQL Correctness Gate

The SQL query added to `compute_graph_cohesion_metrics()` must be verified to:
1. JOIN `graph_edges` to `entries` on `e.id = ge.source_id`
2. Filter `ge.relation_type = 'Prerequisite'` as a hardcoded literal (not a format string)
3. Filter `e.status = 1` (Deprecated) — not `e.status = 0` (Active)
4. Use `COUNT(*)` returning a `u64`

Verify via grep: `grep -A 10 "stale_count\|stale_dependency" crates/unimatrix-store/src/read.rs`

The query must NOT use format string interpolation for `relation_type` or `status` values
(SQL injection guard — Security Risks section of RISK-TEST-STRATEGY.md).
