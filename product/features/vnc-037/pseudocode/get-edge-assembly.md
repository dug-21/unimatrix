# Component: get-edge-assembly

## Purpose

The `context_get` handler's edge logic: resolve the opt-out, issue the ranked select + split count
+ batched title join, project rows → `Vec<GetEdge>`, build `EdgesView`, and hand it to the
serializer. Implements the **FR-19 fail-loud** contract: on the default-on path, any post-primary-
read edge/count/title failure maps to the SAME `ServerError` as the primary read and is RETURNED
(no degrade, no silent omit). Lives in a sibling module so `tools.rs` stays < 500 lines (ADR-001/
002/003, FR-1/FR-3/FR-5/FR-19, C-13, NFR-1).

## Location

- `crates/unimatrix-server/src/mcp/get_edges.rs` (new — pre-authorized OQ-B): `build_edges_view`,
  the title batch, and the projection.
- `crates/unimatrix-server/src/mcp/tools.rs` — the `context_get` handler (`:935`) calls it between
  the primary `entry_store.get` (`:963`) and `format_single_entry` (`:968`).

## Handler integration (tools.rs context_get, between step 3 and step 4)

```
// 3. Primary read (EXISTING — unchanged). On Err ⇒ mapped ServerError, returned.
let entry = self.entry_store.get(id).await
    .map_err(|e| ErrorData::from(ServerError::Core(CoreError::Store(e))))?;

// 3b. NEW — resolve edges (default-on; opt-out skips all queries).
let edges_view: Option<EdgesView> = match params.include_edges {
    Some(false) => None,                          // FR-3: skip ranked select + count + title join entirely
    None | Some(true) => {
        // FR-19 FAIL-LOUD: map any edge/count/title Err to the SAME ServerError as the primary read.
        Some(
            build_edges_view(self.store(), id).await
                .map_err(|e| ErrorData::from(ServerError::Core(CoreError::Store(e))))?
        )
    }
};

// 4. Format (seam) — pass Option<&EdgesView>.
let result = format_single_entry(&entry, ctx.format, edges_view.as_ref());
```

> The `.map_err(...)?` uses the **identical** mapping the primary `entry_store.get` uses at
> `tools.rs:963-965` (`ServerError::Core(CoreError::Store(e))` → `ErrorData::from`). That is the
> "same error mapping as the primary-read failure path" FR-19/C-13 requires — one consistent
> failure contract, no new partial-success shape. The `?` propagates, so a failed edge path
> returns an error result, never a success payload with omitted edges. On `Some(false)`,
> `build_edges_view` is never called, so this path cannot fail there (FR-19 scoped to default-on).

## build_edges_view (get_edges.rs)

```
async fn build_edges_view(store: &Store, id: u64) -> Result<EdgesView, StoreError>:
    let pool = store.read_pool_server()                       // NFR-3 read pool

    // 1. ranked ≤cap displayed rows (canonicalized, LEFT JOIN confidence, LIMIT GET_EDGE_DISPLAY_LIMIT)
    let rows = query_ranked_neighbors(pool, id).await?        // Result — no .unwrap() (FR-19)

    // 2. honest uncapped split totals (same canonicalized set, ↔ once)
    let split = count_neighbors_split(pool, id).await?        // Result — no .unwrap() (FR-19)

    // 3. batched title join over the ≤cap displayed targets ONLY (never the uncapped set)
    let target_ids: Vec<u64> = rows.iter().map(|r| r.target_id).dedup_collect()
    let title_map: HashMap<u64,String> =
        if target_ids.is_empty() { HashMap::new() }
        else { fetch_titles_batch(pool, &target_ids).await? } // Result — no .unwrap() (FR-19)

    // 4. project rows → GetEdge (see get-edge-vocabulary)
    let edges: Vec<GetEdge> = rows.iter().map(|r| GetEdge {
        edge_type:    r.relation_type.clone(),
        direction:    r.direction_hint,                       // "both"/"outbound"/"inbound" from ranked SQL
        target_id:    r.target_id,
        target_title: title_map.get(&r.target_id).cloned(),   // None ⇒ dangling (retained, DNB-1)
        authored:     r.source == EDGE_SOURCE_AGENT,           // exact match, store constant (R-09)
    }).collect()

    // 5. assemble (edges already ≤cap from SQL; totals uncapped)
    Ok(EdgesView { edges, totals: EdgeTotals { inbound: split.inbound, outbound: split.outbound } })
```

