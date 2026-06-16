# Component: store-ranked-query

## Purpose

The **get-only ranked variant** of the neighbor read: canonicalize symmetric edges to one `↔`
row, `LEFT JOIN` the target entry's confidence, then `ORDER BY (source='agent') DESC,
t.confidence DESC NULLS LAST, target_id ASC LIMIT ?` with `?` bound to `GET_EDGE_DISPLAY_LIMIT`.
Returns the **≤cap displayed rows** — never the full fan-out. Lives beside, and never mutates,
`query_direct_neighbors` (ADR-001/004/006/007, C-7/C-8, FR-8/FR-9/FR-11).

## Location

`crates/unimatrix-store/src/graph_queries_ranked.rs` (new — pre-authorized OQ-B). Co-located with
`count_neighbors_split` (store-split-count) so both share the canonicalization CASE.

## Function Signature

```
async fn query_ranked_neighbors(
    pool: &SqlitePool,
    id: u64,                         // anchor; direction is implicitly Both; cap = GET_EDGE_DISPLAY_LIMIT
) -> Result<Vec<RawEdgeRow>, StoreError>
```

Returns at most `GET_EDGE_DISPLAY_LIMIT` rows. Each row: `source_id = anchor`, `target_id =
other endpoint`, `relation_type`, `source`, `target_confidence = Some|None`. A `direction` hint is
needed by the projection for `↔`/`→`/`←`; carry it as described below.

## The SQL (static; `?1` positional bind for the anchor; `?2` bind for the cap)

Build on the shared canonicalization CTE from OVERVIEW.md (`nbr` → `canon` → `deduped`), then:

```sql
WITH nbr AS (
    SELECT source_id, target_id, target_id AS other_id, relation_type, source, 'outbound' AS leg
    FROM graph_edges WHERE source_id = ?1 AND relation_type != 'Supersedes'
    UNION ALL
    SELECT source_id, target_id, source_id AS other_id, relation_type, source, 'inbound' AS leg
    FROM graph_edges WHERE target_id = ?1 AND relation_type != 'Supersedes'
),
canon AS (
    SELECT relation_type, source, other_id,
           CASE WHEN relation_type IN ('Contradicts','CoAccess','Informs')
                THEN 'both' ELSE leg END AS direction,
           MIN(?1, other_id) AS pair_lo, MAX(?1, other_id) AS pair_hi
    FROM nbr
),
deduped AS (
    SELECT relation_type, source, other_id, direction
    FROM canon
    GROUP BY relation_type, pair_lo, pair_hi,
             CASE WHEN direction = 'both' THEN 1 ELSE other_id END
)
SELECT d.relation_type, d.source, d.other_id AS target_id, d.direction,
       t.confidence       AS target_confidence
FROM deduped d
LEFT JOIN entries t ON t.id = d.other_id            -- LEFT: dangling target retained (D-02/SR-11)
ORDER BY (d.source = 'agent') DESC,                 -- 1. authored first (D-09.1)
         t.confidence DESC NULLS LAST,              -- 2. inferred by TARGET confidence (D-09.3); NULL/dangling last
         target_id ASC                              -- 3. deterministic tiebreak
