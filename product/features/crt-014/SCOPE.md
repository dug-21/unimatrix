# crt-014: Topology-Aware Supersession

## Problem Statement

Unimatrix's knowledge retrieval penalizes outdated entries (deprecated/superseded) using two hardcoded scalar constants introduced in crt-010:

- `DEPRECATED_PENALTY = 0.7` — applied when `entry.status == Status::Deprecated`
- `SUPERSEDED_PENALTY = 0.5` — applied when `entry.superseded_by.is_some()`

ADR-005 (crt-010) explicitly acknowledged these are "judgment calls, not empirically derived." They treat all deprecated/superseded entries identically regardless of their position in the supersession graph, producing incorrect penalty ordering:

| Scenario | Current Penalty | Correct Behavior |
|----------|----------------|-----------------|
| Deprecated with no successor (orphan) | 0.7x | Softer — no known replacement |
| Superseded by 1 active entry (A→B) | 0.5x | Harsh — clean replacement exists |
| 2-hop outdated (A→B→C, A queried) | 0.5x (stops at B) | Harsher + follow to C |
| Partially superseded (A→B + A→C) | 0.5x | Softer — each successor covers a subset |

ADR-003 further imposed a single-hop successor injection limit: chain A→B→C silently stops at B, injecting B even if B is also superseded. This means search can inject an already-outdated successor instead of following the chain to the correct active terminal node C.

There is also no integrity check for supersession cycles (A supersedes B supersedes A), which would cause infinite loops if multi-hop traversal were naively implemented.

These limitations are the direct motivation for the Graph Enablement milestone in the product vision.

## Goals

1. Add `petgraph` (with `stable_graph` feature only) to `unimatrix-engine` as a first-class dependency.
2. Implement `graph.rs` in `unimatrix-engine/src/` — builds a directed supersession DAG per-query from `EntryRecord.supersedes`/`superseded_by` fields.
3. Replace `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` constants with a `graph_penalty(node_id, graph) -> f64` function — topology-derived from successor count, chain depth, active reachability, and fan-out.
4. Enable full multi-hop successor resolution in `search.rs` — chains A→B→C now follow to the terminal active node C; `search.rs:251` comment "Single-hop only (ADR-003)" is removed.
5. Add `is_cyclic_directed` integrity check in graph construction — returns an error if a cycle is detected in the supersession edges.
6. Supersede ADR-003 and ADR-005 with updated decisions.

## Non-Goals

- **Phase 2 (Co-Access Graph):** Transitive co-access boost, connected component analysis for coherence gate — deferred to crt-015 or equivalent. crt-014 builds supersession graph only.
- **Phase 3 (Unified Knowledge Graph):** Correction chain traversal, knowledge decay propagation, semantic neighborhood enrichment, Graphviz export — deferred.
- **Graph caching (Option B):** crt-014 uses Option A (per-query rebuild). No `RwLock<StableGraph>` cache, no mutation hooks.
- **Empirical penalty calibration:** The topology-derived formula provides relative severity. Absolute scale tuning via A/B testing or user feedback is a separate concern.
- **Penalty configurability per-query:** The `graph_penalty` function uses fixed parameters in v1.
- **co-access boost changes:** `coaccess.rs` is unchanged by crt-014.
- **Graphviz / DOT export:** Visualization is a Phase 3 concern.

## Background Research

### ASS-017 Findings (product/research/ass-017/ANALYSIS.md)

Complete petgraph integration analysis confirmed:

- **Recommended config:** `petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }` — `stable_graph` preferred over `graph` because index stability across node removal (entry deletion/quarantine) matters.
- **Build strategy:** Option A (per-query rebuild) at current entry count (~500 entries, ~400 co-access pairs) costs ~1-2ms — negligible. Option B (cached with `RwLock`) deferred until profiling shows it matters.
- **petgraph profile:** 9 years maintained, 130M+ downloads, MIT/Apache-2.0, pure Rust, no unsafe in core. Dependencies: `fixedbitset`, `indexmap` (both lightweight).
- **Integration point:** `unimatrix-engine` — alongside existing `confidence.rs` and `coaccess.rs` modules. Engine already depends on `unimatrix-store` and `unimatrix-core`.
- **Risks:** Complexity creep (graph algorithms over-applied — discipline needed), testing surface (graph construction + scoring need integration tests).

