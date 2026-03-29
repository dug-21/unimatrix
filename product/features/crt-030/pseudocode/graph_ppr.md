# graph_ppr.rs — Personalized PageRank Pure Function

## Purpose

Implement `personalized_pagerank` as a pure, synchronous, deterministic function over
`TypedRelationGraph`. Propagates relevance mass from seed entries through positive-edge
chains (Supports, CoAccess, Prerequisite) using power iteration.

This module mirrors `graph_suppression.rs` (col-030) in every structural convention:
- Declared as `#[path = "graph_ppr.rs"] mod graph_ppr;` inside `graph.rs`
- Re-exported: `pub use graph_ppr::personalized_pagerank;` from `graph.rs`
- Does NOT appear in `lib.rs`
- Unit tests inline in this file (or split to `graph_ppr_tests.rs` if > 500 lines)

---

## graph.rs Modification (two lines only)

In `crates/unimatrix-engine/src/graph.rs`, after the existing `graph_suppression` block:

```
// After line 28 (existing graph_suppression declaration):
#[path = "graph_ppr.rs"]
mod graph_ppr;
pub use graph_ppr::personalized_pagerank;
```

These two declarations are the entire change to `graph.rs`. No other modification needed.

---

## New Function: `personalized_pagerank`

### Signature

```
pub fn personalized_pagerank(
    graph: &TypedRelationGraph,
    seed_scores: &HashMap<u64, f64>,
    alpha: f64,
    iterations: usize,
) -> HashMap<u64, f64>
```

### Doc Comment (verbatim — AC-04 requirement)

```
/// Compute Personalized PageRank over positive edges (Supports, CoAccess, Prerequisite).
///
/// SR-01 constrains `graph_penalty` and `find_terminal_active` to Supersedes-only
/// traversal; it does not restrict new retrieval functions from using other edge types.
/// PPR uses Supports, CoAccess, and Prerequisite only.
///
/// `seed_scores` must be pre-normalized to sum 1.0 (caller responsibility).
/// The function does NOT re-normalize internally.
///
/// Returns an empty HashMap if `seed_scores` is empty.
/// Runs exactly `iterations` steps — no early-exit convergence check (determinism).
///
/// All graph traversal uses `edges_of_type()` exclusively (AC-02, SR-01 boundary).
/// Direct `.edges_directed()` or `.neighbors_directed()` calls are prohibited here.
```

### Imports Required

```
use std::collections::HashMap;
use petgraph::Direction;
use petgraph::visit::EdgeRef;
use crate::graph::{RelationType, TypedRelationGraph};
```

### Algorithm Pseudocode

```
FUNCTION personalized_pagerank(graph, seed_scores, alpha, iterations) -> HashMap<u64, f64>

    -- Zero-sum guard (FR-01): empty seed map → return immediately
    IF seed_scores.is_empty() THEN
        RETURN HashMap::new()
    END IF

    -- ADR-004: sort all node IDs ascending ONCE before the iteration loop.
    -- This Vec is constructed HERE, never inside the loop.
    -- Covers ALL nodes in the graph (not just seeds) for correct deterministic order.
    let all_node_ids: Vec<u64> =
        graph.node_index.keys().copied().collect()
    all_node_ids.sort_unstable()   -- ascending u64 order

    -- Initialize current score map: seed nodes get their personalization mass;
    -- all other nodes start at 0.0.
    let current_scores: HashMap<u64, f64> =
        HashMap::with_capacity(all_node_ids.len())
    FOR node_id IN all_node_ids DO
        let score = seed_scores.get(node_id).copied().unwrap_or(0.0)
        current_scores.insert(node_id, score)
    END FOR

    -- Power iteration: exactly `iterations` steps, no early exit
    FOR _ IN 0..iterations DO
        let next_scores: HashMap<u64, f64> =
            HashMap::with_capacity(current_scores.len())

        -- Accumulate contributions in sorted node-ID order (correctness constraint, ADR-004)
        FOR &node_id IN &all_node_ids DO
            let node_idx = match graph.node_index.get(node_id)
                SOME(idx) => idx
                NONE      => CONTINUE  -- node absent from index (should not occur)
            END match

            -- Teleportation term: (1 - alpha) * personalization[v]
            let teleport = (1.0 - alpha) * seed_scores.get(node_id).copied().unwrap_or(0.0)

            -- Neighbor contribution term:
            --   alpha * SUM_{u: u→node_id ∈ positive_edges}
            --           (current_scores[u] * edge_weight(u, node_id) / positive_out_degree(u))
            --
            -- Traverse Direction::Incoming on this node to reach sources u such that u → node_id.
            -- This is done via three separate edges_of_type calls (AC-02).
            let neighbor_contribution: f64 = 0.0

            FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::Supports, Direction::Incoming) DO
                neighbor_contribution += incoming_contribution(graph, &current_scores, &edge_ref)
            END FOR
            FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::CoAccess, Direction::Incoming) DO
                neighbor_contribution += incoming_contribution(graph, &current_scores, &edge_ref)
            END FOR
            FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Incoming) DO
                neighbor_contribution += incoming_contribution(graph, &current_scores, &edge_ref)
            END FOR

            next_scores.insert(node_id, teleport + alpha * neighbor_contribution)
        END FOR

        current_scores = next_scores
    END FOR

    RETURN current_scores
END FUNCTION
```

