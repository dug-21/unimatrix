# Component: subgraph-depth1-dispatch

File: `crates/unimatrix-server/src/mcp/graph_read_subgraph.rs`

## Purpose

Route `subgraph` at `max_depth == 1` to the existing live-SQL `subgraph_via_db` so a write committed
immediately before the call is visible with no tick lag (AC-01/AC-07/AC-11), and impose a stable,
presentation-only ordering on subgraph output uniformly across all paths (FR-9 / ADR-003). No new
helper, no wire/struct change, no lock on the depth-1 path.

Two edits in this one file:
1. **Dispatch** — an exact `max_depth == 1` early return, inserted before the lock block.
2. **Ordering** — a uniform sort applied as the final assembly step in BOTH `subgraph_via_db` and
   `handle_subgraph`'s warm-BFS assembly.

Both are additive; every existing branch's returned SET is unchanged.

---

## Edit 1 — depth-1 dispatch (in `handle_subgraph`)

### Insertion point (load-bearing — SR-07 / ADR-001)

Insert AFTER `resolve_supersessions` is computed (currently `graph_read_subgraph.rs:162`) and BEFORE
the "Step 2: Acquire graph snapshot" lock block (currently `:164`/`:169`). `petgraph_dirs`,
`edge_types`, `seed_ids`, `max_nodes`, `max_depth`, `resolve_supersessions` are ALL already resolved
above the insertion point — no new plumbing.

### Pseudocode

```
# ... existing validation through:
#   let resolve_supersessions = params.resolve_supersessions.unwrap_or(true);   (:162)

# NEW — depth-1 live dispatch (before the lock block):
IF max_depth == 1:
    RETURN subgraph_via_db(
        store,
        &seed_ids,
        max_depth,              # == 1
        max_nodes,
        &petgraph_dirs,
        &edge_types,
        resolve_supersessions,
    ).await
END IF

# ... existing "Step 2" lock block and everything below runs ONLY for max_depth > 1, unchanged.
```

### Correctness constraints

- **Exact `== 1`** — never `<= 1` or a range. `max_depth` is already validated to `1..=10` above, so
  depth>1 always falls through to the unchanged lock/BFS/`use_fallback` path (AC-02).
- **Before the lock** — the depth-1 return precedes `typed_graph_state.read()`, so depth-1 acquires
  ZERO `TypedGraphState` lock (A3 / NFR-2 / AC-10 / R-08).
- **Depth>1 untouched at SET level** — the `use_fallback` → `subgraph_via_db` cold-start branch
  (`:177–188`) and the warm in-memory BFS (`:190+`) are reachable only for `max_depth > 1` and keep
  their returned node/edge SET byte-for-byte (R-01/R-02). The only added effect on depth>1 is Edit 2's
  presentation-only sort.
- **Filter args already resolved** — passing `petgraph_dirs`/`edge_types`/`resolve_supersessions` after
  full resolution (not defaults) is the load-bearing integration point; an insertion above their
  resolution would leak stale/default filters to the live path (Integration Risk R-01/R-03).

---

## Edit 2 — uniform ordering sort (in BOTH functions)

### What

After the final `nodes: Vec<EntryRecord>` and `edges: Vec<EdgeRecord>` are assembled (i.e. after
hydration + metadata + `EdgeRecord` construction, after the R-05 dangling filter), and immediately
BEFORE building the `SubgraphResponse`, sort:

- `nodes` by ascending `EntryRecord.id`.
- `edges` by canonical triple `(source_id, target_id, relation_type)` ascending.

### Recommended shape — one shared helper (guarantees uniformity structurally, SR-03)

Add a private free function and call it from both `handle_subgraph` (warm-BFS assembly) and
`subgraph_via_db`, so the two paths cannot drift into two ordering contracts:

```
fn sort_subgraph_output(nodes: &mut Vec<EntryRecord>, edges: &mut Vec<EdgeRecord>):
    nodes.sort_by_key(|n| n.id)                       # ascending id; ids unique → total order
    edges.sort_by(|a, b|
        (a.source_id, a.target_id, &a.relation_type)
            .cmp(&(b.source_id, b.target_id, &b.relation_type)))
```

Call site in each function (replace the direct `Ok(SubgraphResponse { ... })` tail):

```
    let mut nodes = ...;   # existing hydration result, made mutable
    let mut edges = ...;   # existing EdgeRecord Vec, made mutable
    sort_subgraph_output(&mut nodes, &mut edges);      # NEW — final assembly step
    Ok(SubgraphResponse { nodes, edges, truncated, seed_ids: ..., depth_reached })
```

