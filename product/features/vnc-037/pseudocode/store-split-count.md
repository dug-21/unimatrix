# Component: store-split-count

## Purpose

Compute the **honest, uncapped** inbound/outbound edge totals via a separate `COUNT(*)`-style
aggregate over the **same canonicalized set** as the ranked select — so a `↔` edge counts **once**,
not twice. The count NEVER references `GET_EDGE_DISPLAY_LIMIT` (it is uncapped). This is what keeps
the visible-empty-box feedback loop and the #744/#745 inbound-degree observability intact
(D-05/D-10, ADR-001/007, FR-10, C-6/C-7).

## Location

`crates/unimatrix-store/src/graph_queries_ranked.rs` (same new module as `query_ranked_neighbors`).
Co-location is deliberate: **both queries MUST apply the identical canonicalization CASE** — drift
between them re-introduces a double-count on one surface only (ADR-007 "Harder", R-03).

## Function Signature

```
async fn count_neighbors_split(
    pool: &SqlitePool,
    id: u64,
) -> Result<EdgeCountSplit, StoreError>

struct EdgeCountSplit { inbound: usize, outbound: usize }   // post-canonicalization, ↔ once
```

## Canonical-row → direction bucket convention (stated + tested — ADR-007 #4)

A `↔` (symmetric) edge's canonical row has `direction = 'both'`. It must be attributed to **exactly
one** bucket. **Convention: count every `↔` edge as `inbound`.** Asymmetric edges count in their
actual leg (`outbound` if anchor is `source_id`, `inbound` if anchor is `target_id`). This makes
`↔` counted once; the convention is asserted by a test so it cannot silently drift.

> Rationale for choosing inbound: the inbound count is the load-bearing observability signal
> (#744/#745). Bucketing `↔` consistently into inbound keeps the convention single and testable.
> The invariant the spec requires is "once, not twice" — the bucket choice is fixed here so the
> tester asserts a definite expected number.

## The SQL (same `deduped` CTE as the ranked select; `?1` anchor bind; NO cap, NO LIMIT)

```sql
WITH nbr AS (        -- identical to store-ranked-query
    SELECT source_id, target_id, target_id AS other_id, relation_type, source, 'outbound' AS leg
    FROM graph_edges WHERE source_id = ?1 AND relation_type != 'Supersedes'
    UNION ALL
    SELECT source_id, target_id, source_id AS other_id, relation_type, source, 'inbound' AS leg
    FROM graph_edges WHERE target_id = ?1 AND relation_type != 'Supersedes'
),
canon AS (           -- identical to store-ranked-query
    SELECT relation_type, source, other_id,
           CASE WHEN relation_type IN ('Contradicts','CoAccess','Informs')
                THEN 'both' ELSE leg END AS direction,
           MIN(?1, other_id) AS pair_lo, MAX(?1, other_id) AS pair_hi
    FROM nbr
),
deduped AS (         -- identical to store-ranked-query
    SELECT relation_type, source, other_id, direction
    FROM canon
    GROUP BY relation_type, pair_lo, pair_hi,
             CASE WHEN direction = 'both' THEN 1 ELSE other_id END
)
SELECT
    -- ↔ ('both') bucketed into inbound (convention above); asymmetric counted in its leg
    SUM(CASE WHEN direction IN ('inbound','both') THEN 1 ELSE 0 END) AS inbound,
    SUM(CASE WHEN direction = 'outbound'          THEN 1 ELSE 0 END) AS outbound
FROM deduped
```

> No `LIMIT`, no `GET_EDGE_DISPLAY_LIMIT` — uncapped (C-12). It is a bounded aggregate over the
> indexed neighbor predicate; it NEVER materializes rows into Rust (C-7/SR-14). The canonicalization
> CTE is byte-identical to the ranked select's (extract it into one shared `&str` fragment or a
> shared CTE-builder so the two cannot diverge — R-03 parity).

## Body (pseudocode)

```
fn count_neighbors_split(pool, id):
    let sql = <the static statement above>
    let row = sqlx::query(&sql)
        .bind(id as i64)                                  // ?1 (used by both legs + MIN/MAX)
        .fetch_one(pool)
        .await
        .map_err(|e| StoreError::Database(e.into()))?    // FR-19: Result, no .unwrap()
    let inbound  = row.try_get::<i64,_>("inbound").map_err(StoreError::Database)?  as usize
    let outbound = row.try_get::<i64,_>("outbound").map_err(StoreError::Database)? as usize
    Ok(EdgeCountSplit { inbound, outbound })
```

> `SUM(...)` over zero rows returns NULL in SQLite. Guard: read as `Option<i64>` and
> `unwrap_or(0)`, OR wrap with `COALESCE(SUM(...),0)` in the SQL. Prefer `COALESCE` so the Rust
> side reads a non-null `i64` (the `?1`-anchor binding appears 4× — anchor in both legs and in
> `MIN`/`MAX`; if the driver requires distinct positional indices, repeat the bind or use a named
> bind — implementer's call, but the count MUST stay a single aggregate).

## Constraints honored

- **C-6/ADR-007**: canonicalization BEFORE `COUNT` — `deduped` is the counted set.
- **C-7/SR-14**: counting in SQL, never Rust-side counting of a materialized neighbor set.
- **C-12**: uncapped; the constant is not referenced.
- **FR-7/D-04**: `!= 'Supersedes'` inherited ⇒ Supersedes excluded from totals too.
- **NFR-3**: `read_pool_server()`, indexed predicate.

## Data Flow

- **Inputs**: `pool` (`read_pool_server`), `id` (anchor).
- **Outputs**: `EdgeCountSplit { inbound, outbound }`; consumed by get-edge-assembly as
  `EdgeTotals` inside `EdgesView`.

## Error Handling

- Any sqlx/`try_get` failure ⇒ `StoreError::Database`, propagated via `?`. **No `.unwrap()`/`expect()`**.
- Non-existent / zero-edge `id` ⇒ `{inbound: 0, outbound: 0}` (via `COALESCE`), NOT an error
  (drives the FR-12 explicit empty state downstream).

## Key Test Scenarios

- **canonicalization parity (R-03.2, R-01)** — a symmetric pair (both rows) contributes **once** to
  the totals AND occupies one slot in the ranked select; assert on the COUNT output directly,
  independent of the displayed set. Extend to all three symmetric types.
- **capped totals exact (R-03.1, FR-10)** — seed `> cap` mixed-direction edges; the rendered set is
  ≤cap but the totals report the true uncapped split (drives the `…N more` affordance downstream).
- **direction split load-bearing (R-03.3, #744)** — high inbound + zero outbound entry reports the
  true inbound count (observability survives the cap).
- **↔ bucket convention** — a single `↔` edge contributes 1 to inbound, 0 to outbound (per the
  stated convention); asserted explicitly so the convention cannot drift.
- **zero-edge** — non-existent/edge-free `id` ⇒ `{0, 0}` (no error).
- **Supersedes excluded from totals (R-12)** — a `Supersedes` edge is not counted.
- **counting in SQL (R-04)** — assert the statement carries `SUM(...)`/aggregate, not a row fetch +
  Rust count.
