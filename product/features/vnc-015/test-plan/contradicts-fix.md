# Test Plan: query_contradicts_edges_for_entry Fix

**Component**: `crates/unimatrix-store/src/read.rs`
**Architecture ref**: Component 7
**Risk coverage**: R-07 (High)
**AC coverage**: AC-16

---

## Pre-Implementation Requirement

Before Stage 3b writes any code, the following call-site audit must be completed:

```bash
grep -rn "query_contradicts_edges_for_entry" crates/
```

Expected callers (from ARCHITECTURE.md):
- `suppress_contradicts` in `crates/unimatrix-engine/` or `crates/unimatrix-server/`
- Any test helpers that call the function directly

For each caller: document the expected behavior change. If a caller processes the result
as a scalar (`.first()`, single-row expectation) it will break after the OR-clause fix.
This audit result must be documented in the RISK-COVERAGE-REPORT.md under R-07.

---

## Unit Test Expectations

### Location: `crates/unimatrix-store/src/read.rs` (inline tests)

#### test_query_contradicts_returns_source_direction (AC-16)
- Arrange: insert a GRAPH_EDGES row `(source_id=A, target_id=B, relation_type='Contradicts')`
  (unidirectional — simulating pre-vnc-015 NLI-written data)
- Act: `query_contradicts_edges_for_entry(&store, A).await`
- Assert: returns 1 row (the A→B direction is returned for A as source)

#### test_query_contradicts_returns_target_direction (AC-16, R-07 transition compatibility)
- Arrange: same unidirectional row `(source_id=A, target_id=B, Contradicts)` — NO B→A row
  (pre-vnc-015 unidirectional data in the transition period)
- Act: `query_contradicts_edges_for_entry(&store, B).await`
- Assert: returns 1 row (the A→B direction is returned for B as target via the OR clause)
- Note: This is the key transition-period compatibility test. Pre-vnc-015, only A→B exists.
  The OR-clause fix must return this row when querying from B's perspective.
  Without the fix (`WHERE target_id = ?1` only), querying from A returns nothing — wrong.

#### test_query_contradicts_bidirectional_post_vnc015 (AC-16, R-07)
- Arrange: insert both directions `(A, B, Contradicts)` AND `(B, A, Contradicts)`
  (post-vnc-015 write path creates both)
- Act: `query_contradicts_edges_for_entry(&store, A).await`
- Assert: returns 2 rows (both the A→B and B→A direction rows)

#### test_query_contradicts_both_endpoints_return_same_rows (R-07)
- Arrange: insert both directions `(A, B, Contradicts)` AND `(B, A, Contradicts)`
- Act: query from A AND query from B
- Assert: each returns 2 rows
- Assert: the row sets are equivalent (same edge, both directions visible from either endpoint)

#### test_query_contradicts_only_contradicts_relation_type
- Arrange: insert rows `(A, B, Contradicts)` AND `(A, B, Supports)` AND `(B, A, Supports)`
- Act: `query_contradicts_edges_for_entry(&store, A).await`
- Assert: returns exactly 1 row (only the Contradicts row; Supports filtered out)
- Note: confirms `AND relation_type = 'Contradicts'` filter is preserved in the OR fix

#### test_query_contradicts_no_results_for_unrelated_entry
- Arrange: insert `(A, B, Contradicts)` row
- Act: `query_contradicts_edges_for_entry(&store, C).await` where C is neither A nor B
- Assert: returns 0 rows

---

## Caller Behavior Regression Tests

### Location: wherever suppress_contradicts is implemented

#### test_suppress_contradicts_behavior_unchanged_after_query_fix (R-07)
- Arrange: write a Contradicts edge via the pre-vnc-015 write path
  (unidirectional: insert `(A, B, Contradicts)` only)
- Act: call `suppress_contradicts(...)` or whatever the upstream caller does
- Assert: suppression applies correctly — same behavior as before the fix
- Note: This test must be written for the actual caller behavior, not just the raw function.
  If suppress_contradicts now receives 1 row instead of 0 (because the OR clause picks up
  the source direction it previously missed), this is a behavior change that must be tested.

#### test_suppress_contradicts_works_with_bidirectional_edges (R-07)
- Arrange: write bidirectional Contradicts edge `(A↔B)` via vnc-015 write path
- Act: call suppression logic
- Assert: suppression applies correctly with 2 rows returned (not double-suppressed or confused)

---

## Integration Test Expectations

### Location: infra-001 `test_contradiction.py`

These tests run against the full MCP interface to verify end-to-end bidirectionality.

#### test_contradicts_query_bidirectional_via_search (AC-16)
- Arrange: store entries A and B; add Contradicts edge via `context_edge(mode: "add")`
- Act: call `context_search` or `context_briefing` with a query that should surface A
- Assert: contradiction suppression applies (entry B, which contradicts A, is penalized in results)
- Assert: the reverse also holds (search for B, A is penalized)
- Note: this tests the full pipeline from write through query to suppression behavior

#### test_existing_nli_contradicts_handled_post_fix (R-07 transition compatibility)
- Arrange: server has pre-existing unidirectional Contradicts rows (from NLI detection)
- Act: run contradiction detection or search
- Assert: no regression in contradiction suppression behavior for pre-existing rows
- Note: This may require a fixture with pre-loaded unidirectional data to simulate the
  transition period. If infra-001 does not support this fixture, document as a gap.

---

## Code Review Gate

Verify the SQL query in `query_contradicts_edges_for_entry` after Stage 3b:

```bash
grep -A 5 "query_contradicts_edges_for_entry" crates/unimatrix-store/src/read.rs
```

Expected SQL form:
```sql
WHERE (source_id = ?1 OR target_id = ?1) AND relation_type = 'Contradicts'
```

Must NOT be:
```sql
WHERE target_id = ?1 AND relation_type = 'Contradicts'     -- old unidirectional (wrong)
WHERE source_id = ?1 AND relation_type = 'Contradicts'     -- only source direction (incomplete)
```
