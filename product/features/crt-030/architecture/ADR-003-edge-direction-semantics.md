## ADR-003: Edge Direction Semantics for PPR Traversal

### Context

`TypedRelationGraph` is a directed graph. Edge direction carries semantic meaning:

- `Supports` edges: `A → B` means "A supports B" (A provides evidence for B). Example:
  a lesson-learned A explains why a decision B was made. In GRAPH_EDGES, source=A, target=B.
- `Prerequisite` edges: `A → B` means "B requires A" (A is a prerequisite of B). Example:
  entry A must be understood before entry B. In GRAPH_EDGES, source=A, target=B.
- `CoAccess` edges: stored bidirectionally — both `A → B` and `B → A` are written during
  bootstrap from the co_access table. There is no semantic directionality.

The PPR personalization vector seeds from HNSW candidates (the "known relevant" set). The
goal is to surface entries that are connected to these seeds via positive edges — typically
the sources that support or enable the seed entries.

The question is: should PPR traverse **outgoing** edges from each seed (following the
direction the edge was written) or **incoming** edges (walking backward to the sources)?

Two options:
1. **Outgoing traversal**: From seed B, follow `B → X` edges. For Supports, this means
   "what does B support?" — moving away from seeds toward targets. For a decision B, this
   would surface entries B supports (unlikely to be lesson-learned entries).
2. **Incoming traversal**: From seed B, follow edges arriving at B, i.e., `X → B`. For
   Supports A→B, traversing Incoming on B reaches A. This surfaces lesson-learned and
   outcome entries A that support the seed decision B — which is exactly the use case.

The SCOPE.md Background Research section documents the resolved choice: Incoming traversal
for all three positive edge types.

### Decision

All three positive edge types use `Direction::Incoming` in `edges_of_type()` calls within
`graph_ppr.rs`.

**Supports (A→B)**: Traverse `Direction::Incoming` on node B to reach A. A lesson-learned
A that supports decision B becomes reachable when B is a seed. This breaks the access
imbalance described in the problem statement.

**Prerequisite (A→B)**: Traverse `Direction::Incoming` on node B to reach A. A prerequisite
A becomes reachable when the entry B that requires it is a seed. Supports the transparent
inclusion of Prerequisite edges described in SCOPE.md Goals item 6.

**CoAccess (stored as A→B and B→A)**: Traverse `Direction::Incoming`. Since edges are stored
bidirectionally, traversing Incoming on any node X reaches all CoAccess neighbors of X
(both those stored as `Y → X` and the reverse direction is covered by the `X → Y` partner
in the graph). The effect is symmetric.

**SR-01 compliance**: `edges_of_type()` is the sole traversal boundary. `graph_ppr.rs`
must never call `.edges_directed()` or `.neighbors_directed()` directly. SR-01 (which
restricts `graph_penalty` and `find_terminal_active` to Supersedes-only) does not apply
to `personalized_pagerank` — it is a new retrieval function, not a penalty function.
The SR-01 non-applicability is documented in the `personalized_pagerank` function comment
(AC-04 requirement).

**Supersedes and Contradicts exclusion**: These edge types are excluded by construction —
no `edges_of_type(node, RelationType::Supersedes, ...)` or
`edges_of_type(node, RelationType::Contradicts, ...)` call appears in `graph_ppr.rs`.

### Consequences

- Lesson-learned, outcome, and other "source" entries that support seed decisions become
  discoverable via PPR — directly addressing the access imbalance.
- CoAccess traversal activates `GRAPH_EDGES.CoAccess` as a positive relevance channel,
  complementing the existing co-access boost in the fused scorer.
- Prerequisite traversal requires no code change when #412 begins producing Prerequisite
  edges — they will be traversed automatically because Prerequisite is an included type.
- The Incoming direction for CoAccess is equivalent to the bidirectional traversal used
  in `suppress_contradicts` for Contradicts — consistent with the `graph_suppression.rs`
  pattern.
- A future edge type added to the positive set would require an explicit `edges_of_type`
  call in `graph_ppr.rs` — the exclusion is by construction, not by a blocklist.
