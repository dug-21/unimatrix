# store-analytics — Pseudocode

**Files**:
- `crates/unimatrix-store/src/analytics.rs` — `AnalyticsWrite::GraphEdge` variant + drain arm
- `crates/unimatrix-store/src/read.rs` — `GraphEdgeRow` struct + `query_graph_edges` method

---

## Purpose

Add `AnalyticsWrite::GraphEdge` variant to the analytics write pipeline (analytics.rs) and
add `GraphEdgeRow` read struct plus `Store::query_graph_edges()` to the store read layer (read.rs).
These are the two storage-layer surfaces consumed by server-state and background-tick components.

---

## analytics.rs Changes

### AnalyticsWrite::GraphEdge variant

Add to `AnalyticsWrite` enum after the existing `DeleteObservationPhases` variant:

```
/// Table: `graph_edges` — idempotent INSERT OR IGNORE via UNIQUE(source_id, target_id, relation_type).
///
/// SHEDDING POLICY: Shed-safe for bootstrap-origin writes only. W1-2 NLI confirmed
/// edge writes MUST NOT use this variant — use direct write_pool path instead.
/// (ARCHITECTURE §2c SR-02, ADR-001 Consequences)
///
/// weight must be finite (not NaN, not ±Inf). The drain task validates weight.is_finite()
/// and drops the event with ERROR log if the check fails. (FR-12, AC-17)
GraphEdge {
    source_id:      u64,
    target_id:      u64,
    relation_type:  String,    // RelationType::as_str() value
    weight:         f32,       // validated finite by caller before enqueue
    created_by:     String,
    source:         String,
    bootstrap_only: bool,
}
```

Note: `created_at` is NOT in the variant fields. The drain task supplies `created_at` at
execution time using `current_unix_seconds()` — the same pattern used by the CoAccess drain arm.
The `metadata` column in GRAPH_EDGES is not in the variant; it receives NULL from the INSERT.

### variant_name() update

```
ADD to match arms in variant_name():
    AnalyticsWrite::GraphEdge { .. } => "GraphEdge",
```

Place before the catch-all `_ => "Unknown"` arm. Alphabetical or declaration order — match
the existing convention in the file.

### Drain arm in execute_analytics_write

Add to the match in `execute_analytics_write`:

```
AnalyticsWrite::GraphEdge {
    source_id,
    target_id,
    relation_type,
    weight,
    created_by,
    source,
    bootstrap_only,
} =>
    -- Weight finite validation guard (NF-01, AC-17, R-07)
    IF NOT weight.is_finite():
        tracing::error!(
            source_id = source_id,
            target_id = target_id,
            relation_type = relation_type,
            weight = weight,
            "analytics drain: GraphEdge weight is not finite (NaN/Inf); event dropped"
        )
        RETURN Ok(())   -- drop the event; do not write; do not retry

    LET now = current_unix_seconds()
    LET bootstrap_only_int: i64 = IF bootstrap_only { 1 } ELSE { 0 }

    sqlx::query(
        "INSERT OR IGNORE INTO graph_edges
             (source_id, target_id, relation_type, weight, created_at,
              created_by, source, bootstrap_only)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
    )
    .bind(source_id as i64)
    .bind(target_id as i64)
    .bind(relation_type)
    .bind(weight)
    .bind(now)
    .bind(created_by)
    .bind(source)
    .bind(bootstrap_only_int)
    .execute(&mut **txn)
    .await?
```

Notes on binding conventions (match existing codebase patterns):
- `u64` fields (`source_id`, `target_id`) are bound as `i64` — SQLite stores integers as i64.
- `bool` (`bootstrap_only`) is cast to `i64` (0/1) before binding.
- `weight: f32` is bound directly — sqlx handles f32 → REAL conversion.
- `metadata` column is omitted from the INSERT column list; SQLite uses DEFAULT NULL.
- `INSERT OR IGNORE` exploits `UNIQUE(source_id, target_id, relation_type)` for idempotency.

---

## read.rs Changes

### GraphEdgeRow struct

Add to `read.rs` as a public struct. This struct is exported from `unimatrix-store` and
imported by `unimatrix-engine` in `graph.rs` and by `unimatrix-server` in `typed_graph.rs`.

```
/// One row from the `graph_edges` table.
///
/// Used by the background tick to load all edges and pass them to
/// `build_typed_relation_graph`. The `metadata` column is not included —
/// it is NULL for all crt-021 writes and reserved for W3-1 GNN use.
pub struct GraphEdgeRow {
    pub source_id:      u64,
    pub target_id:      u64,
    pub relation_type:  String,
    pub weight:         f32,
    pub created_at:     i64,
    pub created_by:     String,
    pub source:         String,
    pub bootstrap_only: bool,
}
```

