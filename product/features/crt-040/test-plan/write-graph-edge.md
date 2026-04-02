# Test Plan: write-graph-edge (Wave 2)

**File modified:** `crates/unimatrix-server/src/services/nli_detection.rs`

**Change:** Add `pub(crate) async fn write_graph_edge(...)` as a sibling to `write_nli_edge`.
`write_nli_edge` is NOT modified.

**Risk coverage:** R-02 (High — primary), R-07 (Medium — false return semantics)

---

## Unit Test Expectations

All tests live in `#[cfg(test)] mod tests` inside `nli_detection.rs`. Tests that write to
`graph_edges` use an in-process SQLite store (same pattern as existing edge write tests in
the codebase — use `Store::open_in_memory()` or the existing `create_test_store()` helper).

### TC-01: write_graph_edge writes source='cosine_supports' (AC-11, R-02)

```
async fn test_write_graph_edge_writes_cosine_supports_source()
```

- Arrange: in-memory store, source_id=1, target_id=2, relation_type="Supports",
  weight=0.70, source=EDGE_SOURCE_COSINE_SUPPORTS
- Act: `let wrote = write_graph_edge(&store, 1, 2, "Supports", 0.70, ts, "cosine_supports", r#"{"cosine":0.70}"#).await`
- Assert:
  - `wrote == true`
  - Query `graph_edges` row: `source == "cosine_supports"`
  - Query `graph_edges` row: `created_by == "cosine_supports"`
  - Query `graph_edges` row: `relation_type == "Supports"`
  - Query `graph_edges` row: `weight ≈ 0.70` (f32 within 1e-5)
- Covers: AC-11 (R-02 write_graph_edge path)

### TC-02: write_nli_edge still writes source='nli' after change (R-02 — mandatory regression)

```
async fn test_write_nli_edge_still_writes_nli_source()
```

- Arrange: in-memory store, source_id=10, target_id=20, relation_type="Supports",
  weight=0.85
- Act: `let wrote = write_nli_edge(&store, 10, 20, "Supports", 0.85, ts, r#"{"nli_entailment":0.85}"#).await`
- Assert:
  - `wrote == true`
  - Query `graph_edges` row: `source == "nli"`
  - Query `graph_edges` row: `created_by == "nli"`
- Covers: R-02 (write_nli_edge immutability — compiler alone is insufficient)
- CRITICAL: This test must exist. A refactor that accidentally passes `"cosine_supports"` to
  a shared helper would compile but fail only here.

### TC-03: write_graph_edge and write_nli_edge produce different source values for same pair

```
async fn test_write_graph_edge_and_write_nli_edge_distinct_sources()
```

- Arrange: in-memory store with two different pair IDs
- Act: write one edge via `write_nli_edge`, one via `write_graph_edge`
- Assert: two rows in `graph_edges`; sources are `"nli"` and `"cosine_supports"` respectively
- Covers: R-02 (isolation test — both functions operate on distinct tuples)

### TC-04: Second write_graph_edge call for same pair returns false (INSERT OR IGNORE)

```
async fn test_write_graph_edge_duplicate_returns_false_no_warn()
```

- Arrange: write `(1, 2, "Supports")` once via `write_graph_edge` (first call returns `true`)
- Act: call `write_graph_edge` again for the same `(1, 2, "Supports")` triple
- Assert:
  - Second call returns `false` (INSERT OR IGNORE: `rows_affected = 0` → `rows_affected() > 0` is `false`)
  - No panic
  - `graph_edges` still has exactly ONE row for `(1, 2, "Supports")` (row count = 1)
- Covers: R-07 (UNIQUE conflict returns `false` via `rows_affected() > 0`; this is a silent
  no-op, not an error — the `Ok` arm returns `rows_affected() > 0`, so both a genuine insert
  (`rows_affected = 1` → `true`) and a UNIQUE conflict dedup (`rows_affected = 0` → `false`)
  are handled here without `warn!`)
