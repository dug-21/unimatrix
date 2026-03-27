# Test Plan: Store Query Helpers (crt-029)

Source file: `crates/unimatrix-store/src/read.rs`
Pseudocode: `pseudocode/store-query-helpers.md`

Functions covered:
- `Store::query_entries_without_edges() -> Result<Vec<u64>>`
- `Store::query_existing_supports_pairs() -> Result<HashSet<(u64, u64)>>`

Risks addressed: R-04 (pre-filter scan), R-06 (pool choice)

---

## Design Notes

Both methods are async and use `read_pool()`. Tests require an in-memory SQLite database (or
test fixture) seeded with `entries` and `graph_edges` rows. The existing `unimatrix-store`
test infrastructure uses `Store::open_in_memory()` or equivalent — follow that pattern rather
than creating isolated scaffolding.

Both functions must be independently testable without any NLI model or server binary.

---

## Unit Test Expectations — `query_entries_without_edges`

Tests in `unimatrix-store/src/read.rs` `#[cfg(test)]` or `unimatrix-store/tests/`.

The SQL contract:
```sql
SELECT id FROM entries WHERE status = 0
AND id NOT IN (
  SELECT source_id FROM graph_edges WHERE bootstrap_only = 0
  UNION
  SELECT target_id FROM graph_edges WHERE bootstrap_only = 0
)
```

### AC-15 — Core correctness

#### `test_query_entries_without_edges_empty_store`
- Seed: empty `entries` table
- Call `query_entries_without_edges()`
- Assert returns empty `Vec`
- Edge case: no panic on empty table (verifies the NOT IN subquery handles 0 rows)

#### `test_query_entries_without_edges_no_edges`
- Seed: 3 active entries (status = 0), no rows in `graph_edges`
- Call `query_entries_without_edges()`
- Assert returned `Vec` contains all 3 entry IDs (all are isolated)

#### `test_query_entries_without_edges_with_edges`
- Seed: 4 active entries (IDs 1–4), edges: `(1→2, bootstrap_only=0)` and `(3→4, bootstrap_only=0)`
- Call `query_entries_without_edges()`
- Assert returned `Vec` is **empty** — IDs 1, 2, 3, 4 all appear in either source_id or target_id

#### `test_query_entries_without_edges_partial_coverage`
- Seed: 5 active entries (IDs 1–5), edge: only `(1→2, bootstrap_only=0)`
- Call `query_entries_without_edges()`
- Assert returned `Vec` contains IDs 3, 4, 5 (isolated)
- Assert IDs 1 and 2 are NOT returned (they have edges)

#### `test_query_entries_without_edges_bootstrap_only_ignored`
- Seed: 3 active entries (IDs 1–3), edges: `(1→2, bootstrap_only=1)` and `(2→3, bootstrap_only=1)`
- Call `query_entries_without_edges()`
- Assert returned `Vec` contains all 3 IDs (bootstrap-only edges do not count; entries are still "isolated" for tick purposes)
- This is the critical test distinguishing bootstrap from non-bootstrap edges

#### `test_query_entries_without_edges_inactive_excluded`
- Seed: 2 active entries (IDs 1, 2), 1 deprecated entry (ID 3, status != 0), no edges
- Call `query_entries_without_edges()`
- Assert returned `Vec` contains only IDs 1 and 2 (deprecated entry excluded by `WHERE status = 0`)

---

## Unit Test Expectations — `query_existing_supports_pairs`

### AC-15 (supporting function for pre-filter)

#### `test_query_existing_supports_pairs_empty`
- Seed: empty `graph_edges` table
- Call `query_existing_supports_pairs()`
- Assert returns empty `HashSet`

#### `test_query_existing_supports_pairs_supports_only`
- Seed: `graph_edges` with one non-bootstrap Supports row: `(source_id=10, target_id=20, relation_type='Supports', bootstrap_only=0)`
- Call `query_existing_supports_pairs()`
- Assert returned `HashSet` contains exactly one pair
- Assert the pair is normalized: `(min(10,20), max(10,20))` = `(10, 20)`

#### `test_query_existing_supports_pairs_bootstrap_excluded`
- Seed: `graph_edges` with only bootstrap Supports rows (`bootstrap_only=1`)
- Call `query_existing_supports_pairs()`
- Assert returns empty `HashSet` (bootstrap rows excluded from pre-filter by `WHERE bootstrap_only = 0`)

