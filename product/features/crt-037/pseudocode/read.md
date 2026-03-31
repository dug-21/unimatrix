# Component: read.rs (unimatrix-store)

## Purpose

Add `query_existing_informs_pairs()` to the `Store` (impl on `SqlxStore`). Returns the
set of `(source_id, target_id)` pairs already written with `relation_type = "Informs"` and
`bootstrap_only = 0`. Used by Phase 2 of `run_graph_inference_tick` as a pre-filter dedup
before HNSW scanning.

Mirrors `query_existing_supports_pairs` except for two differences:
1. Filters by `relation_type = 'Informs'` (not `'Supports'`).
2. Returns directional `(source_id, target_id)` without min/max normalization.

Wave 1. Single async DB read method.

## Files Modified

`crates/unimatrix-store/src/read.rs`

## New/Modified Functions

### Store::query_existing_informs_pairs

```
pub async fn query_existing_informs_pairs(&self) -> Result<HashSet<(u64, u64)>>:

    // SQL: directional — no min/max normalization (ADR-003, OQ-3 resolution)
    // Only non-bootstrap Informs rows returned (bootstrap_only = 0)
    // Reads from read_pool() — same as query_existing_supports_pairs (C-02, ADR-004)
    SQL:
        SELECT source_id, target_id
        FROM graph_edges
        WHERE relation_type = 'Informs' AND bootstrap_only = 0

    rows = sqlx::query(SQL)
        .fetch_all(self.read_pool())
        .await
        .map_err(|e| StoreError::Database(e.into()))?

    // Map rows to directional (source_id, target_id) WITHOUT min/max normalization
    // Contrast with query_existing_supports_pairs which does Ok((a.min(b), a.max(b)))
    rows.into_iter()
        .map(|row| {
            source_id = row.try_get::<i64, _>("source_id")
                .map_err(|e| StoreError::Database(e.into()))? as u64
            target_id = row.try_get::<i64, _>("target_id")
                .map_err(|e| StoreError::Database(e.into()))? as u64
            Ok((source_id, target_id))   // directional — NOT (a.min(b), a.max(b))
        })
        .collect::<Result<HashSet<(u64, u64)>>>()
```

The directional pair is the contract (ADR-003): the temporal ordering guard
(`source.created_at < target.created_at`) makes reverse-direction edges detection-impossible.
Symmetric normalization would obscure the directional contract and risk suppressing valid
edges on timestamp anomalies. `INSERT OR IGNORE` on `UNIQUE(source_id, target_id, relation_type)`
is the secondary backstop.

## Contrast with query_existing_supports_pairs

| Aspect | query_existing_supports_pairs | query_existing_informs_pairs |
|--------|-------------------------------|------------------------------|
| SQL filter | `relation_type = 'Supports'` | `relation_type = 'Informs'` |
| bootstrap_only | `= 0` | `= 0` |
| Pool | `read_pool()` | `read_pool()` |
| Tuple order | `(a.min(b), a.max(b))` | `(source_id, target_id)` — directional |
| Purpose | Symmetric dedup (S/C pairs are undirected) | Directional dedup (Informs is causal/temporal) |

The symmetric normalization in `query_existing_supports_pairs` is correct for Supports
because the NLI scan deduplicates (A,B) and (B,A) as the same pair. The directional
approach in `query_existing_informs_pairs` is correct for Informs because the temporal
guard means (A→B with A_older) and (B→A with B_older) are distinct detection outcomes —
only the former will ever pass detection.

## State Machines

None. Stateless async method on `&self`.

## Initialization Sequence

No initialization. Called on every tick in Phase 2 of `run_graph_inference_tick`. On error,
the caller degrades gracefully (logs warn, uses empty HashSet, falls back to INSERT OR IGNORE
backstop).

## Data Flow

```
GRAPH_EDGES table
  --sqlx::query (read_pool)-->
  rows: Vec<SqliteRow> {source_id: i64, target_id: i64}
  --map to (u64, u64) directional-->
  HashSet<(u64, u64)>
  --returned to Phase 2 of run_graph_inference_tick as existing_informs_pairs-->
```

## Error Handling

Returns `Err(StoreError::Database(...))` on SQL execution failure or row decoding failure.
The error type matches the existing pattern in `query_existing_supports_pairs` and all
other read methods in this file.

Caller (`run_graph_inference_tick`) handles the error via:
```
let existing_informs_pairs = match store.query_existing_informs_pairs().await:
    Ok(pairs) => pairs
    Err(e) =>
        // Degraded: empty pre-filter; INSERT OR IGNORE is backstop.
        tracing::warn!(error = %e, "graph inference tick: failed to fetch existing Informs pairs; INSERT OR IGNORE dedup")
        HashSet::new()
```

This is the same degraded pattern used for `query_existing_supports_pairs` (observed in
`nli_detection_tick.rs` lines 81–88).

## Key Test Scenarios

Mirror the existing `query_existing_supports_pairs` test structure, modified for Informs:

Empty graph: `query_existing_informs_pairs()` on a store with no graph_edges returns
`Ok(HashSet::new())`.

One non-bootstrap Informs row stored as (source=10, target=20): result contains `(10, 20)`.
Does NOT contain `(20, 10)` — directional, no symmetric expansion (this is the key
behavioral difference from the Supports version).

Bootstrap row excluded: `bootstrap_only = 1` Informs row → result is empty.

Non-Informs rows excluded: Supports and Contradicts non-bootstrap rows are not included.
Only `relation_type = 'Informs'` rows appear.

Mixed: one non-bootstrap Informs `(10, 20)`, one bootstrap Informs `(30, 40)`, one
non-bootstrap Supports `(50, 60)`. Result contains only `(10, 20)`.

Directional contract: store `(source=20, target=10)`. Assert result contains `(20, 10)`
and NOT `(10, 20)` — confirms no normalization is applied (ADR-003, distinct from
`query_existing_supports_pairs` normalization test which asserts the inverse).

In-tick dedup via Phase 4b `seen_informs_pairs` is NOT tested here — that is a
`nli_detection_tick.rs` concern. This test scope covers the DB-layer dedup only.

## Constraints

- ADR-003: No symmetric normalization. Return `(source_id, target_id)` as-is.
- C-02: Use `read_pool()` — this is a read-only query.
- NF-05: No DDL change. This query reads existing rows; the `"Informs"` string is stored
  as free-text in an unconstrained `TEXT` column.
- The function signature must match the call site in `nli_detection_tick.rs`:
  `store.query_existing_informs_pairs().await` returns `Result<HashSet<(u64, u64)>>`.