- Note: `warn!` must NOT fire on the second call. Verifying the absence of warn requires
  either a `tracing_subscriber::testing` setup or code inspection. Specify as a code review
  gate if tracing testing infrastructure is not available.
- IMPORTANT: This assertion (`false` on UNIQUE conflict) is authoritative. The `Ok` arm of
  `write_graph_edge` returns `rows_affected() > 0`, which is `false` when INSERT OR IGNORE
  silently discards the row. Budget counters in Path C rely on this: only `true` returns
  increment the budget counter.

### TC-05: write_graph_edge returns false on SQL error (not panic)

```
async fn test_write_graph_edge_sql_error_returns_false()
```

- Arrange: use a store backed by a closed or read-only pool, or simulate a SQL error by
  passing an invalid table state
- Act: call `write_graph_edge`
- Assert: returns `false`, does not panic, does not propagate error
- Covers: failure mode from RISK-TEST-STRATEGY Failure Modes table
- Implementation note: if a SQL error is hard to inject directly, this TC can be specified
  as a code inspection gate — verify the `Err(e)` arm logs at `warn!` and returns `false`.

### TC-06: write_graph_edge metadata written correctly

```
async fn test_write_graph_edge_metadata_format()
```

- Arrange: call with `metadata = r#"{"cosine":0.71}"#`
- Assert: `graph_edges.metadata` column contains `{"cosine":0.71}` (exact string)
- Covers: FR-10 (edge metadata format)

### TC-07: write_graph_edge Informs edge (correct relation_type)

```
async fn test_write_graph_edge_informs_relation_type()
```

- Arrange: call with `relation_type = "Informs"`, source = `"cosine_supports"` (hypothetical
  future caller scenario — tests that the function is truly generic)
- Assert: `graph_edges.relation_type == "Informs"`
- Covers: FR-06 (write_graph_edge is a general-purpose helper, not Supports-only)

---

## Integration Test Expectations

No new infra-001 integration tests are needed for `write_graph_edge` in isolation. The
function's MCP-visible output (graph edge presence in `graph_edges`) is captured by the
lifecycle integration tests planned in OVERVIEW.md.

Regression guard: the existing `tools` and `lifecycle` suites exercise Path A (which calls
`write_nli_edge`). If `write_nli_edge` is accidentally modified, those existing tests will
catch it via `inferred_edge_count` or edge-source queries.

---

## Concurrency Note

`write_graph_edge` and `write_nli_edge` may both be called in the same tick when
`nli_enabled=true`. The `UNIQUE(source_id, target_id, relation_type)` constraint is the
authoritative dedup backstop. The unit tests above (TC-04) verify the false-return behavior
for the same triple, but they do not test concurrent execution. Concurrent behavior is
covered by the `edge_cases` suite in infra-001 (`concurrent ops` category) and is not a new
risk introduced by this feature.

---

## Edge Cases

| Edge Case | Expectation |
|-----------|-------------|
| `source` parameter is empty string | Write proceeds; no validation in write_graph_edge itself — validation is the caller's responsibility |
| `weight` is a valid f32 outside [0,1] | Write proceeds; no range check in write_graph_edge |
| Same `(source_id, target_id, relation_type)` attempted by both Path B and Path C in same tick | Second INSERT OR IGNORE returns false; no warn; one row in DB. Covered by TC-04. |
| `write_nli_edge` called after write_graph_edge for same pair | Same INSERT OR IGNORE behavior; returns false; row retains first writer's `source` value |

---

## Assertions Summary

| Risk/AC | Test | Assertion |
|---------|------|-----------|
| R-02 | TC-02 | `write_nli_edge` writes `source='nli'` (unchanged) |
| R-02 | TC-01 | `write_graph_edge` writes `source='cosine_supports'` |
| R-07 | TC-04 | Second call returns `false`, no panic, no warn, row count = 1 |
| AC-11 | TC-01 | `graph_edges.source == "cosine_supports"` and `created_by == "cosine_supports"` |
| FR-06 | TC-05 | SQL error returns `false`, not panic |
| FR-10 | TC-06 | Metadata format `{"cosine": f32}` written correctly |
