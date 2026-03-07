# ASS-017: petgraph Integration Analysis

**Issue:** #128
**Date:** 2026-03-07
**Status:** Complete

---

## 1. Primary Scenario: Graph-Derived Scoring Replacing Hardcoded Penalties

### Current State

crt-010 introduced two hardcoded constants in `unimatrix-engine/src/confidence.rs`:

```rust
pub const DEPRECATED_PENALTY: f64 = 0.7;   // line 52
pub const SUPERSEDED_PENALTY: f64 = 0.5;   // line 57
```

Applied multiplicatively to the re-rank score in `search.rs:204-206`:

```rust
if entry.superseded_by.is_some() {
    penalty_map.insert(entry.id, SUPERSEDED_PENALTY);  // 0.5x
} else if entry.status == Status::Deprecated {
    penalty_map.insert(entry.id, DEPRECATED_PENALTY);  // 0.7x
}
```

ADR-005 (crt-010) acknowledges these are "judgment calls, not empirically derived." ADR-003 explicitly defers multi-hop traversal.

### What petgraph Enables

Build a directed graph from existing store edges at query time (or cached with invalidation):

| Edge Type | Source Field | Current Traversal |
|-----------|-------------|-------------------|
| supersedes → superseded_by | `EntryRecord.supersedes`, `.superseded_by` | Single-hop lookup (ADR-003) |
| correction chains | `EntryRecord.correction_count`, store correction records | Count only, no traversal |
| co-access pairs | `CO_ACCESS` table | Flat pair lookup, no transitivity |
| feature → outcome | `FEATURE_ENTRIES` + `OUTCOME_INDEX` | Index scan per feature |

With petgraph, penalty becomes a **function of graph position** rather than a constant:

#### Proposed: Topology-Derived Penalty

```
penalty(node) = f(successor_depth, active_reachability, fan_out)
```

| Signal | Meaning | Effect |
|--------|---------|--------|
| **Successor count** | How many active entries supersede this one | More successors = more confidently outdated |
| **Active reachability** | Can you reach an Active node from this node via supersession edges | Reachable = clean replacement exists, harsher penalty |
| **Chain depth** | Position in supersession chain (A→B→C, A is depth 2) | Deeper = more outdated |
| **Fan-out** | Entry partially superseded by N entries vs fully by 1 | Partial supersession = softer penalty |
| **Leaf status** | Deprecated with no successors (orphan) | Different treatment — deprecated by age, not replacement |

Example scoring:

