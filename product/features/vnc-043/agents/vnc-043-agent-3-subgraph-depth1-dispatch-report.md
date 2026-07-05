# Agent Report — vnc-043 Wave 1: subgraph depth-1 dispatch + uniform ordering

Agent: `vnc-043-agent-3-subgraph-depth1-dispatch`
Component: `subgraph-depth1-dispatch` (`handle_subgraph` / `subgraph_via_db`)

## Summary

Implemented the depth-1 live-read dispatch and the uniform presentation-only ordering for
`context_graph` subgraph mode, per validated pseudocode and ADR-001/ADR-003 (vnc-043):

- **Depth-1 dispatch** — exact `if max_depth == 1 { return subgraph_via_db(...) }` early return
  inserted after `resolve_supersessions` is computed and BEFORE the lock/snapshot block. Reuses the
  already-resolved `seed_ids`/`max_depth`/`max_nodes`/`petgraph_dirs`/`edge_types`/`resolve_supersessions`
  (no new plumbing). Takes NO `TypedGraphState` lock. Exact `== 1` (never `<= 1`/range), so depth>1 and
  its `use_fallback` cold-start branch fall through byte-for-byte unchanged at the SET level.
- **Uniform ordering** — private `sort_subgraph_output(&mut [EntryRecord], &mut [EdgeRecord])`: nodes
  ascending by `id`, edges by the canonical `(source_id, target_id, relation_type)` triple (stable sort).
  Called from BOTH `subgraph_via_db` and `handle_subgraph`'s warm-BFS assembly, AFTER the R-05
  dangling-edge filter and BEFORE `SubgraphResponse` construction. Set-preserving;
  `truncated`/`depth_reached`/per-edge `depth` unchanged.

No `GraphParams` wire/struct change, no `RelationEdge`/hot-path touch.

## Files Modified

- `crates/unimatrix-server/src/mcp/graph_read_subgraph.rs` — dispatch, `sort_subgraph_output` helper,
  sort calls in both paths (`nodes`/`edges` made mutable).
- `crates/unimatrix-server/src/mcp/graph_read_subgraph_bfs_tests.rs` — DB-backed live fixtures
  (`insert_db_entry`, `insert_db_edge`, ordering predicates), 11 new depth-1 tests, 2 existing tests repaired.

## Tests

- Component suite: **37 passed / 0 failed** (`cargo test -p unimatrix-server --lib graph_read_subgraph`).
- Full server lib: **4373 passed / 0 failed / 1 ignored** (`cargo test -p unimatrix-server --lib`).
- Clippy clean on the file; rustfmt applied only to the two changed files (no unrelated churn).

New coverage: dispatch boundary (routes-live at 1, cache at 2 & 10), dual-path SET parity
(absent/empty/explicit `edge_types`), depth-1 dangling-filter-under-cap, hydration/tag parity, node+edge
ordering, depth>1 same ordering keys, deterministic one-shot, direction-label invariant, realistic fan-in
(30 Advances -> `truncated==false`).

## Flags

1. **Adjacent depth-1 fixtures repaired (in-file, resolved):** the dispatch change silently broke two
   existing depth-1 unit tests that seeded only the in-memory `TypedRelationGraph` (no DB rows) —
   `test_bfs_max_depth_one_only_direct_neighbors` and `test_bfs_star_topology_near_cap_edges_within_bound`.
   Fixed: the former converted to a DB-backed live fixture; the latter's `max_depth` moved `1 -> 2` to keep
   it on the warm/cache path (a star's leaves have no second hop, so the collected SET is identical —
   assertion preserved). This trap is captured as Unimatrix pattern #5456.
2. **R-06 depth>1 fixed-order integration sweep deferred to Stage 3c:** I did not audit or modify
   integration/wire suites (`crates/unimatrix-server/tests/graph_subgraph_integration.rs`,
   `product/test/infra-001/suites/test_tools.py`) for depth>1 fixed-order assertions the new uniform sort
   may flip — per instructions that is the 3c tester sweep. No fixed-order depth>1 assertion exists in the
   unit suite I own.
3. **Pre-existing 500-line overage (out of scope):** `graph_read_subgraph.rs` is now 742 lines (was already
   689 before my ~53-line change) — over the 500-line rule. The brief assumed it was under 500 ("no split
   needed"); it was not. I did not split (out of scope for this wave and risky mid-delivery). A follow-up
   module split is an architect/leader call.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` + `context_get` — surfaced ADR-001 vnc-043 (#5448,
  insertion point + load-bearing-path coverage), ADR-003 vnc-043 (#5450, uniform ordering + truncation),
  lesson #4562 (`use_fallback` under same lock guard — respected by placing the depth-1 return before the
  lock), ADR-005 vnc-018 (#4479, neighbors depth-1-live/depth>1-cache asymmetry mirrored).
- Deviations from established patterns: none.
- Stored: entry #5456 "Routing a cached-read graph path to live-SQL dispatch silently breaks
  in-memory-graph test fixtures" via `context_store` (category: pattern, topic: unimatrix-server).