### Codebase Audit

**Penalty constants** (`crates/unimatrix-engine/src/confidence.rs`, lines 52–57):
```rust
pub const DEPRECATED_PENALTY: f64 = 0.7;
pub const SUPERSEDED_PENALTY: f64 = 0.5;
```
Imported in `crates/unimatrix-server/src/services/search.rs:15`.

**Penalty application** (`search.rs:190–211`):
- `penalty_map: HashMap<u64, f64>` built before re-ranking
- Single-hop successor injection at `search.rs:219–259`; comment at line 251: `"Single-hop only (ADR-003, AC-06)"`
- Penalty applied multiplicatively during sort at lines ~278, ~343, ~369

**EntryRecord fields** (`unimatrix-store/src/schema.rs:67–69`):
```rust
pub supersedes: Option<u64>,
pub superseded_by: Option<u64>,
```
Both `Option<u64>` — single successor ID only. Multi-hop requires graph traversal.

**Existing tests for penalty constants** (`confidence.rs:720–752`): Four tests assert exact constant values and ordering — these will need to be updated or removed when constants are replaced with the `graph_penalty` function.

**Search tests** (`search.rs:450–571`): Tests reference `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` directly — will need updating.

### ADR-003 and ADR-005 (Unimatrix knowledge store)

- ADR-003: Established single-hop supersession limit. Rationale: multi-hop traversal requires cycle detection infrastructure not yet in place. crt-014 provides that infrastructure.
- ADR-005: Established hardcoded penalty constants. Rationale: topology-derived scoring requires petgraph not yet added. crt-014 adds petgraph.

Both decisions are superseded by crt-014. New ADRs will be issued.

## Proposed Approach

### New Module: `crates/unimatrix-engine/src/graph.rs`

Builds a `petgraph::stable_graph::StableGraph<u64, ()>` (directed) from a slice of `EntryRecord`s:

```
build_supersession_graph(entries: &[EntryRecord]) -> Result<SupersessionGraph, CycleError>
```

Graph construction:
1. Add a node for each entry (keyed by `entry.id`)
2. For each entry with `supersedes: Some(pred_id)`: add directed edge `pred_id → entry.id`
3. Run `petgraph::algo::is_cyclic_directed` — return `Err(CycleError)` if cycle detected
4. Return the graph

`graph_penalty(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> f64`:
- Find the node in the graph
- If not present: return `1.0` (no penalty — unknown topology)
- Compute topology signals:
  - **active_reachable**: DFS from node — can an Active, non-superseded entry be reached? (bool)
  - **chain_depth**: shortest path distance to the nearest Active terminal (0 = is terminal)
  - **successor_count**: number of direct successors in graph
  - **is_orphan**: `status == Deprecated` and no successors
- Derive penalty:
  - Orphan deprecated (no successors): `~0.75` (softer than current 0.7)
  - Active reachable, depth 1: `~0.4` (clean replacement, harsher than current 0.5)
  - Active reachable, depth 2+: `0.4 * 0.6^(depth-1)` (additional 0.6x per extra hop)
  - Partial supersession (successor_count > 1, all cover subsets): `~0.6` (softer)
  - No active reachable (dead-end deprecated chain): `~0.65`

`find_terminal_active(node_id: u64, graph: &SupersessionGraph, entries: &[EntryRecord]) -> Option<u64>`:
- DFS/BFS from `node_id` following directed edges
- Return the first Active, non-superseded entry found
- Used by `search.rs` for multi-hop successor injection

### Changes: `crates/unimatrix-server/src/services/search.rs`

1. Remove import of `DEPRECATED_PENALTY`, `SUPERSEDED_PENALTY`
2. Before penalty marking (Step 6a): call `build_supersession_graph` over all candidate entries
3. Replace `penalty_map.insert(entry.id, SUPERSEDED_PENALTY/DEPRECATED_PENALTY)` with `penalty_map.insert(entry.id, graph_penalty(entry.id, &graph, &all_entries))`
4. Replace single-hop successor injection logic with `find_terminal_active(entry.id, &graph, &all_entries)` — follow to terminal node, not just `entry.superseded_by`

### Changes: `crates/unimatrix-engine/Cargo.toml`

Add:
```toml
petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }
```

### Constant Deprecation

`DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` in `confidence.rs` are removed (or marked `#[deprecated]` pending test migration). `graph_penalty` replaces their role.

## Acceptance Criteria

- AC-01: `petgraph` with `stable_graph` feature is added to `unimatrix-engine/Cargo.toml`; workspace builds without warnings.
- AC-02: `crates/unimatrix-engine/src/graph.rs` exists and is public from the engine crate.
- AC-03: `build_supersession_graph` returns `Err(CycleError)` when given entries with cyclic `supersedes`/`superseded_by` references.
- AC-04: `build_supersession_graph` returns `Ok(graph)` for valid DAGs including chains of depth 1, 2, and 3+.
- AC-05: `graph_penalty` returns a value in `(0.0, 1.0)` for all valid inputs.
- AC-06: Orphan deprecated entry (no successors) receives a softer penalty than a superseded entry with an active terminal successor.
- AC-07: A 2-hop outdated entry (A→B→C where C is active) receives a harsher penalty than a 1-hop outdated entry (A→B where B is active).
- AC-08: A partially-superseded entry (A→B and A→C where B, C are active) receives a softer penalty than a fully-superseded single-successor entry.
- AC-09: `find_terminal_active` returns `Some(C)` for chain A→B→C where B is also superseded and C is active.
- AC-10: `find_terminal_active` returns `None` for a node with no active terminal reachable.
- AC-11: In `search.rs`, successor injection follows to the terminal active node (multi-hop), not just `entry.superseded_by` (single-hop).
- AC-12: In `search.rs` Flexible mode, `penalty_map` is populated via `graph_penalty`, not the removed constants.
- AC-13: Existing search tests pass with updated penalty logic (values may differ from crt-010 baselines; assertions updated to reflect topology-derived values).
- AC-14: `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` constants are removed from `confidence.rs`; no remaining references in non-test code.
- AC-15: New ADRs superseding ADR-003 and ADR-005 are stored in Unimatrix (via `context_store`).
- AC-16: Integration tests covering cycle detection, multi-hop traversal, and penalty ordering exist in `unimatrix-engine`.

## Constraints

- **petgraph feature surface:** `stable_graph` only. No `graphmap`, `matrix_graph`, `serde-1`, `rayon`, `dot_parser`, or `generate` features.
- **No graph persistence:** Graph is always rebuilt from store data per query. No serialization of `StableGraph`.
- **No async in graph module:** `graph.rs` is pure sync (no I/O). The calling async service wraps it via `spawn_blocking` if needed (matching existing engine patterns).
- **No schema changes:** `supersedes`/`superseded_by` remain `Option<u64>` single-link fields. Multi-hop traversal is purely a graph traversal over existing links, not a schema change.
- **Test infrastructure is cumulative:** Extend existing fixtures and helpers; no isolated test scaffolding.
- **Workspace Rust edition 2024, MSRV 1.89:** petgraph 0.8.x is compatible.
- **Affected crates:** `unimatrix-engine` (Cargo.toml + new graph.rs + confidence.rs changes), `unimatrix-server` (search.rs).

## Open Questions

1. **Penalty formula parameters:** The exact coefficients for `graph_penalty` (e.g., the 0.6x per-hop decay factor, the 0.75 orphan penalty) are proposed values from ASS-017. Should the architect treat these as fixed design decisions or leave them as tunable parameters for the implementation phase?

2. **Graph construction scope:** `build_supersession_graph` is called over the search candidate entries (those returned from the vector index). Should it be built over ALL entries in the store (for accurate depth/reachability computation across the full graph), or just the candidate set (cheaper, but may miss distant connections)? The product vision says "~1-2ms at current entry count" — this implies full-store graph, which needs clarification on the read pattern.

3. **`confidence.rs` constant removal:** Tests in `confidence.rs` (T-PC-01..04) and `search.rs` assert exact values of the removed constants. Should these tests be removed entirely, or replaced with behavioral assertions against `graph_penalty` output?

4. **Error propagation in search:** If `build_supersession_graph` returns `Err(CycleError)` during a live search, should it: (a) fall back to the old constant penalties, (b) return an error to the caller, or (c) log and skip the penalty step? The spec needs to define the fallback behavior.

## Tracking

GH Issue: https://github.com/dug-21/unimatrix/issues/260
