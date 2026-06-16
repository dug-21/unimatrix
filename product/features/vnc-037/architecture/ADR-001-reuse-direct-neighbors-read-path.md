## ADR-001 (vnc-037): Rank-and-limit get-edges in SQL — live depth-1 query, confidence JOIN, `LIMIT 3` + a separate split `COUNT(*)`; opt-out skips both

> **REVISED under the next-hop reframe.** The prior decision reused the plain
> `query_direct_neighbors(…, &[], Both)` and capped at 10 in Rust. The reframe makes
> ranking the core and bounds hub-node fan-out: the read path now **ranks and limits in
> SQL** and gets totals from a **separate cheap `COUNT(*)`**.

### Context

`context_get` must surface a **ranked, capped (≤3)** set of an entry's depth-1 typed edges,
both directions, **by default** (D-01/D-05). Two depth-1 read mechanisms exist: the
in-memory `TypedRelationGraph` (tick-lagged ~30–60s, ADR #4479) vs the live `graph_edges`
SQL path that `context_graph` neighbors uses at depth=1. `context_get` is the precise
point-read case where a just-written (or just-carried-forward, vnc-035) edge must be visible
immediately — the asymmetry ADR #4479 resolved toward live SQL at depth=1.

Under the reframe the **plain** `query_direct_neighbors(pool, id, &[], Both)` is **no longer
the right reuse target on its own**: it returns *unranked*, *un-canonicalized*, *unbounded*
rows. A hub node with 1000 edges would pull 1000 rows into memory only to sort and slice
them in Rust — violating the memory/latency intent (SR-14) and double-counting symmetric
edges (SR-08). This is the hottest read in the system and also feeds the co-access loop, so
per-call cost compounds (SR-12).

### Decision

On the opt-in path issue **two bounded SQL queries** against `read_pool_server()`, both
operating on the **canonicalized** edge set (symmetric → one `↔`, ADR-007):

1. **Ranked select (the displayed ≤3)** — a new ranked variant beside
   `query_direct_neighbors` that canonicalizes symmetric edges, `LEFT JOIN entries t ON
   t.id = <other endpoint>` for the rank key (ADR-006), and:
   ```sql
   ... ORDER BY (source = 'agent') DESC, t.confidence DESC NULLS LAST, target_id ASC
       LIMIT 3
   ```
   Authored edges (`source='agent'`) fill slots first; inferred fill the remainder ranked by
   **target-entry confidence** (D-09 / ADR-006). `LEFT JOIN` (not inner) **retains** dangling
   targets with NULL confidence ranked last (`NULLS LAST`, D-02 / SR-11). The
   empty-`edge_types` branch inherits the existing `!= 'Supersedes'` filter (ADR #4461 /
   D-04), so supersession de-dup is free. A non-existent id returns an empty result, not an
   error.

2. **Split `COUNT(*)` (the honest uncapped totals)** — a separate cheap aggregate over the
   **same canonicalized** neighbor set, split inbound/outbound. A `↔` edge counts **once**
   (D-05 / D-10 / ADR-007). The count is independent of the `LIMIT 3`, so totals stay exact —
   that is what keeps the visible-empty-box feedback loop and #744/#745 inbound-degree
   observability intact.

The plain `query_direct_neighbors` / `neighbors_sql` path gains **only** the `source` column
(ADR-004); the rank / JOIN / canonicalization / `LIMIT` logic lives entirely in the **new
ranked variant** so `context_graph` neighbors is untouched (SR-02 / SR-06). The ranked
variant must inherit the same `!= 'Supersedes'` exclusion.

Supporting points (carried forward, still valid):
- **Live SQL on `read_pool_server()`** (the accessor `neighbors_sql` uses) gives point-read
  freshness — a just-written / just-carried edge appears immediately, no staleness to document.
- **One batched title join** for the **≤3 displayed** targets only: `SELECT id, title FROM
  entries WHERE id IN (?, …)` (precedent `fetch_nodes_batch`, `graph_read_subgraph.rs:568`),
  building a `HashMap<u64,String>`. The uncapped set is never title-resolved. Total per
  opt-in get: **ranked select + split count + title batch** — all bounded, none N+1, none
  full-fan-out.
- **Opt-out (`include_edges == Some(false)`) skips the ranked select, the split count, AND
  the title batch** — zero added round trips, behaves as pre-vnc-037 (D-07). OQ-03 recommends
  internal/programmatic callers (hook path, briefing pipeline, by-ID loop fetches) opt out by
  default.
- Edge / count / title query failure maps `StoreError → ServerError/ErrorData` the same way
  the existing `entry_store.get` does; no `.unwrap()` in non-test code.

### Consequences

- **Easier:** hub-node fan-out is **bounded in SQL** — a 1000-edge node returns 3 rows + two
  counts, never 1000 rows (SR-14); point-read freshness preserved; opt-out is free; the
  ranked variant reuses the indexed neighbor predicate (`idx_graph_edges_source_type` /
  `idx_graph_edges_target_type`) plus the `entries` PK for the confidence JOIN; totals stay
  exact and decoupled from the display cap.
- **Harder:** the read path now issues **two** SQL queries (ranked select + split count) plus
  the title batch on the hottest read — the per-call cost is real and **must be measured
  against an edge-free baseline before AC-12 numbers are locked** (SR-12, OQ-C); the ranked
  store function is net-new code that must stay distinct from the plain shared function so the
  neighbors contract does not drift; the confidence JOIN + canonicalization make the ranked
  SQL non-trivial — correctness rests on the discriminating tests named in AC-10 (rank order,
  symmetric-once, high-degree-hits-`LIMIT`).
- **Constraint:** if the plain `query_direct_neighbors` signature or its `!= 'Supersedes'`
  filter changes, both this feature and `context_graph` neighbors are affected — see ADR-004
  for the shared-row safety requirement.

Cross-ref: ADR-004 (the additive `source` column + confidence JOIN this query issues),
ADR-006 (the ranking rule and exact `ORDER BY`), ADR-007 (the symmetric canonicalization
applied before both queries), ADR-002 (the projection consuming the rows).