`depth_reached` is computed from `collected_edges` (`.map(|e| e.3).max()`) BEFORE this sort and is
order-independent — leave it where it is. Per-edge `EdgeRecord.depth` is preserved (the sort reorders
whole records, not fields).

Inline alternative (no helper) is acceptable but the helper is preferred because it makes "one
ordering contract across paths" structural rather than a copy-paste convention.

### Correctness constraints

- **Presentation-only + set-preserving** (FR-9 / AC-02 / ADR-003): the sort runs AFTER the dangling
  filter and never inserts/removes members. `truncated` and `depth_reached` are unchanged.
- **Deterministic** (NFR-4 / AC-14): node ids are unique so node order is fully determined; use a
  **stable** sort for edges (`sort_by` is stable) so any duplicate canonical triple keeps insertion
  order — no run-to-run flake in the DoD one-shot.
- **Uniform across depths** (SR-03): identical helper on both the live/`subgraph_via_db` path and the
  warm-BFS `handle_subgraph` path. Prior depth>1 order was arbitrary/undocumented, so this is not a
  set-level behavior change for depth>1 (reconciles AC-02).

---

## Data flow (depth-1 live path, post-change)

```
GraphParams (max_depth=1, seed_ids, edge_types?, direction?, resolve_supersessions?)
  → handle_subgraph validation → petgraph_dirs, edge_types(RelationType), resolve_supersessions
  → subgraph_via_db:
       seeds (follow_to_current if resolve) → BFS depth 1 via query_direct_neighbors(read_pool_server)
       → R-02 canonical-triple dedup → max_nodes cap / truncated
       → R-05 dangling-edge filter
       → fetch_nodes_batch (id,title,content,status,kind,tags via load_tags_for_entries)
       → fetch_edge_metadata (OR-chain ≤ MAX_EDGES_UPPER=1000)
       → build EdgeRecord list (direction always "outgoing")
       → sort_subgraph_output(nodes, edges)         ← NEW
       → SubgraphResponse
```

## Error handling (unchanged — inherited)

- Invalid `seed_ids`/`max_depth`/`max_nodes`/`direction`/`edge_types` → `ErrorData(ERROR_INVALID_PARAMS)`
  returned by existing validation BEFORE the depth-1 dispatch is reached.
- Live DB read failure inside `subgraph_via_db` (`query_direct_neighbors`, `fetch_nodes_batch`,
  `fetch_edge_metadata`) → propagated as `ErrorData(ERROR_INTERNAL)` exactly as on today's cold-start
  path. No new swallow/warn-continue.
- Non-existent / dangling seed → empty neighborhood, not an error (matches cache path).
- `edge_types` absent or `[]` → `all_non_supersedes_types()` (all except Supersedes), same on both paths.

## Key test scenarios (hints for tester — full plan is risk-driven)

- Dispatch: `max_depth==1` reflects a pre-call write (R-01/AC-01); `max_depth==2` and `==10` do NOT
  route live (served from cache, within-tick write NOT visible) (R-01/AC-02/AC-11).
- Depth>1 cold-start: empty `TypedRelationGraph` at depth>1 still fires `use_fallback` → live (R-02/AC-02).
- Load-bearing path on depth-1 live: R-02 dedup under `direction:both` (one physical edge → one record);
  R-05 dangling filter when `max_nodes` caps mid-hop; `MAX_EDGES_UPPER` cap respected (R-04).
- Dual-path SET parity: same seed+`edge_types`+`direction`+`resolve_supersessions` on warm+fresh graph →
  depth-1 live node SET == prior cache-BFS node SET, edge SET == edge SET, order-independent (R-03/NFR-3).
- Filters at depth-1: `edge_types` given-only + absent-defaults-all; `direction` inclusion honored while
  `EdgeRecord` keeps canonical `source→target` + `direction:"outgoing"` label (R-11/AC-05/AC-06).
- Hydration/tag parity: depth-1 node carries id,title,content,status,kind,tags equal to cache path (R-05/AC-04).
- Supersession: `resolve_supersessions` default-true vs explicit-false identical on depth-1 (AC-08).
- Ordering: depth-1 nodes ascending by id, edges by `(source_id,target_id,relation_type)`; DoD one-shot
  run twice byte-identical; depth>1 output carries the SAME keys (R-06/AC-14). Sweep existing depth>1
  tests for a fixed-order assertion; update any as presentation-only.
- Truncation: ≥30 incoming `Advances` → `truncated==false`; pathological >199 → `truncated==true` (R-09/AC-15).
- No lock on depth-1: structural/review assertion that the `==1` early return precedes any
  `typed_graph_state.read()` (R-08).
