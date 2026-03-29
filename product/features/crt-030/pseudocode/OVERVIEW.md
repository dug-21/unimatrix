# crt-030 Pseudocode Overview — Personalized PageRank for Multi-Hop Relevance Propagation

## Components Covered

| Component | File | Pseudocode |
|-----------|------|------------|
| PPR pure function | `crates/unimatrix-engine/src/graph_ppr.rs` (new) | `graph_ppr.md` |
| Config extension | `crates/unimatrix-server/src/infra/config.rs` (modify) | `config_ppr_fields.md` |
| Pipeline Step 6d | `crates/unimatrix-server/src/services/search.rs` (modify) | `search_step_6d.md` |

Also requires: `graph.rs` gets two lines added (`mod graph_ppr;` + `pub use graph_ppr::personalized_pagerank;`).
Those two lines are trivial; their exact placement is documented in `graph_ppr.md`.

---

## Data Flow Between Components

```
[search.rs Step 6d]
  │
  ├─ reads: typed_graph: TypedRelationGraph (cloned at line 638, before Step 6a)
  │         type: crates/unimatrix-engine/src/graph.rs::TypedRelationGraph
  │
  ├─ reads: phase_snapshot: Option<HashMap<String, Vec<(u64, f32)>>>
  │         extracted by col-031 pre-loop block before Step 7
  │         absent → cold-start, affinity = 1.0
  │
  ├─ reads: cfg: InferenceConfig fields
  │         ppr_alpha, ppr_iterations, ppr_inclusion_threshold,
  │         ppr_blend_weight, ppr_max_expand
  │         source: crates/unimatrix-server/src/infra/config.rs
  │
  ├─ reads: use_fallback: bool (from TypedGraphState clone at line 638)
  │         when true: skip Step 6d entirely
  │
  ├─ mutates: results_with_scores: Vec<(EntryRecord, f64)>
  │           blends scores of existing HNSW entries
  │           appends new PPR-only entries (quarantine-checked)
  │
  └─ calls ──►  personalized_pagerank(
                    graph:      &TypedRelationGraph,
                    seed_scores: &HashMap<u64, f64>,   // normalized to sum 1.0
                    alpha:       f64,                   // from cfg.ppr_alpha
                    iterations:  usize                  // from cfg.ppr_iterations
                )
                returns: HashMap<u64, f64>  // entry_id → PPR score
                         empty if seed_scores is empty/all-zero
```

```
[graph_ppr.rs :: personalized_pagerank]
  │
  ├─ reads: graph.node_index: HashMap<u64, NodeIndex>   (sorted keys → Vec<u64>)
  ├─ reads: graph.inner[node_idx]: u64                  (node weight = entry id)
  │
  └─ traverses via edges_of_type() ONLY (AC-02):
       graph.edges_of_type(node_idx, RelationType::Supports,     Direction::Incoming)
       graph.edges_of_type(node_idx, RelationType::CoAccess,     Direction::Incoming)
       graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Incoming)
       reads: edge.weight(): &RelationEdge → weight: f32 → as f64
```

---

## Shared Types (no changes — all existing)

| Type | Location | Role in crt-030 |
|------|----------|-----------------|
| `TypedRelationGraph` | `unimatrix-engine/src/graph.rs:167` | Passed to PPR function by reference |
| `RelationType` | `unimatrix-engine/src/graph.rs:71` | Edge filter enum: Supports, CoAccess, Prerequisite |
| `RelationEdge` | `unimatrix-engine/src/graph.rs:116` | Edge weight via `.weight: f32` |
| `NodeIndex` | `petgraph::stable_graph::NodeIndex` | Node handle in StableGraph |
| `Direction` | `petgraph::Direction` | Incoming for all three PPR edge types |
| `EntryRecord` | `unimatrix-core` | Fetched in Step 6d for PPR-only expansion |
| `Status` | `unimatrix-core` | Quarantine check in Step 6d expansion |
| `SecurityGateway::is_quarantined` | `unimatrix-server/src/infra/security.rs` | Applied to every PPR-only entry fetched |
| `InferenceConfig` | `unimatrix-server/src/infra/config.rs` | Source of five new PPR config fields |

New types introduced by crt-030:
- None. PPR score map is `HashMap<u64, f64>` (standard library).

---

## Sequencing Constraints (Build Order)

1. `graph_ppr.rs` — pure function, zero external dependencies beyond existing `graph.rs` types.
   Can be implemented and tested independently.

2. `config.rs` PPR fields — independent of graph code. Adds five fields to `InferenceConfig`;
   no dependency on `graph_ppr.rs`.

3. `search.rs` Step 6d — depends on both (1) and (2):
   - Calls `personalized_pagerank` from (1).
   - Reads `cfg.ppr_alpha`, `cfg.ppr_iterations`, `cfg.ppr_inclusion_threshold`,
     `cfg.ppr_blend_weight`, `cfg.ppr_max_expand` from (2).

4. `graph.rs` two-line modification — must be done alongside (1) so `personalized_pagerank`
   is visible to `search.rs`. Not a separate component; described in `graph_ppr.md`.

---

## Pipeline Step Order (crt-030 insertion point)

```
Step 5:   HNSW search
Step 6:   Fetch entries, quarantine filter (existing)
Step 6a:  Status penalty marking (existing)
Step 6b:  Supersession candidate injection (existing)
──────────────────────────────────────────────────────
Step 6d:  PPR expansion (NEW — crt-030)              ← INSERTION POINT
──────────────────────────────────────────────────────
Step 6c:  Co-access boost prefetch (existing, now over full expanded pool)
Step 7:   NLI scoring + fused score computation + sort + truncate (existing)
Step 9:   Truncate to k (existing)
Step 10:  Floors (existing)
Step 10b: Contradicts collision suppression (existing)
```

ADR-005 is authoritative: 6d inserts after 6b and before 6c so PPR-surfaced entries
participate in co-access boost prefetch and NLI scoring.

---

## Critical Constraints Summary

| Constraint | Component | Risk |
|------------|-----------|------|
| `edges_of_type` exclusively — no `.edges_directed()` calls in `graph_ppr.rs` | graph_ppr | R-09 / AC-02 |
| Node-ID-sorted Vec constructed ONCE before iteration loop | graph_ppr | R-04 / ADR-004 |
| `use_fallback = true` skips Step 6d entirely, zero allocation | search_step_6d | R-02 / AC-12 |
| Every PPR-only entry fetched in Step 6d must pass quarantine check | search_step_6d | R-08 (Critical) |
| Inclusion threshold comparison is strictly `>`, not `>=` | search_step_6d | R-06 / AC-13 |
| Personalization vector from phase_snapshot — no lock re-acquisition | search_step_6d | R-10 / ADR-006 |
| Step 6c runs over the full expanded pool (after Step 6d) | search_step_6d | ADR-005 |
