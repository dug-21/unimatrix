# engine-types — Pseudocode

**File**: `crates/unimatrix-engine/src/graph.rs`
**Replaces**: `SupersessionGraph`, `build_supersession_graph`
**Preserves**: All penalty constants, all traversal logic, all 25+ existing unit tests

---

## Purpose

Upgrade `StableGraph<u64, ()>` → `StableGraph<u64, RelationEdge>` by introducing
`RelationType`, `RelationEdge`, and `TypedRelationGraph`. Update `build_supersession_graph`
→ `build_typed_relation_graph` with a second parameter accepting pre-loaded `GraphEdgeRow`
slices. Update `graph_penalty` and `find_terminal_active` signatures. Route all traversal
through the `edges_of_type` filter boundary. Remove `SupersessionGraph` and
`build_supersession_graph`.

---

## New / Modified Types

### RelationType (new enum)

```
pub enum RelationType {
    Supersedes,
    Contradicts,
    Supports,
    CoAccess,
    Prerequisite,   // reserved for W3-1; no write path in crt-021
}
```

#### as_str() method

```
FUNCTION as_str(&self) -> &'static str:
    match self:
        Supersedes  → "Supersedes"
        Contradicts → "Contradicts"
        Supports    → "Supports"
        CoAccess    → "CoAccess"
        Prerequisite → "Prerequisite"
```

#### from_str() method

```
FUNCTION from_str(s: &str) -> Option<RelationType>:
    match s:
        "Supersedes"   → Some(Supersedes)
        "Contradicts"  → Some(Contradicts)
        "Supports"     → Some(Supports)
        "CoAccess"     → Some(CoAccess)
        "Prerequisite" → Some(Prerequisite)
        _              → None
```

---

### RelationEdge (new struct)

```
pub struct RelationEdge {
    pub relation_type:  String,   // RelationType::as_str() value — never integer
    pub weight:         f32,      // validated finite; Supersedes=1.0, CoAccess=count/MAX
    pub created_at:     i64,      // unix epoch seconds
    pub created_by:     String,   // agent_id or "bootstrap"
    pub source:         String,   // "entries.supersedes" | "co_access" | "nli" | "bootstrap"
    pub bootstrap_only: bool,     // true → structurally excluded from TypedRelationGraph.inner
}
```

Note: `bootstrap_only` is stored in `GRAPH_EDGES` as `INTEGER NOT NULL DEFAULT 0`.
`build_typed_relation_graph` maps `GraphEdgeRow.bootstrap_only: bool` to this field.

---

### TypedRelationGraph (new struct, replaces SupersessionGraph)

```
pub struct TypedRelationGraph {
    pub(crate) inner:      StableGraph<u64, RelationEdge>,
    pub(crate) node_index: HashMap<u64, NodeIndex>,
}
```

#### edges_of_type method — THE SOLE FILTER BOUNDARY (SR-01 mitigation)

```
FUNCTION edges_of_type(
    &self,
    node_idx:      NodeIndex,
    relation_type: RelationType,
    direction:     Direction,
) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>>:

    RETURN self.inner
        .edges_directed(node_idx, direction)
        .filter(move |e| e.weight().relation_type == relation_type.as_str())
```

INVARIANT: `graph_penalty`, `find_terminal_active`, `dfs_active_reachable`, and
`bfs_chain_depth` MUST NOT call `.edges_directed()` or `.neighbors_directed()` directly.
All traversal goes through `edges_of_type` or through neighbor extraction from
`edges_of_type` iterator results.

---

## Modified Functions

### build_typed_relation_graph (replaces build_supersession_graph)

Signature change: now takes `edges: &[GraphEdgeRow]` as second argument.
The `GraphEdgeRow` type is defined in `unimatrix-store/src/read.rs` and imported here.

