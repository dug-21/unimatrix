# crt-037 Test Plan: read.rs (query_existing_informs_pairs)

**Component**: `crates/unimatrix-store/src/read.rs`
**Nature of change**: New `pub async fn query_existing_informs_pairs(&self) ->
Result<HashSet<(u64, u64)>>`. SQL: `SELECT source_id, target_id FROM graph_edges WHERE
relation_type = 'Informs' AND bootstrap_only = 0`.
**Risks addressed**: R-01 (write + readback), R-09 (directional dedup not normalized),
R-17 (duplicate edge prevention).

---

## Background: ADR-003 Directional Dedup

`query_existing_informs_pairs` uses directional `(source_id, target_id)` tuples — no
`(min, max)` normalization. This is intentional: the temporal ordering guard in Phase 8b
means `(target, source)` would never be written by detection. Symmetric normalization would
suppress valid new edges on re-runs if timestamp anomalies produced unexpected ordering.
`INSERT OR IGNORE` on `UNIQUE(source_id, target_id, relation_type)` is the secondary backstop.

Test R-09 scenario 2 (reverse lookup returns false) is the critical non-normalization proof.

---

## Unit Tests (Store Integration Tests — Real SQLite)

These tests use the `sqlx` test pool or an in-memory SQLite fixture from the existing
`unimatrix-store` test infrastructure. Use existing store test helpers — do not create
isolated scaffolding.

### Basic Retrieval

**Test**: `test_query_existing_informs_pairs_empty_table_returns_empty_set`
- Arrange: empty `graph_edges` table (fresh DB fixture)
- Act: `store.query_existing_informs_pairs().await`
- Assert: `Ok(HashSet::new())` — no panic, no error
  — covers ADR-003 consequence "empty table"

**Test**: `test_query_existing_informs_pairs_returns_directional_tuple`
- Arrange: insert one row: `(source_id=100, target_id=200, relation_type='Informs', bootstrap_only=0)`
- Act: `store.query_existing_informs_pairs().await`
- Assert: `set.contains(&(100u64, 200u64))` is true — covers R-09 scenario 1

**Test**: `test_query_existing_informs_pairs_does_not_normalize_reverse`
- Arrange: same row as above: `(source_id=100, target_id=200, ...)`
- Act: `store.query_existing_informs_pairs().await`
- Assert: `set.contains(&(200u64, 100u64))` is **false** — verifies non-normalization
  — covers R-09 scenario 2 (critical: ADR-003 mandates this test explicitly)

**Test**: `test_query_existing_informs_pairs_multiple_rows`
- Arrange: insert three rows:
  `(10, 20, 'Informs', 0)`, `(30, 40, 'Informs', 0)`, `(50, 60, 'Informs', 0)`
- Act: `store.query_existing_informs_pairs().await`
- Assert: set length = 3; all three tuples present

### Bootstrap Exclusion

**Test**: `test_query_existing_informs_pairs_excludes_bootstrap_only_rows`
- Arrange: insert one row with `bootstrap_only=1` and `relation_type='Informs'`
- Act: `store.query_existing_informs_pairs().await`
- Assert: set is empty — `bootstrap_only=1` rows excluded
  — covers R-09 scenario 3 and matches `query_existing_supports_pairs` semantics

**Test**: `test_query_existing_informs_pairs_includes_non_bootstrap_excludes_bootstrap`
- Arrange: two rows: `(100, 200, 'Informs', bootstrap_only=0)` and
  `(300, 400, 'Informs', bootstrap_only=1)`
- Act: `store.query_existing_informs_pairs().await`
- Assert: set contains `(100, 200)` only; length = 1

### Relation Type Isolation

**Test**: `test_query_existing_informs_pairs_excludes_other_relation_types`
- Arrange: rows with `relation_type='Supports'`, `'Contradicts'`, `'Informs'` (one each),
  all with `bootstrap_only=0`
- Act: `store.query_existing_informs_pairs().await`
- Assert: set length = 1; contains only the `Informs` pair

### R-01: Write and Readback

**Test**: `test_write_nli_edge_informs_row_is_retrievable`
- Arrange: call `write_nli_edge(store, source_id, target_id, "Informs", weight, ts, metadata)`
  (or the equivalent store write function); fresh DB
- Act: `store.query_existing_informs_pairs().await`
- Assert: set contains `(source_id, target_id)` — covers R-01 scenario 2/3

**Test**: `test_graph_edges_informs_relation_type_stored_verbatim`
- Arrange: write an `Informs` edge via `write_nli_edge`
- Act: raw SQL query: `SELECT relation_type FROM graph_edges WHERE source_id = ? AND target_id = ?`
- Assert: `relation_type == "Informs"` — string stored verbatim, no truncation, no case shift
  — covers R-01 scenario 3

### Dedup (R-17, AC-23)

**Test**: `test_query_existing_informs_pairs_dedup_prevents_duplicate_write`
- Arrange: write same `(source_id, target_id, 'Informs')` row twice via `write_nli_edge`
  (second insert should hit `INSERT OR IGNORE`)
- Act: `store.query_existing_informs_pairs().await`
- Assert: set length = 1 (not 2) — `INSERT OR IGNORE` backstop works
  — covers R-17 schema verification

---

## Acceptance Criteria Covered

| AC-ID | Test Name |
|-------|-----------|
| AC-23 (partial) | `test_query_existing_informs_pairs_dedup_prevents_duplicate_write` |
| R-01 | `test_write_nli_edge_informs_row_is_retrievable`, `test_graph_edges_informs_relation_type_stored_verbatim` |
| R-09 | `test_query_existing_informs_pairs_returns_directional_tuple`, `test_query_existing_informs_pairs_does_not_normalize_reverse` |

Full AC-23 (two-tick run) is covered by the tick integration tests in `nli_detection_tick.md`.