### query_graph_edges on SqlxStore

```
FUNCTION query_graph_edges(&self) -> Result<Vec<GraphEdgeRow>, StoreError>:

    LET rows = sqlx::query(
        "SELECT source_id, target_id, relation_type, weight, created_at,
                created_by, source, bootstrap_only
         FROM graph_edges"
    )
    .fetch_all(&self.read_pool)
    .await
    .map_err(|e| StoreError::Database(e.into()))?

    LET result: Vec<GraphEdgeRow> = rows
        .into_iter()
        .map(|row| GraphEdgeRow {
            source_id:      row.try_get::<i64, _>("source_id")? as u64,
            target_id:      row.try_get::<i64, _>("target_id")? as u64,
            relation_type:  row.try_get("relation_type")?,
            weight:         row.try_get::<f32, _>("weight")?,
            created_at:     row.try_get::<i64, _>("created_at")?,
            created_by:     row.try_get("created_by")?,
            source:         row.try_get("source")?,
            bootstrap_only: row.try_get::<i64, _>("bootstrap_only")? != 0,
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| StoreError::Database(e.into()))?

    RETURN Ok(result)
```

Implementation notes:
- Uses `self.read_pool` (not write_pool) — this is a SELECT.
- `bootstrap_only` is stored as INTEGER 0/1; mapped to `bool` via `!= 0`.
- `source_id` and `target_id` are stored as i64 in SQLite; cast to u64.
- `metadata` is intentionally not selected — column is NULL for all crt-021 rows
  and `GraphEdgeRow` has no metadata field. W3-1 adds metadata to GraphEdgeRow
  in a future feature.
- No ORDER BY required; the caller (`build_typed_relation_graph`) is order-independent.
- Error mapping follows the `entry_from_row` pattern in read.rs: wrap sqlx errors in
  `StoreError::Database`.

### Store trait exposure

`query_graph_edges` is an `async fn` on `SqlxStore`. It must be added to the `Store` trait
in `unimatrix-core` if `Store::query_all_entries` is defined there (check the trait boundary).
If `Store` is a trait in `unimatrix-core/src/traits.rs`, add:

```
async fn query_graph_edges(&self) -> Result<Vec<GraphEdgeRow>, StoreError>;
```

If `query_all_entries` is implemented directly on `SqlxStore` without a trait boundary,
then `query_graph_edges` is added to `SqlxStore` in the same pattern.

FLAG: The implementer must check whether `Store::query_all_entries` is a trait method or
a direct `SqlxStore` impl method and follow the same pattern for `query_graph_edges`.

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `weight.is_finite()` fails in drain arm | Log ERROR, return Ok(()); event dropped, not retried |
| `query_graph_edges` store error | Return `Err(StoreError::Database(...))` |
| Bind type cast overflow (u64 > i64::MAX) | Not expected in practice; entry IDs are auto-incremented from 1 |

---

## Key Test Scenarios

1. **variant_name returns "GraphEdge"** (AC-09):
   - Construct `AnalyticsWrite::GraphEdge { .. }` with valid fields.
   - Call `variant_name()`. Assert returns `"GraphEdge"`.

2. **Drain arm: valid GraphEdge writes to graph_edges** (AC-09):
   - Open an in-memory SQLite database with graph_edges table.
   - Enqueue `AnalyticsWrite::GraphEdge { source_id: 1, target_id: 2, relation_type: "Supersedes", weight: 1.0, created_by: "test", source: "test", bootstrap_only: false }`.
   - Drain.
   - Query `graph_edges`. Assert one row with correct values.
   - Assert `bootstrap_only = 0` (was false → cast to 0).

3. **Drain arm: NaN weight rejected** (AC-17, R-07):
   - Enqueue `AnalyticsWrite::GraphEdge { weight: f32::NAN, ... }`.
   - Drain.
   - Query `graph_edges`. Assert zero rows written.

4. **Drain arm: Inf weight rejected** (AC-17):
   - Enqueue `AnalyticsWrite::GraphEdge { weight: f32::INFINITY, ... }`.
   - Drain. Assert zero rows.

5. **Drain arm: idempotent insert (INSERT OR IGNORE)** (FR-12, R-08):
   - Enqueue same GraphEdge twice with identical (source_id, target_id, relation_type).
   - Drain both.
   - Assert one row in graph_edges (not two).

6. **query_graph_edges returns rows** (integration):
   - Insert two rows directly into graph_edges via sqlx.
   - Call `store.query_graph_edges()`.
   - Assert two `GraphEdgeRow` structs returned with correct field values.
   - Assert `bootstrap_only: bool` correctly mapped from INTEGER.

7. **query_graph_edges returns empty on empty table** (integration):
   - Call on database with zero graph_edges rows.
   - Assert `Ok(vec![])` returned.