```
FUNCTION build_typed_relation_graph(
    entries: &[EntryRecord],
    edges:   &[GraphEdgeRow],
) -> Result<TypedRelationGraph, GraphError>:

    LET graph = TypedRelationGraph {
        inner: StableGraph::new(),
        node_index: HashMap::with_capacity(entries.len()),
    }

    -- Pass 1: Add one node per entry (identical to build_supersession_graph Pass 1)
    FOR each entry in entries:
        LET idx = graph.inner.add_node(entry.id)
        graph.node_index.insert(entry.id, idx)

    -- Pass 2a: Add Supersedes edges from entries.supersedes (authoritative source)
    --
    -- IMPORTANT: Supersedes edges are derived from entries.supersedes here,
    -- NOT from GRAPH_EDGES rows. This preserves the cycle-detection path operating
    -- on the authoritative source (entries.supersedes is canonical for supersession).
    -- GRAPH_EDGES Supersedes rows are for persistence/attribution; they are not the
    -- source of graph topology for in-memory construction. (ARCHITECTURE §1, R-12)
    FOR each entry in entries:
        IF entry.supersedes is Some(pred_id):
            MATCH graph.node_index.get(pred_id):
                None:
                    tracing::warn!(
                        entry_id = entry.id,
                        missing_pred_id = pred_id,
                        "build_typed_relation_graph: dangling supersedes reference, skipping edge"
                    )
                Some(pred_idx):
                    LET succ_idx = graph.node_index[entry.id]
                    LET edge = RelationEdge {
                        relation_type: "Supersedes".to_string(),
                        weight: 1.0,
                        created_at: 0,        -- not significant for in-memory penalty use
                        created_by: "bootstrap".to_string(),
                        source: "entries.supersedes".to_string(),
                        bootstrap_only: false,
                    }
                    graph.inner.add_edge(pred_idx, succ_idx, edge)

    -- Pass 2b: Add non-Supersedes edges from GRAPH_EDGES rows
    --          bootstrap_only=true rows are STRUCTURALLY EXCLUDED here (C-13, ADR-001 §3)
    --          No conditional check at traversal time is needed because these edges
    --          never enter graph.inner at all.
    FOR each row in edges:
        IF row.bootstrap_only:
            CONTINUE    -- structural exclusion; never added to inner graph

        -- Skip Supersedes rows from GRAPH_EDGES: already derived from entries.supersedes above
        IF row.relation_type == "Supersedes":
            CONTINUE    -- authoritative Supersedes already handled in Pass 2a

        -- Resolve node indices; skip if either endpoint is missing from node_index
        LET source_idx = MATCH graph.node_index.get(row.source_id):
            None:
                tracing::warn!(
                    source_id = row.source_id,
                    target_id = row.target_id,
                    relation_type = row.relation_type,
                    "build_typed_relation_graph: source_id not in entries snapshot, skipping edge"
                )
                CONTINUE
            Some(idx) → idx

        LET target_idx = MATCH graph.node_index.get(row.target_id):
            None:
                tracing::warn!(
                    source_id = row.source_id,
                    target_id = row.target_id,
                    relation_type = row.relation_type,
                    "build_typed_relation_graph: target_id not in entries snapshot, skipping edge"
                )
                CONTINUE
            Some(idx) → idx

        -- Parse relation_type string; skip unrecognized types with a warning (R-10)
        LET _rtype = MATCH RelationType::from_str(&row.relation_type):
            None:
                tracing::warn!(
                    relation_type = row.relation_type,
                    source_id = row.source_id,
                    target_id = row.target_id,
                    "build_typed_relation_graph: unrecognized relation_type, skipping edge"
                )
                CONTINUE
            Some(t) → t    -- validated but not stored separately; relation_type String is carried

        LET edge = RelationEdge {
            relation_type: row.relation_type.clone(),
            weight:        row.weight,
            created_at:    row.created_at,
            created_by:    row.created_by.clone(),
            source:        row.source.clone(),
            bootstrap_only: row.bootstrap_only,   -- always false here (filtered above)
        }
        graph.inner.add_edge(source_idx, target_idx, edge)

    -- Pass 3: Cycle detection on the Supersedes sub-graph
    --
    -- petgraph::algo::is_cyclic_directed operates on the full inner graph.
    -- Non-Supersedes edges cannot form a directed cycle that is relevant
    -- to supersession chain traversal. However, CoAccess edges may be
    -- bidirectional (A→B and B→A), which would make is_cyclic_directed
    -- return true erroneously.
    --
    -- MITIGATION: Build a temporary Supersedes-only view for cycle detection.
    -- Use petgraph::algo::is_cyclic_directed on a filtered graph that includes
    -- only edges where weight().relation_type == "Supersedes".
    --
    -- IMPLEMENTATION NOTE: Use petgraph::visit::EdgeFiltered or build a
    -- separate temporary StableGraph<u64, ()> containing only Supersedes edges
    -- from graph.inner, run is_cyclic_directed on it.
    -- The second approach (temporary graph) is safer and avoids petgraph version
    -- compatibility issues with EdgeFiltered.

    LET mut temp_graph: StableGraph<u64, ()> = StableGraph::new()
    LET mut temp_nodes: HashMap<u64, NodeIndex> = HashMap::new()

    FOR each (entry_id, node_idx) in graph.node_index:
        LET tidx = temp_graph.add_node(entry_id)
        temp_nodes.insert(entry_id, tidx)

    FOR each edge_ref in graph.inner.edge_references():
        IF edge_ref.weight().relation_type == "Supersedes":
            LET src_id = graph.inner[edge_ref.source()]
            LET tgt_id = graph.inner[edge_ref.target()]
            LET tsrc = temp_nodes[src_id]
            LET ttgt = temp_nodes[tgt_id]
            temp_graph.add_edge(tsrc, ttgt, ())

    IF is_cyclic_directed(&temp_graph):
        RETURN Err(GraphError::CycleDetected)

    RETURN Ok(graph)
```

