## ADR-001 vnc-043: subgraph depth-1 reads live via reused `subgraph_via_db`, dispatched before the lock

### Context
`subgraph` mode always reads the in-memory tick-cache (`TypedRelationGraph`, rebuilt every 30–60 s)
except when `use_fallback == true` (cold-start / cycle), where it already reads live SQL via
`subgraph_via_db` (`graph_read_subgraph.rs:395`). A curator who writes a capability status/edge and
immediately re-queries the capability board inside the tick window sees stale data. `neighbors` mode
solved the identical problem with a depth-1 = live / depth>1 = cache split (ADR-005 vnc-018 #4479):
"depth=1 must be fresh; depth>1 tolerates a tick window." vnc-043 extends that established asymmetry
into subgraph. This is not a new contract — it mirrors a decided one.

Two risks shape the decision:
- **SR-02** — `subgraph_via_db` is today reached only on the rare `use_fallback` branch. Routing every
  `max_depth == 1` call through it makes a cold-start-only path the default board-read path; any latent
  bug (R-02 dedup, R-05 dangling filter, metadata cap) now fires on normal reads.
- **SR-07** — inserting a depth-1 dispatch must not capture depth>1 or disturb the depth>1
  `use_fallback` cold-start branch (#4562 / GH #623), which SCOPE mandates unchanged.

### Decision
At `max_depth == 1`, `handle_subgraph` delegates to the existing `subgraph_via_db` **unconditionally**,
with **no dedicated depth-1 helper** (Open Q2 resolved: reuse). `subgraph_via_db` at `max_depth == 1`
already performs seed + one hop with `edge_types`/`direction` filtering, R-02 canonical-triple dedup,
R-05 dangling-edge filter, batch hydration (id, title, content, status, kind, tags via ADR-006),
`fetch_edge_metadata` under the `MAX_EDGES_UPPER` guard, and `max_nodes`/`truncated` — so a dedicated
`subgraph_sql` would duplicate tested code.

**Insertion point (load-bearing):** the exact-match guard goes **after** `resolve_supersessions` is
computed (`graph_read_subgraph.rs:162`) and **before** the lock/snapshot block (`:164`):
```rust
if max_depth == 1 {
    return subgraph_via_db(
        store, &seed_ids, max_depth, max_nodes,
        &petgraph_dirs, &edge_types, resolve_supersessions,
    ).await;
}
```
`petgraph_dirs` and `edge_types` are already resolved above, so no new plumbing is needed. Consequences
of the placement:
- **Exact `== 1`** — never `<= 1` or a range — so depth>1 can never be captured.
- **Before the lock** — the depth-1 path takes no `TypedGraphState` lock (constraint A3; keeps AC-10
  "no hot-path touch"). The `use_fallback` → live branch (`:177–188`) and the warm in-memory BFS
  (`:190+`) are reached only for `max_depth > 1` and stay byte-for-byte unchanged (AC-02). The depth>1
  cold-start fallback still fires on an empty `TypedRelationGraph`.

Mirrors `handle_neighbors` (`graph_read_neighbors.rs:185`): `if depth == 1 { … } else { … }`. Difference:
neighbors uses a dedicated single-hop `neighbors_sql`; subgraph reuses the fallback BFS at depth 1.

**Load-bearing-path coverage (SR-02, SR-06):** because depth-1 live is now the default board path, the
test plan MUST exercise, on that path specifically: R-02 dedup (`direction: both`), R-05 dangling filter
(cap fires mid-hop), hydration+tag parity, the `MAX_EDGES_UPPER` guard, `max_nodes`/`truncated`, and a
**dual-path SET-parity** assertion — same seed+filter+resolve yields the same node/edge set on depth-1
live vs the prior warm cache-BFS (order aside) on a warm+fresh graph. Freshness is the only intended
difference: write-then-read visible at depth-1 (AC-11); within-tick write NOT visible at depth>1.

### Consequences
Easier: capability-board reads are live at depth-1 with no client-side filter/join and no tick lag,
closing the #903 origin problem; no new code path, no new SQL shape, no wire/struct change; the depth-1
path avoids the lock entirely.
Harder: a formerly-rare branch becomes load-bearing, so its correctness must be regression-covered on
the default path, not just the happy one-shot; subgraph and neighbors now differ in *how* depth-1 is
implemented (reuse vs dedicated helper) — an intentional, documented asymmetry. Any future move to make
subgraph depth>1 live would reopen ADR-004 vnc-019 and require a new ADR.
