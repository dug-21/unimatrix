# Architecture — vnc-043: context_graph subgraph live depth-1 read + doc fix

> GH #903. NARROW feature: handler dispatch + doc text + tests. No wire/interface/struct change.
> Extends established decisions (ADR-005 vnc-018, ADR-001/003/004 vnc-019) — does not invent new contracts.

## System Overview

`context_graph` is the MCP graph-traversal tool (seven modes). `subgraph` mode does multi-hop
BFS from seed IDs with edge-type/direction filtering and batch node hydration. Today subgraph
always reads the in-memory tick-cache (`TypedRelationGraph`, rebuilt every 30–60 s) unless
`use_fallback == true` (cold-start / cycle), in which case it reads live SQL via `subgraph_via_db`.

This feature makes `subgraph` at `max_depth == 1` read the **live DB unconditionally**, mirroring
the established `neighbors` depth-1 = live / depth>1 = cache asymmetry (ADR-005 vnc-018). It also
corrects three mis-documented discoverable surfaces (`edge_types`/`direction` already work on
subgraph but the docs say "neighbors only") — the literal root cause of #903.

Two co-equal Class-1 deliverables:
1. **DOC** — correct `edge_types`/`direction` availability + depth-1 staleness carve-out across the
   discoverable surfaces (schemars field docs + the twin description literals).
2. **CODE** — route `max_depth == 1` to the existing `subgraph_via_db` before lock acquisition.

## Component Breakdown

| Component | File | Responsibility | Change |
|-----------|------|----------------|--------|
| `handle_subgraph` | `mcp/graph_read_subgraph.rs:68` | subgraph entry point; validate → dispatch → assemble | **Insert `max_depth == 1` dispatch** after param resolution, before lock; add ordering to warm-BFS assembly |
| `subgraph_via_db` | `mcp/graph_read_subgraph.rs:395` | live-SQL BFS (seed + hops), filter/dedup/dangling/hydrate/metadata | **Reused as the depth-1 path** (no new helper); add ordering to its assembly |
| `GraphParams` schemars docs | `mcp/graph_read.rs:82` (`direction`), `:84` (`edge_types`) | discoverable field contract | **Doc text edit** — state both apply to subgraph |
| `CONTEXT_GRAPH_DESCRIPTION` | `mcp/tools.rs:76` | mirror const (substring-tested) | **Doc text edit** — filter availability + depth-1 live carve-out |
| live `#[tool(description=…)]` literal | `mcp/tools.rs:~3945–3996` | client-facing description (byte-identical twin of the const) | **Identical doc text edit** |
| byte-equality guard | `mcp/tools.rs:6263` `test_graph_tool_attr_description_matches_const` | enforces the two literals stay byte-identical | unchanged (already the SR-01 guard) |
| substring assertions | `mcp/tools.rs:6198+` | semantic content of description | **Extend** with new filter/staleness assertions |

## Component Interactions / Data Flow

`tools.rs::context_graph` → `require_cap(Read)` → `validate_no_unsupported_params`
(`graph_read_validation.rs` — subgraph arm unchanged; already permits `seed_ids/max_nodes/max_depth`,
does not reject `edge_types`/`direction`) → `handle_subgraph`.

Inside `handle_subgraph` (post-change):
```
1. validate seed_ids, max_depth, max_nodes, direction→petgraph_dirs, edge_types, resolve_supersessions
2. IF max_depth == 1  → return subgraph_via_db(store, &seed_ids, 1, max_nodes,
                                &petgraph_dirs, &edge_types, resolve_supersessions).await   ← NEW (live)
3. { lock → clone graph + use_fallback → release }                                          ← only depth>1
4. IF use_fallback    → return subgraph_via_db(..., max_depth, ...)                          ← unchanged
5. in-memory BFS over TypedRelationGraph                                                     ← unchanged
6. dangling-filter → hydrate → metadata → ORDER → SubgraphResponse
```
`petgraph_dirs` and `edge_types` are already computed (lines 116–156) before the insertion point,
so the depth-1 call needs no new plumbing. **The depth-1 path never acquires the `TypedGraphState`
lock** (constraint SCOPE §Constraints / A3), preserving "no hot-path touch" (AC-10).

## Dispatch insertion point (precise)