---

## Helper Function: `incoming_contribution`

Used inside the iteration loop to compute one source node's contribution to a target node.
Inline as a private helper (not a public API).

### Pseudocode

```
FUNCTION incoming_contribution(
    graph: &TypedRelationGraph,
    current_scores: &HashMap<u64, f64>,
    edge_ref: &EdgeReference<RelationEdge>
) -> f64

    -- Source node (u): the node the edge comes FROM (Incoming direction)
    let source_idx = edge_ref.source()
    let source_id  = graph.inner[source_idx]   -- node weight = entry ID

    -- Edge weight: f32 → f64 cast (RelationEdge.weight)
    let edge_weight = edge_ref.weight().weight as f64

    -- Current score of source
    let source_score = current_scores.get(source_id).copied().unwrap_or(0.0)
    IF source_score == 0.0 THEN
        RETURN 0.0   -- early exit: no mass to propagate
    END IF

    -- Out-degree normalization: positive_out_degree_weight of source
    let out_degree = positive_out_degree_weight(graph, source_idx)
    IF out_degree == 0.0 THEN
        RETURN 0.0   -- zero positive out-degree: no propagation (FR-05)
    END IF

    RETURN source_score * edge_weight / out_degree
END FUNCTION
```

---

## Helper Function: `positive_out_degree_weight`

Computes the sum of outgoing Supports + CoAccess + Prerequisite edge weights from a node.
Used for normalization in PPR. Private to `graph_ppr.rs`.

### Signature

```
fn positive_out_degree_weight(graph: &TypedRelationGraph, node_idx: NodeIndex) -> f64
```

### Pseudocode

```
FUNCTION positive_out_degree_weight(graph, node_idx) -> f64

    let total: f64 = 0.0

    -- Three outgoing edge-type queries (AC-02: edges_of_type only)
    FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::Supports, Direction::Outgoing) DO
        total += edge_ref.weight().weight as f64
    END FOR
    FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::CoAccess, Direction::Outgoing) DO
        total += edge_ref.weight().weight as f64
    END FOR
    FOR edge_ref IN graph.edges_of_type(node_idx, RelationType::Prerequisite, Direction::Outgoing) DO
        total += edge_ref.weight().weight as f64
    END FOR

    RETURN total   -- 0.0 if no positive out-edges
END FUNCTION
```

Notes:
- Uses `Direction::Outgoing` because we want the out-degree of the SOURCE node (u),
  not the incoming edges to it.
- Returns 0.0 for nodes with no positive out-edges. Callers guard on `== 0.0`.
- Supersedes and Contradicts edges are excluded by construction (no edges_of_type call
  for those types here).

---

## State Machines / Initialization

No state. `personalized_pagerank` is a pure function:
- No `self`
- No `mut` external state
- All allocations are local and dropped on return

---

## Error Handling

| Condition | Handling |
|-----------|----------|
| `seed_scores.is_empty()` | Return empty `HashMap` immediately (no iteration) |
| Node ID present in `all_node_ids` but absent from `graph.node_index` | `continue` (defensive; should not occur — both built from the same `node_index`) |
| Source node absent from `current_scores` during accumulation | `.unwrap_or(0.0)` — contributes 0.0 mass |
| `positive_out_degree_weight == 0.0` | Return 0.0 from `incoming_contribution` — no propagation |
| `edge_ref.weight().weight` is NaN or Inf | Protected upstream: `build_typed_relation_graph` stores validated finite weights |

No `Result` return type. Degenerate inputs (empty graph, no positive edges, all-zero scores)
produce empty or zero-filled output maps — no panics.

---

## Key Test Scenarios

These scenarios correspond to risks and acceptance criteria. The tester agent translates
them into `#[test]` functions in `graph_ppr_tests.rs` (if tests overflow `graph_ppr.rs`)
or inline in `graph_ppr.rs`.

### T-PPR-01: Empty seed scores → empty return (FR-01 / E-01)
```
graph = build_typed_relation_graph with 3 entries, 2 Supports edges
seed_scores = HashMap::new()
result = personalized_pagerank(graph, seed_scores, 0.85, 20)
ASSERT result.is_empty()
```

### T-PPR-02: Empty graph → empty return (E-01)
```
graph = TypedRelationGraph::empty()
seed_scores = {42: 1.0}
result = personalized_pagerank(graph, seed_scores, 0.85, 20)
ASSERT result.is_empty()   // no nodes in graph.node_index
```