#### `test_query_existing_supports_pairs_excludes_contradicts`
- Seed: `graph_edges` with:
  - `(source_id=1, target_id=2, relation_type='Contradicts', bootstrap_only=0)`
  - `(source_id=3, target_id=4, relation_type='Supports', bootstrap_only=1)`
  - `(source_id=5, target_id=6, relation_type='Supports', bootstrap_only=0)`
- Call `query_existing_supports_pairs()`
- Assert returned `HashSet` contains exactly one pair: `(5, 6)`
- Assert pairs `(1, 2)` and `(3, 4)` are absent

#### `test_query_existing_supports_pairs_mixed_bootstrap`
- Seed: `graph_edges` with:
  - `(source_id=1, target_id=2, relation_type='Supports', bootstrap_only=0)` — non-bootstrap
  - `(source_id=1, target_id=3, relation_type='Supports', bootstrap_only=1)` — bootstrap
  - `(source_id=4, target_id=5, relation_type='Supports', bootstrap_only=0)` — non-bootstrap
- Call `query_existing_supports_pairs()`
- Assert returned `HashSet` contains exactly two pairs: `(1, 2)` and `(4, 5)`
- Assert `(1, 3)` is absent (bootstrap_only=1 excluded)

#### `test_query_existing_supports_pairs_normalization`
- Seed: `graph_edges` with `(source_id=30, target_id=10, relation_type='Supports', bootstrap_only=0)`
  (higher source_id, lower target_id — reversal from normalized form)
- Call `query_existing_supports_pairs()`
- Assert returned `HashSet` contains `(10, 30)` (normalized as `(min, max)`)
- Assert the raw `(30, 10)` representation is NOT present (normalization applied at read time
  or in the tick's pre-filter logic — the test must verify whichever approach is chosen)

---

## Pool Choice Verification (R-06 / C-12)

This is a code-level check, not a unit test.

```bash
grep -n 'read_pool\|write_pool' crates/unimatrix-store/src/read.rs
```

Both `query_entries_without_edges` and `query_existing_supports_pairs` must use `read_pool()`.
Using `write_pool_server()` in a read-only query creates contention with tick writes (the tick
uses the write pool for `INSERT` in Phase 8).

The architectural conflict between Unimatrix entries #3593 (write-pool) and #3595 (read-pool)
for `compute_graph_cohesion_metrics` must also be verified here. Entry #3619 confirms
`read_pool()` is correct; entry #3593 must be deprecated before delivery.

---

## Integration Harness

No new integration tests are planned for the store helpers themselves. Their effects are
observable indirectly through the lifecycle integration tests described in OVERVIEW.md:
- `test_graph_inference_tick_writes_supports_edges` — exercises `query_existing_supports_pairs`
  indirectly by verifying the pre-filter allows new edges to be written
- `test_graph_inference_tick_no_contradicts_edges` — exercises `query_entries_without_edges`
  indirectly via the tick's Phase 2 data fetch

Direct DB-level assertions in integration tests (reading GRAPH_EDGES rows) would bypass the
MCP protocol and are outside the scope of infra-001 suites.

---

## Assertions Summary

| AC-ID | Test Name | Expected Result |
|-------|-----------|-----------------|
| AC-15 | `test_query_entries_without_edges_empty_store` | Empty Vec |
| AC-15 | `test_query_entries_without_edges_no_edges` | All 3 IDs returned |
| AC-15 | `test_query_entries_without_edges_with_edges` | Empty Vec (all IDs have edges) |
| AC-15 | `test_query_entries_without_edges_partial_coverage` | Only edge-free IDs returned |
| AC-15 | `test_query_entries_without_edges_bootstrap_only_ignored` | All 3 IDs returned (bootstrap edges don't count) |
| AC-15 | `test_query_entries_without_edges_inactive_excluded` | Only active entry IDs returned |
| (R-04) | `test_query_existing_supports_pairs_empty` | Empty HashSet |
| (R-04) | `test_query_existing_supports_pairs_supports_only` | 1 pair: (10, 20) |
| (R-04) | `test_query_existing_supports_pairs_bootstrap_excluded` | Empty HashSet |
| (R-04) | `test_query_existing_supports_pairs_excludes_contradicts` | Only (5, 6) |
| (R-04) | `test_query_existing_supports_pairs_mixed_bootstrap` | (1,2) and (4,5) only |
| (R-04) | `test_query_existing_supports_pairs_normalization` | (10, 30) present, (30, 10) absent |
| R-06/C-12 | grep gate (shell) | Both helpers use `read_pool()` |
