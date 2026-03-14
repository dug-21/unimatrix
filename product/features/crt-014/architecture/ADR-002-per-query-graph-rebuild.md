## ADR-002: Per-Query Graph Rebuild (Option A)

### Context

The supersession DAG can be constructed in two ways:

**Option A — Per-query rebuild**: Reconstruct the `StableGraph` from store data on each `context_search` call that reaches Step 6 (penalty marking). Reads all entries via `Store::query(QueryFilter::default())`, builds the graph, uses it for penalty computation and successor injection, then drops it.

**Option B — Cached with invalidation**: Build once, store in a `RwLock<SupersessionGraph>` field on `SearchService`, rebuild on store mutations (store_ops, correct, deprecate, quarantine).

ASS-017 estimated ~1-2ms for Option A at current entry count (~500), growing linearly. Option B is O(1) amortized per query but introduces cache invalidation complexity — mutation hooks, concurrent reader handling, and staleness windows between mutations and graph updates.

The human explicitly chose Option A in the design session (OQ-2 answer): "Build over ALL entries in the store, not just search candidates."

### Decision

Use Option A: per-query rebuild for crt-014.

Implement `build_supersession_graph(&[EntryRecord])` as a pure synchronous function taking a pre-loaded entry slice. The caller (search.rs) is responsible for fetching all entries from the store before calling the function. Graph construction happens inside or adjacent to the existing `spawn_blocking` pattern in the search pipeline.

Add a comment in `search.rs` marking the full-store read and graph build as "Option A — per-query rebuild, see ADR-002 (crt-014); upgrade to Option B if profiling shows regression."

### Consequences

Easier: No cache invalidation logic. No `RwLock`. Always-fresh graph. Entry deletion, quarantine, and correction events immediately reflected on next query. Simpler testing — no need to mock cache state.

Harder: Every search call with deprecated/superseded results incurs a full-store read + graph construction. At current scale this is negligible (~1-2ms). If entry count exceeds ~5,000 or query rate becomes very high, Option B becomes necessary. The comment in search.rs documents the upgrade path.