> Return `Result<EdgesView, StoreError>` so the single `.map_err(...)?` at the handler maps the
> whole thing once (ranked / count / title failures funnel through the same `?`). If `fetch_titles_batch`
> uses `ErrorData` internally (like `fetch_nodes_batch` at `graph_read_subgraph.rs:586`), wrap it
> to `StoreError` here, OR map directly to `ServerError` and have `build_edges_view` return
> `Result<EdgesView, ErrorData>` — either is FR-19-compliant as long as the mapping equals the
> primary-read mapping and there is no `.unwrap()`. Pick one and keep it consistent.

## fetch_titles_batch (get_edges.rs) — precedent fetch_nodes_batch (graph_read_subgraph.rs:568)

```
async fn fetch_titles_batch(pool, ids: &[u64]) -> Result<HashMap<u64,String>, _>:
    // positional binds, ≤cap ids — naturally small, no chunking (Security: never string-interpolate ids)
    let placeholders = repeat("?", ids.len()).join(", ")
    let sql = format!("SELECT id, title FROM entries WHERE id IN ({})", placeholders)
    let mut q = sqlx::query(&sql); for id in ids { q = q.bind(*id as i64) }
    let rows = q.fetch_all(pool).await.map_err(...)?           // Result — no .unwrap()
    Ok(rows.iter().filter_map(|r| Some((r.get::<i64,_>("id") as u64, r.get::<String,_>("title")))).collect())
    // an id absent from the result ⇒ not in the map ⇒ target_title: None downstream (dangling retained)
```

> Only the ≤cap displayed targets are title-resolved (NFR-1); the uncapped neighbor set is never
> materialized. One batched join, never N+1 (FR-5).

## Constraints honored

- **FR-3**: `Some(false)` ⇒ `None`, zero edge queries (NFR-1 opt-out adds zero cost).
- **FR-19/C-13**: every query returns `Result`, no `.unwrap()`/`expect()`; default-on failure maps
  to the primary-read `ServerError` and is returned; opt-out cannot reach the failure.
- **FR-5**: one batched title join over ≤cap targets; dangling ⇒ `None` title, edge retained.
- **C-7/SR-14**: the ≤cap bound comes from the SQL `LIMIT`; assembly never slices an unbounded fetch.
- **Security**: positional binds for the `IN (…)` list.

## Data Flow

```
include_edges resolve → (Some(false): None)
                      → (None/Some(true): query_ranked_neighbors + count_neighbors_split + fetch_titles_batch
                                          → project → EdgesView)  ── Err ⇒ FR-19 mapped ServerError, returned
EdgesView (Some) | None → format_single_entry(entry, format, edges)
```

## Error Handling

- Post-primary-read failure on default-on ⇒ mapped `ServerError` (same as primary read), RETURNED.
  **Distinct from a zero-edge success** (FR-12): zero edges ⇒ `Ok(EdgesView{edges:[], totals:{0,0}})`
  → success with explicit empty state; a failure ⇒ `Err` → no success payload (AC-14b distinction).
- Non-existent id never reaches here (primary `entry_store.get` errors first).
- Dangling target ⇒ `target_title: None`, retained — not a failure (DNB-1).

## Key Test Scenarios

- **edge-query-failure-fails-loud (R-16, AC-14a, named RED #4876)** — inject a failure into the
  ranked query AFTER a successful primary read ⇒ `context_get` returns the mapped `ServerError`,
  NOT a success with omitted edges. Repeat for the split count and the title join (same mapped
  failure). Run RED.
- **zero-edges-is-not-failure (AC-14b)** — a genuine zero-edge entry returns a SUCCESS with the
  explicit empty state, structurally distinguishable from the (a) error result.
- **opt-out skips ALL queries (R-14, AC-11)** — `Some(false)` issues zero ranked/count/title
  queries (query-count/instrumentation) and the payload has no `edges` key.
- **default-on surfaces (AC-01)** — `None`/`Some(true)` surface ≤cap edges + totals; a just-written
  edge appears immediately (live SQL, no tick wait).
- **batched title join, no N+1 (AC-02)** — query-count assertion: titles resolve in ONE join over
  the ≤cap targets; uncapped set never title-resolved.
- **carried-forward / context_edge authored (R-05, FR-17, named)** — a carried-forward (vnc-035)
  or `context_edge`-written edge has `source='agent'` ⇒ `authored=true` and wins a slot ahead of
  inferred.
- **internal-caller opt-out (OQ-03)** — enumerated internal call sites (hook path, briefing by-ID
  fetches, by-ID loop fetches) pass `Some(false)`; asserted per site (see get-params).
- **no .unwrap()/expect() on the edge path (AC-14c, static)**.