Insert the exact-match `max_depth == 1` guard **after** `resolve_supersessions` is computed
(current `graph_read_subgraph.rs:162`) and **before** the lock/snapshot block (current `:164–172`).
This ordering is load-bearing (SR-07):
- Exact `== 1` match — never `<= 1` or a range — cannot capture depth>1.
- Placed before the lock block, so the `use_fallback` → live cold-start branch (`:177–188`) and the
  warm in-memory BFS (`:190+`) are reached **only** for `max_depth > 1` and remain byte-for-byte
  unchanged (AC-02). The depth>1 cold-start fallback still fires on an empty `TypedRelationGraph`.

This mirrors `handle_neighbors` (`graph_read_neighbors.rs:185`): `if depth == 1 { neighbors_sql }
else { neighbors_bfs }`. Difference: neighbors has a *dedicated* single-hop `neighbors_sql`;
subgraph **reuses `subgraph_via_db`** (Open Q2 resolved) because at `max_depth == 1` it already does
exactly seed + one hop with filter, R-02 dedup, R-05 dangling filter, hydration (tags via ADR-006),
metadata, and `max_nodes`/`truncated` — a dedicated helper would duplicate it.

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `handle_subgraph` | `async fn(store: &Store, typed_graph_state: &Arc<RwLock<TypedGraphState>>, params: &GraphParams) -> Result<SubgraphResponse, ErrorData>` | `graph_read_subgraph.rs:68` |
| `subgraph_via_db` (reused depth-1 path) | `async fn(store: &Store, seed_ids: &[u64], max_depth: u8, max_nodes: u32, petgraph_dirs: &[PetgraphDirection], edge_types: &[RelationType], resolve_supersessions: bool) -> Result<SubgraphResponse, ErrorData>` | `graph_read_subgraph.rs:395` |
| `query_direct_neighbors` | `async fn(pool, id: u64, edge_types: &[&str], NeighborDirection) -> Result<Vec<Neighbor>>` (same edge query `neighbors` depth-1 uses) | `unimatrix_store` |
| `SubgraphResponse` | `{ nodes: Vec<EntryRecord>, edges: Vec<EdgeRecord>, truncated: bool, seed_ids: Vec<u64>, depth_reached: u8 }` — **shape fixed** (ADR-004 vnc-019; no `graph_rebuilt_at`) | `mcp/mod.rs` |
| `EdgeRecord` | `{ source_id: u64, target_id: u64, relation_type: String, direction: "outgoing", depth: u8, metadata: Option<Value> }` | `mcp/mod.rs` |
| `fetch_nodes_batch` | hydrates id, title, content, status, kind, tags (tags via `load_tags_for_entries`, ADR-006) | `graph_read_subgraph.rs:585` |
| `GraphParams.edge_types` / `.direction` | `Option<Vec<String>>` / `Option<String>` — wire-locked, already present (ADR-003 vnc-018 / ADR-001 vnc-019); **doc-only** change | `graph_read.rs:82,84` |
| `CONTEXT_GRAPH_DESCRIPTION` + live twin literal | `&str` × 2, byte-identical (rmcp 1.7.0 cannot single-source a const into `#[tool]`) | `tools.rs:76`, `:~3945` |
| byte-equality guard | `test_graph_tool_attr_description_matches_const` (#869) | `tools.rs:6263` |

Downstream agents MUST NOT invent new names, fields, or a `subgraph_sql` helper — reuse
`subgraph_via_db`. No `GraphParams`/`SubgraphResponse`/`RelationEdge` shape change (AC-10).

## Resolved decisions (ADRs + design-level resolutions)

- **ADR-001 vnc-043** — depth-1 live dispatch by reusing `subgraph_via_db` (insertion point, dual-path
  parity, load-bearing-path regression coverage). Addresses SR-02, SR-06, SR-07.
- **ADR-002 vnc-043** — description source-of-truth: keep the twin-literal + byte-equality-guard
  pattern; edit both literals identically. Addresses SR-01.
- **ADR-003 vnc-043** — depth-1 response contract: stable ordering (nodes by `id`, edges by
  `(source_id, target_id, relation_type)`) applied uniformly across all subgraph paths, plus
  `max_nodes`/`truncated` semantics. Addresses SR-03, AC-14, Open Q5 / SR-05.

### Open Q4 / SR-04 resolution (snapshot pinning) — no external snapshot exists
Verified in-repo: **no** `.snap` files, **no** `insta`/`assert_snapshot`, **no** `schema_for`/JSON-schema
snapshot pins the description string or the `GraphParams` schema. The only pins on the description are
in-crate and both handled in-scope:
1. Substring assertions (`tools.rs:6198+`) — assert *presence*; adding text is safe. **Extend** them
   with the new filter-availability and depth-1-live substrings (AC-13, AC-09).
2. Byte-equality guard `test_graph_tool_attr_description_matches_const` (#869) — requires the two
   literals stay byte-identical; kept green by editing both literals identically (ADR-002).
The schemars field docs (`direction`/`edge_types`) are single-source (derive) — edited once each, no
snapshot pin. **Conclusion:** the doc fix cannot silently red-bar CI on a stale snapshot; there is none.

### Open Q5 / SR-05 resolution (max_nodes / truncation at depth-1) — no caller change
`subgraph_via_db` already honors `max_nodes` (default 200, hard cap 200) and sets `truncated`. At
depth-1 the result is `seed(s) + one hop`, so a single-goal board read tolerates up to **199** incoming
capabilities before truncating at the default. Realistic goal capability fan-in is an order of magnitude
below that. **Decision:** the board caller does **not** raise `max_nodes`; the default (200) covers
realistic fan-in. "Realistic fan-in" for the AC-15 test = **≥ 30 incoming `Advances` capabilities on the
seed goal**, asserting `truncated == false` (comfortably above a real goal, well under the 199 headroom).
The existing `truncated` flag stays surfaced and asserted so a pathological >199-neighbor board read
signals partial rather than truncating silently (no-silent-truncation, AC-15). No new field, no
`SubgraphResponse` change (ADR-004 vnc-019 preserved).

## Regression coverage the reuse demands (SR-02)

Routing every `max_depth == 1` call through `subgraph_via_db` promotes a formerly cold-start-only branch
to the default board-read path. The test plan MUST cover, **on the depth-1 live path** (not just the DoD
happy one-shot):
- R-02 edge dedup under `direction: both` (no double edges on one physical edge).
- R-05 dangling-edge filter when `max_nodes` caps mid-hop.
- Hydration parity — id, title, content, status, kind, **tags** (ADR-006) identical to the cache path.
- `MAX_EDGES_UPPER` (1000) metadata OR-chain guard respected.
- `max_nodes` / `truncated` behavior (Open Q5 fixture).
- **Dual-path SET parity (SR-06):** same seed + `edge_types` + `direction` + `resolve_supersessions`
  yields the same node/edge *set* (order-independent) on depth-1 live vs the prior warm cache-BFS, on a
  warm+fresh graph with no within-tick writes. The only intended difference is freshness.
- **Freshness both ways (SR-08):** write-then-read visible at depth-1 (AC-11); within-tick write NOT
  visible at depth>1 (staleness preserved, ADR-005 vnc-018 precedent).
- **depth>1 non-regression:** cold-start fallback still fires on an empty `TypedRelationGraph` (AC-02).

## Constraints honored

- No `GraphParams` / `SubgraphResponse` / `RelationEdge` wire or struct change (AC-10; ADR-003 vnc-018).
- Depth-1 live path takes no `TypedGraphState` lock (A3; hot path untouched).
- Staleness disclosed in description text only — no `graph_rebuilt_at` (ADR-004 vnc-019).
- `fetch_edge_metadata` OR-chain stays ≤ `MAX_EDGES_UPPER` (already enforced in `subgraph_via_db`).
- File stays under the 500-line limit (dispatch is ~6 lines; no split needed).

## Open questions for other agents / human

- **For tester:** confirm no existing depth>1 subgraph test asserts a *fixed* output order that the new
  uniform ordering (ADR-003) would change; if one does, update it as presentation-only (the set is
  unchanged). `fetch_nodes_batch` already documents "arbitrary order," so well-written tests use sets.
- **For spec/synthesizer:** the AC-13 substring assertions and AC-14 ordering assertion are new test
  contracts, not code the coder invents — carry them into the acceptance map explicitly.