LIMIT ?2                                             -- ← bound to GET_EDGE_DISPLAY_LIMIT, NEVER literal 3
```

> The `IN ('Contradicts','CoAccess','Informs')` symmetric set, the `!= 'Supersedes'` filter, the
> `ORDER BY`, and `LIMIT ?2` are static SQL — never assembled from input. `?1` (anchor) and `?2`
> (cap) are positional binds (Security: no string interpolation of ids).

## Body (pseudocode)

```
fn query_ranked_neighbors(pool, id):
    let sql = <the static statement above>                       // no literal 3
    let rows = sqlx::query(&sql)
        .bind(id as i64)                                          // ?1
        .bind(GET_EDGE_DISPLAY_LIMIT)                             // ?2  (i64 — bind, never inline)
        .fetch_all(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?            // FR-19: Result, no .unwrap()
    return rows.iter().map(map_ranked_row).collect()             // collect() propagates StoreError

fn map_ranked_row(row) -> Result<RawEdgeRow, StoreError>:
    Ok(RawEdgeRow {
        source_id:         id (the anchor)        // anchor is always source_id of the projected row
        target_id:         row.try_get::<i64,_>("target_id")? as u64
        relation_type:     row.try_get("relation_type")?
        source:            row.try_get("source")?
        target_confidence: row.try_get::<Option<f64>,_>("target_confidence")?   // None when LEFT JOIN misses
    })
    // each .try_get maps Err → StoreError::Database; no .unwrap()
```

> **Carrying `direction`**: the projection (get-edge-vocabulary) needs the SQL-computed
> `direction` (`both`/`inbound`/`outbound`). Two acceptable options — implementer picks one and the
> tester asserts it:
> (a) widen the row mapping so the ranked rows carry a `direction: &'static str` alongside
>     `RawEdgeRow` (e.g. return `Vec<(RawEdgeRow, RankedDirection)>`), OR
> (b) recompute direction in Rust from `relation_type` + which leg matched.
> **Prefer (a)** — the SQL already computed it inside `canon`, recomputing risks divergence from
> the canonicalization decision (a `↔` edge must NOT be re-derived as `→`/`←`). If (a), the anchor
> stays `source_id`; the projection reads `direction` directly. Keep the symmetric type list in ONE
> place (the SQL CASE) — do not duplicate it in Rust.

## Constraints honored

- **C-7/SR-14**: `LIMIT` + canonicalization in SQL; `fetch_all` returns ≤cap rows; the hub's full
  fan-out is never materialized. Never fetch-all-then-slice in Rust.
- **C-8**: exact locked `ORDER BY`; `graph_edges.weight` is NOT referenced.
- **C-6/ADR-007**: canonicalization runs BEFORE `ORDER BY…LIMIT`.
- **NFR-3**: `read_pool_server()` (passed in by the handler), indexed predicate
  (`idx_graph_edges_source_type` / `idx_graph_edges_target_type`, `entries.id` PK for the JOIN).
- **FR-7/D-04**: `!= 'Supersedes'` inherited in both legs.

## Data Flow

- **Inputs**: `pool` (`read_pool_server`), `id` (anchor), `GET_EDGE_DISPLAY_LIMIT`.
- **Outputs**: `Vec<RawEdgeRow>` (≤cap) with `direction` hint; consumed by get-edge-assembly for
  projection + title resolution.

## Error Handling

- Any sqlx failure ⇒ `StoreError::Database`, propagated via `?`. **No `.unwrap()`/`expect()`** (FR-19).
- Non-existent `id` ⇒ empty `Vec`, NOT an error (ADR-001).
- Dangling target ⇒ `target_confidence = None`, row **retained**, ranks last via `NULLS LAST`.

## Key Test Scenarios (discriminating, not smoke — #3886/#3645)

- **ranking-by-target-confidence, proof-outside-cap (R-02, #3886)** — seed ≥ `GET_EDGE_DISPLAY_LIMIT + 1`
  inferred edges whose target confidences straddle the cap boundary; the higher-confidence target is
  INCLUDED by the correct global rank but EXCLUDED by a batch-local/weight bug. Seed the proof target
  with a LOWER `graph_edges.weight` so weight-order and confidence-order disagree — assert weight does
  NOT decide. Per-edge rank trace in the test plan.
- **authored-priority-under-cap (R-02)** — `> cap` edges with ≥ cap authored ⇒ only authored show,
  no inferred appears.
- **inferred-fill-only-when-authored<cap (R-02)** — `< cap` authored + several inferred ⇒ inferred
  top up to exactly `cap`, ordered by target confidence.
- **deterministic tiebreak (R-06)** — equal-confidence inferred targets resolve by `target_id ASC`,
  stable across runs; cold-start uniform 0.0 ⇒ tiebreak decides.
- **LEFT-not-INNER / dangling retained (R-06, DNB-1)** — inferred edge whose `target_id` has no
  `entries` row appears in the output with `target_confidence = None`, ranks last (`NULLS LAST`).
- **symmetric collapses BEFORE cap (R-01.3)** — seed `> cap` symmetric pairs (both rows each) + one
  authored asymmetric edge; the authored edge still wins a slot (proves pairs collapsed before the
  cap consumed slots). One `↔` per pair in the result, not two.
- **high-degree-node-hits-SQL-LIMIT (R-04, SR-14)** — seed ≥ 50 edges; the function returns exactly
  `cap` rows; assert at the store boundary the full set is never materialized (returned-row count,
  not just rendered output).
- **Supersedes excluded (R-12)** — a `Supersedes` edge never appears in the result.
- **no literal 3 (AC-13a)** — SQL `LIMIT` is `?2` bound to `GET_EDGE_DISPLAY_LIMIT`; assert no
  literal `3` in the statement.
- **positional binds (security)** — anchor and cap are binds; no string-interpolated ids.