---

### graph_penalty (signature updated)

```
FUNCTION graph_penalty(
    node_id: u64,
    graph:   &TypedRelationGraph,   -- was &SupersessionGraph
    entries: &[EntryRecord],
) -> f64:

    -- Guard: node not in graph → no penalty
    LET node_idx = MATCH graph.node_index.get(node_id):
        None → RETURN 1.0
        Some(idx) → idx

    -- Lookup entry record
    LET entry = MATCH entry_by_id(node_id, entries):
        None → RETURN 1.0
        Some(e) → e

    -- Signal 1: outgoing Supersedes edge count (uses edges_of_type boundary)
    LET outgoing_count = graph
        .edges_of_type(node_idx, RelationType::Supersedes, Direction::Outgoing)
        .count()
    LET successor_count = outgoing_count

    -- Signal: is_orphan — Deprecated with no outgoing Supersedes edges
    LET is_orphan = entry.status == Status::Deprecated && outgoing_count == 0

    -- Priority 1: orphan
    IF is_orphan:
        RETURN ORPHAN_PENALTY

    -- Signal 2: active_reachable via Supersedes edges
    LET active_reachable = dfs_active_reachable(node_idx, graph, entries)

    -- Priority 2: no active terminal reachable
    IF NOT active_reachable:
        RETURN DEAD_END_PENALTY

    -- Priority 3: partial supersession — multiple direct Supersedes successors
    IF successor_count > 1:
        RETURN PARTIAL_SUPERSESSION_PENALTY

    -- Signal 3: chain_depth via Supersedes edges
    LET chain_depth = bfs_chain_depth(node_idx, graph, entries)

    -- Priority 4: clean replacement at depth 1
    IF chain_depth == Some(1):
        RETURN CLEAN_REPLACEMENT_PENALTY

    -- Priority 5: hop decay at depth >= 2
    IF chain_depth == Some(d) AND d >= 2:
        LET raw = CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR.powi((d - 1) as i32)
        RETURN raw.clamp(0.10, CLEAN_REPLACEMENT_PENALTY)

    -- Priority 6: defensive fallback
    RETURN DEAD_END_PENALTY
```

---

### find_terminal_active (signature updated)

```
FUNCTION find_terminal_active(
    node_id: u64,
    graph:   &TypedRelationGraph,   -- was &SupersessionGraph
    entries: &[EntryRecord],
) -> Option<u64>:

    LET start_idx = MATCH graph.node_index.get(node_id):
        None → RETURN None
        Some(idx) → idx

    -- Iterative DFS; depth-capped at MAX_TRAVERSAL_DEPTH
    LET stack: Vec<(NodeIndex, usize)> = [(start_idx, 0)]
    LET visited: HashSet<NodeIndex> = {start_idx}

    WHILE stack is not empty:
        LET (current_idx, depth) = stack.pop()
        LET current_id = graph.inner[current_idx]

        IF entry_by_id(current_id, entries) is Some(e)
            AND e.status == Status::Active
            AND e.superseded_by.is_none():
                RETURN Some(current_id)

        IF depth + 1 > MAX_TRAVERSAL_DEPTH:
            CONTINUE

        -- Neighbors via Supersedes edges only (SR-01)
        FOR each edge_ref in graph.edges_of_type(current_idx, Supersedes, Outgoing):
            LET neighbor_idx = edge_ref.target()
            IF NOT visited.contains(neighbor_idx):
                visited.insert(neighbor_idx)
                stack.push((neighbor_idx, depth + 1))

    RETURN None
```

---

## Private Helpers (updated signatures)

### dfs_active_reachable

```
FUNCTION dfs_active_reachable(
    start_idx: NodeIndex,
    graph:     &TypedRelationGraph,   -- was &SupersessionGraph
    entries:   &[EntryRecord],
) -> bool:

    LET stack: Vec<NodeIndex> = [start_idx]
    LET visited: HashSet<NodeIndex> = {}

    WHILE stack is not empty:
        LET current_idx = stack.pop()
        IF NOT visited.insert(current_idx):
            CONTINUE

        -- Neighbors via Supersedes edges only (SR-01)
        FOR each edge_ref in graph.edges_of_type(current_idx, Supersedes, Outgoing):
            LET neighbor_idx = edge_ref.target()
            LET neighbor_id = graph.inner[neighbor_idx]
            IF entry_by_id(neighbor_id, entries) is Some(e)
                AND e.status == Status::Active
                AND e.superseded_by.is_none():
                    RETURN true
            stack.push(neighbor_idx)

    RETURN false
```

### bfs_chain_depth