### T-PPR-03: Single seed, no positive edges — teleportation only (E-03)
```
graph with one node id=1, no edges
seed_scores = {1: 1.0}
result = personalized_pagerank(graph, seed_scores, 0.85, 20)
ASSERT result[1] is finite and > 0.0
ASSERT result.len() == 1
ASSERT result[1] is close to 1.0 (only teleportation, no diffusion)
```

### T-PPR-04: Supports edge — seed B surfaces A (FR-04 / ADR-003)
```
graph: entry A (id=1), entry B (id=2)
       Supports edge: A→B (source=1, target=2)
seed_scores = {2: 1.0}   // B is the seed
result = personalized_pagerank(graph, seed_scores, 0.85, 5)
ASSERT result[1] > 0.0   // A receives mass from B via Incoming traversal on B
ASSERT result[2] > 0.0   // B retains some mass (teleportation)
```

### T-PPR-05: Zero positive out-degree node does not propagate (FR-05 / R-07 / AC-07)
```
graph: entries A, B, C
       Supersedes edge A→B (not a positive edge)
       B is seed
result = personalized_pagerank(...)
ASSERT result[A] == 0.0 or near 0.0 (A has no positive out-edges, teleportation only)
```

### T-PPR-06: CoAccess bidirectional — both neighbors receive mass (FR-04)
```
graph: entries A, B
       CoAccess A→B, CoAccess B→A (bidirectional, as stored in production)
seed_scores = {A: 1.0}
result = personalized_pagerank(graph, seed_scores, 0.85, 5)
ASSERT result[B] > 0.0   // B receives via A's outgoing → B's Incoming
ASSERT result[A] > 0.0
```

### T-PPR-07: Prerequisite direction — seed B surfaces A (R-12 / ADR-003)
```
graph: entries A, B
       Prerequisite edge A→B (source=1, target=2): "B requires A"
seed_scores = {2: 1.0}   // B is seed
result = personalized_pagerank(graph, seed_scores, 0.85, 5)
ASSERT result[1] > 0.0   // A surfaces as prerequisite of B
```

### T-PPR-08: Supersedes edge excluded — seed does not propagate to Supersedes target (R-09 / AC-02)
```
graph: entries A, B, C
       Supersedes edge A→B (not a positive edge)
       Supports edge A→C
seed_scores = {A: 1.0}
result = personalized_pagerank(graph, seed_scores, 0.85, 5)
// B receives no mass via A (Supersedes excluded)
// If B has no other positive in-edges, result[B] comes from teleportation only (0.0 if not in seed)
ASSERT result.get(B).copied().unwrap_or(0.0) == 0.0  // no PPR mass via Supersedes
```

### T-PPR-09: Contradicts edge excluded (R-09)
```
graph: entries A, B
       Contradicts edge A→B
seed_scores = {A: 1.0}
result = personalized_pagerank(graph, seed_scores, 0.85, 5)
ASSERT result.get(B).copied().unwrap_or(0.0) == 0.0
```

### T-PPR-10: Determinism — same inputs, two calls, identical output (ADR-004 / AC-05)
```
graph: 5 nodes, Supports and CoAccess edges
seed_scores = {1: 0.5, 2: 0.3, 3: 0.2}
result1 = personalized_pagerank(graph, seed_scores, 0.85, 20)
result2 = personalized_pagerank(graph, seed_scores, 0.85, 20)
ASSERT result1 == result2   // exact equality, not approximate
```

### T-PPR-11: Timing — 10K nodes, 20 iterations completes < 5ms (R-04 / NFR-01)
```
graph: 10K nodes, ~50K positive edges (simulated)
seed_scores: 10 entries normalized
let start = Instant::now()
let _ = personalized_pagerank(graph, seed_scores, 0.85, 20)
let elapsed = start.elapsed()
ASSERT elapsed < Duration::from_millis(5)
```

### T-PPR-12: Dense CoAccess graph timing (R-13)
```
graph: 50 nodes, each CoAccess-connected to every other (2450 edges)
seed_scores: 3 entries normalized
ASSERT personalized_pagerank completes within 1ms
```

### T-PPR-13: all scores finite and in [0.0, 1.0] for realistic input (R-07)
```
graph: realistic topology (some Supports, some CoAccess)
seed_scores: pre-normalized, all in (0.0, 1.0)
result = personalized_pagerank(graph, seed_scores, 0.85, 20)
FOR (id, score) IN result DO
    ASSERT score.is_finite()
    ASSERT score >= 0.0
    ASSERT score <= 1.0 + f64::EPSILON  // allow tiny float rounding
END FOR
```

### T-PPR-14: Node-ID sort Vec is outside iteration loop (R-04 / ADR-004 code review)
Static check: `grep "sort_unstable"` in `graph_ppr.rs` appears exactly once, outside any
`for` loop block. This is a code review gate; the timing test T-PPR-11 provides a
behavioral signal.