| Scenario | Current | Graph-Derived |
|----------|---------|---------------|
| Deprecated, no successor | 0.7x | ~0.75x (orphan, mild penalty) |
| Superseded by 1 active entry | 0.5x | ~0.4x (clean replacement, harsh) |
| Superseded by entry that's also superseded (A→B→C) | 0.5x (stops at B) | ~0.2x (A is 2 hops from active C) |
| Partially superseded (A split into B + C) | 0.5x | ~0.6x (partial, each successor covers subset) |
| Deprecated but still co-accessed with active entries | 0.7x | ~0.8x (graph says it's still contextually relevant) |

The key insight: **the same status (Deprecated) should produce different penalties depending on the entry's neighborhood.**

---

## 2. Other Scenarios Using Current Feature Set

### 2a. Multi-Hop Supersession Traversal

**Current limitation (ADR-003):** Single-hop only. Chain A→B→C where B is also superseded silently drops C.

**With petgraph:** `petgraph::algo::has_path_connecting` or DFS from any deprecated node to find the terminal active successor. Bounded by graph size, not arbitrary depth limit. Cycle detection comes free via `is_cyclic_directed`.

**Impact:** Successor injection in search (`search.rs:220+`) follows the full chain to the correct active entry.

### 2b. Connected Component Analysis for Coherence Gate

**Current:** `crt-005` coherence gate graph dimension (weight 0.30) uses basic metrics — correction chain counts, co-access pair counts.

**With petgraph:** `petgraph::algo::connected_components` reveals knowledge clustering. Isolated subgraphs indicate knowledge silos. Large connected components indicate well-linked knowledge. The graph dimension becomes a true topological metric rather than a count proxy.

### 2c. Co-Access Transitivity

**Current:** Co-access boost (`coaccess.rs`) operates on direct pairs only. If A↔B co-accessed and B↔C co-accessed, A gets no boost from C.

**With petgraph:** Build undirected co-access graph. Transitive boost with decay: direct pair gets full boost, 2-hop gets reduced boost. `petgraph::algo::dijkstra` with co-access count as edge weight gives distance-weighted transitivity.

**Risk:** Could amplify noise. Needs dampening factor and empirical validation.

### 2d. Contradiction Cluster Detection

**Current:** `crt-003` contradiction detection is pairwise — compares individual entries.

**With petgraph:** Build contradiction edges between entries. Connected components in contradiction graph reveal **contradiction clusters** — groups of entries that collectively disagree. More actionable than pairwise alerts.

### 2e. Impact Analysis for Deprecation

**Current:** Deprecating an entry has no awareness of downstream effects.

**With petgraph:** Before deprecating entry X, traverse: what entries reference X via supersedes? What entries are co-accessed with X? What outcomes link to X? Provides impact radius before mutation.

### 2f. Correction Chain Quality

**Current:** `correction_count` is a scalar on EntryRecord. Used as one of 6 confidence factors (W_CORR = 0.14).

**With petgraph:** Correction chains become traversable. An entry that's the terminal node of a long correction chain (many iterations to get right) might warrant different confidence treatment than one with a single correction. Chain length, branch count, and convergence (did corrections stabilize?) become computable.

---

## 3. Novel Capabilities

### 3a. Knowledge Decay Propagation

When an entry's confidence drops significantly, propagate partial decay to entries that depend on it (via supersedes, co-access, or feature links). Graph-aware cascade with dampening — not viral, but informed.

### 3b. Semantic Neighborhood Enrichment

For briefing context injection: instead of just top-k semantic matches, include the graph neighborhood of those matches. If entry A matches semantically and A is strongly connected to B via co-access/correction edges, B gets promoted even if its embedding similarity is lower.

### 3c. Knowledge Health Visualization

Export the graph via `petgraph::dot::Dot` for Graphviz rendering. Immediate visualization of knowledge topology without building a custom UI. Connected components, orphan clusters, supersession chains — all visible.

### 3d. Topological Ordering for Maintenance

`petgraph::algo::toposort` on supersession DAG gives processing order for batch confidence refresh (`crt-005` maintain). Process terminal entries first, propagate backward. Current batch refresh processes in arbitrary order.

### 3e. Cycle Detection as Integrity Check

`petgraph::algo::is_cyclic_directed` on supersession edges detects impossible states (A supersedes B supersedes A). Currently undetected — would silently cause infinite loops if multi-hop traversal were naively implemented.

---

## 4. Technical Integration Analysis

### 4a. Library Profile

| Property | Value |
|----------|-------|
| Crate | `petgraph` |
| Version | 0.8.3 |
| License | MIT / Apache-2.0 |
| Dependencies | `fixedbitset`, `indexmap` (both lightweight) |
| MSRV | Stable Rust (compatible with workspace edition 2024, MSRV 1.89) |
| Default features | `graphmap`, `stable_graph`, `matrix_graph`, `std` |
| Optional features | `serde-1`, `rayon`, `dot_parser`, `generate` |

### 4b. Recommended Configuration

```toml
[dependencies]
petgraph = { version = "0.8", default-features = false, features = ["stable_graph"] }
```

- `stable_graph` preferred over `graph` — index stability across node removal matters for entry deletion/quarantine
- Disable `graphmap`, `matrix_graph` — unused, reduces compile surface
- `serde-1` only if graph persistence is needed (unlikely — rebuild from store is preferred)
- `rayon` deferred unless parallel graph algorithms prove necessary

### 4c. Integration Point

**Target crate:** `unimatrix-engine`

petgraph would live alongside the existing confidence and co-access modules. The engine crate already depends on `unimatrix-store` for `Store` and `unimatrix-core` for `EntryRecord`. Graph construction reads from the store; graph queries feed into scoring.

```
unimatrix-store (edges in redb)
       ↓ reads
unimatrix-engine (petgraph in-memory graph + scoring)
       ↓ scores
unimatrix-server (search/briefing services)
```

### 4d. Graph Construction Strategy

**Option A: Build per-query** — Reconstruct graph from store on each search/briefing call.
- Pro: Always fresh. No cache invalidation.
- Con: Reads all supersession + co-access edges per query. At 500 entries + 400 co-access pairs, ~1-2ms. Grows linearly.

**Option B: Cached with invalidation** — Build once, invalidate on store mutations (store, correct, deprecate, quarantine).
- Pro: Amortized O(1) per query.
- Con: Cache invalidation complexity. Must handle concurrent readers.
- Implementation: `RwLock<StableGraph>` in engine, rebuilt on mutation signal.

**Recommendation:** Start with Option A. Current entry count (~500) makes per-query construction negligible. Move to Option B when profiling shows it matters.

### 4e. Risks and Negatives

| Risk | Severity | Mitigation |
|------|----------|------------|
| **Compile time increase** | Low | petgraph is lightweight (~3s incremental). Minimal feature set reduces surface. |
| **New dependency** | Low | Well-maintained (9 years, 130M+ downloads), MIT/Apache-2.0, pure Rust, no unsafe in core. |
| **Memory overhead** | Low | StableGraph at 500 nodes + 1000 edges ≈ <100KB. Negligible vs HNSW index. |
| **Complexity creep** | Medium | Graph algorithms are easy to over-apply. Discipline needed to justify each use. Start with supersession traversal only, expand based on measured value. |
| **Cache invalidation (Option B)** | Medium | Deferred. Option A avoids this entirely for v1. |
| **Graph staleness** | Low | Option A eliminates this. Option B needs mutation hooks — already exist in server (fire-and-forget confidence recompute pattern). |
| **Testing surface** | Medium | Graph construction and scoring need integration tests with realistic topologies. Existing test infra supports this — extend, don't create new scaffolding. |
| **API stability** | Low | petgraph 0.8.x is stable. No breaking changes expected in patch releases. |

### 4f. What petgraph Does NOT Solve

- **Empirical penalty calibration** — Graph topology tells you *relative* severity, not absolute values. The base penalty scale still needs tuning (or A/B testing with user feedback).
- **Embedding-space awareness** — petgraph operates on structural edges, not semantic similarity. It complements the vector index, doesn't replace it.
- **Real-time updates** — Graph is a snapshot. If an entry is deprecated mid-query, the in-flight graph won't reflect it (same as current behavior — not a regression).

---

## 5. Recommendation

**Integrate petgraph into `unimatrix-engine`** with minimal feature set (`stable_graph` only).

### Phase 1: Supersession Graph (replaces hardcoded penalties)
- Build directed supersession graph from `supersedes`/`superseded_by` fields
- Replace `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` constants with topology-derived penalty function
- Enable multi-hop successor resolution (replaces ADR-003 single-hop limit)
- Add cycle detection as integrity check
- **Estimated scope:** ~200-300 lines in engine, ~50 lines in server wiring

### Phase 2: Co-Access Graph (enhances existing boost)
- Build undirected co-access graph from `CO_ACCESS` table
- Explore transitive boost with dampening
- Connected component analysis for coherence gate graph dimension
- **Gated on:** Phase 1 validated, empirical signal quality confirmed

### Phase 3: Unified Knowledge Graph (novel capabilities)
- Merge supersession + co-access + correction edges into single graph
- Semantic neighborhood enrichment for briefing
- Knowledge decay propagation
- Graphviz export for visualization
- **Gated on:** Phase 2 validated, entry count > 1000

---

## 6. Open Questions

1. Should the graph include deprecated/quarantined entries? (Yes for traversal, excluded from results — same as current vector index behavior)
2. Should graph construction be synchronous or async? (Sync in engine, async wrapper in server — matches existing Store pattern)
3. Should the penalty function be configurable per-query or fixed? (Fixed for v1, parameterizable later if needed)