```
FUNCTION bfs_chain_depth(
    start_idx: NodeIndex,
    graph:     &TypedRelationGraph,   -- was &SupersessionGraph
    entries:   &[EntryRecord],
) -> Option<usize>:

    LET queue: VecDeque<(NodeIndex, usize)> = [(start_idx, 0)]
    LET visited: HashSet<NodeIndex> = {start_idx}

    WHILE queue is not empty:
        LET (current_idx, depth) = queue.pop_front()
        IF depth > MAX_TRAVERSAL_DEPTH:
            CONTINUE

        FOR each edge_ref in graph.edges_of_type(current_idx, Supersedes, Outgoing):
            LET neighbor_idx = edge_ref.target()
            IF visited.contains(neighbor_idx):
                CONTINUE
            visited.insert(neighbor_idx)
            LET next_depth = depth + 1

            LET neighbor_id = graph.inner[neighbor_idx]
            IF entry_by_id(neighbor_id, entries) is Some(e)
                AND e.status == Status::Active
                AND e.superseded_by.is_none():
                    RETURN Some(next_depth)
            queue.push_back((neighbor_idx, next_depth))

    RETURN None
```

### entry_by_id (unchanged)

```
FUNCTION entry_by_id(id: u64, entries: &[EntryRecord]) -> Option<&EntryRecord>:
    RETURN entries.iter().find(|e| e.id == id)
```

---

## Removed Symbols

- `SupersessionGraph` struct — deleted
- `build_supersession_graph` function — deleted
- References to `StableGraph<u64, ()>` — deleted

---

## Imports Required

```
use unimatrix_store::GraphEdgeRow;   // new import from store layer
use petgraph::visit::EdgeRef;        // for EdgeReference in edges_of_type
// All existing imports retained
```

---

## Error Handling

| Scenario | Return |
|----------|--------|
| Node not in graph (graph_penalty / find_terminal_active) | 1.0 / None |
| Entry not in entries slice | 1.0 / None |
| Supersedes cycle detected in Pass 3 | `Err(GraphError::CycleDetected)` |
| Dangling supersedes reference in Pass 2a | `tracing::warn!`, edge skipped |
| Unknown relation_type string from GRAPH_EDGES | `tracing::warn!`, edge skipped |
| Endpoint not in node_index (non-Supersedes edge) | `tracing::warn!`, edge skipped |

---

## Key Test Scenarios

All 25+ existing unit tests must pass unchanged (AC-01, AC-10, NF-04). The test helper
`make_entry` requires no changes. The test calls to `build_supersession_graph` are
replaced with `build_typed_relation_graph(entries, &[])` — passing an empty edges slice
causes Pass 2b to add zero edges; only Pass 2a (entries.supersedes) populates the graph.
This is functionally identical to the old single-argument call.

1. **Round-trip RelationType** (AC-02): construct each variant, call as_str(), call
   from_str(result), assert equality. Assert `from_str("UnknownType")` returns None.

2. **bootstrap_only exclusion** (AC-12, R-03):
   - Build GraphEdgeRow with `bootstrap_only=true` for a Supersedes edge.
   - Call `build_typed_relation_graph(entries, &[row])`.
   - Assert inner graph has zero edges added from Pass 2b (edge was skipped).
   - Assert `graph_penalty` on the source node returns ORPHAN_PENALTY (no outgoing edges).

3. **Non-Supersedes edges do not affect graph_penalty** (AC-11, R-02):
   - Build TypedRelationGraph where node A has a Contradicts edge to node B and
     a Supersedes edge to node C (Active terminal).
   - Assert `graph_penalty(A, &graph, &entries)` returns CLEAN_REPLACEMENT_PENALTY
     (same as if Contradicts edge did not exist).

4. **Cycle detection on Supersedes-only sub-graph** (R-02 edge case):
   - Build a graph where A and B have CoAccess edges to each other (bidirectional),
     but no Supersedes cycle.
   - Assert `build_typed_relation_graph` returns Ok (not CycleDetected).

5. **Unknown relation_type skipped gracefully** (R-10):
   - Build GraphEdgeRow with `relation_type = "UnknownFutureType"`.
   - Assert `build_typed_relation_graph` returns Ok, logs warning, zero edges added for that row.

6. **Supersedes edges sourced from entries.supersedes, not GRAPH_EDGES** (R-12):
   - Build entries where entry B has `supersedes = Some(A.id)`.
   - Pass empty edges slice to `build_typed_relation_graph`.
   - Assert the graph has an A→B Supersedes edge (derived from entries, not GRAPH_EDGES).
   - Pass a GRAPH_EDGES row asserting A→B Supersedes (bootstrap_only=false).
   - Assert the graph still has exactly one A→B Supersedes edge (not doubled).
   - Note: Pass 2b skips Supersedes rows from GRAPH_EDGES since Pass 2a already handled them.
