## ADR-001 (crt-021): Typed Edge Weight Model

**Supersedes**: Unimatrix entry #1604 (ADR-004, crt-014: Topology-Derived Penalty Scoring)

---

### Context

ADR-004 (entry #1604, crt-014) established the topology-derived penalty scoring model for
the `SupersessionGraph`. That ADR documents that edge weights are `()` (unit type) — edges
carry no data, only topology. The penalty model is purely structural: outgoing edge counts,
chain depth, and reachability determine the penalty multiplier.

crt-021 (W1-1) replaces `StableGraph<u64, ()>` with `StableGraph<u64, RelationEdge>`. The
edge type is no longer unit. `RelationEdge` carries five fields: `relation_type: String`,
`weight: f32`, `created_at: i64`, `created_by: String`, `source: String`. ADR-004 is
therefore architecturally inconsistent with the post-crt-021 codebase: it describes a `()`
edge model that no longer exists.

Additionally, the graph now carries five distinct edge types (Supersedes, Contradicts,
Supports, CoAccess, Prerequisite) rather than the single implicit Supersedes type of the
old `SupersessionGraph`. The penalty model must define which edge types it operates on
and which it ignores.

A further question — not resolved in ADR-004 — is how bootstrap-origin edges that are
heuristically derived (rather than authoritative) interact with confidence scoring. The
old system had no such edges; crt-021 introduces them.

---

### Decision

#### 1. Typed edge weight model

`RelationEdge` carries a `weight: f32` field with the following semantics:

- For **Supersedes** edges: `weight = 1.0` (uniform; supersession is binary — either it
  supersedes or it does not). The penalty computation remains topology-only for Supersedes.
- For **CoAccess** edges: `weight` is the normalized co-access count at bootstrap time
  (`count / MAX(count)` across the co_access table, clamped to [0.0, 1.0]). Higher weight
  = stronger co-access affinity. Used by W3-1 GNN as a per-edge feature; not used by
  current scoring.
- For **Contradicts** edges: `weight` carries NLI contradiction confidence score when
  created by W1-2 NLI (`source='nli'`). Bootstrap-origin Contradicts edges carry
  `weight = 0.0` to signal unconfirmed status.
- For **Supports** edges: `weight` carries NLI entailment confidence when created by W1-2.
- For **Prerequisite** edges: `weight = 1.0` (reserved; no bootstrap path in W1-1).

All `weight` values are validated finite (not NaN, not ±Inf) before any write path accepts
them. Invalid weights are rejected with a logged error; the edge is not written.

#### 2. Penalty logic filters to Supersedes edges only

`graph_penalty` and `find_terminal_active` operate exclusively on Supersedes edges.
Non-Supersedes edges are present in `TypedRelationGraph` but are structurally invisible to
penalty computation.

The filter is enforced at a single method boundary:

```rust
impl TypedRelationGraph {
    pub fn edges_of_type(
        &self,
        node_idx: NodeIndex,
        relation_type: RelationType,
        direction: Direction,
    ) -> impl Iterator<Item = EdgeReference<'_, RelationEdge>>
```

All traversal functions (`graph_penalty`, `find_terminal_active`, `dfs_active_reachable`,
`bfs_chain_depth`) call `edges_of_type(..., RelationType::Supersedes, ...)`. Direct calls
to `.edges_directed()` in these functions are prohibited.

**Rationale for Supersedes-only filtering:**

The six penalty cases (ORPHAN_PENALTY, DEAD_END_PENALTY, PARTIAL_SUPERSESSION_PENALTY,
CLEAN_REPLACEMENT_PENALTY with hop decay, and FALLBACK_PENALTY) are defined in terms of
supersession chain topology. They have no meaningful interpretation when applied to
CoAccess or Contradicts edges:
- A CoAccess edge from entry A to entry B does not mean A is "superseded" by B.
- A Contradicts edge does not form a directed replacement chain.

Future edge types (Supports, Prerequisite) similarly do not fit the supersession chain
model. When W3-1 (GNN) integrates edge weights into confidence scoring, it will learn
separate scoring weights per edge type through its own learned model — not through the
existing penalty constants.

The Supersedes-only filter preserves AC-10 (all existing `graph.rs` tests pass against
the typed graph unchanged) because the tests construct graphs with only Supersedes edges.
Against such a graph, `edges_of_type(..., Supersedes, ...)` returns the same edges that
`.edges_directed()` returned against the old `()` graph.

#### 3. Bootstrap edge exclusion policy

Edges with `bootstrap_only=1` in `GRAPH_EDGES` are excluded from the in-memory
`TypedRelationGraph` entirely. `build_typed_relation_graph` filters them out during
edge insertion:

```rust
for row in edges {
    if row.bootstrap_only {
        continue; // structural exclusion — never added to graph
    }
    // add edge to graph
}
```

This means no traversal function — including those that do not filter by type — can
encounter a bootstrap-only edge. The exclusion is structural, not conditional.

**Rationale:**

Bootstrap Contradicts edges are derived from `shadow_evaluations`, which stores
cosine-similarity heuristic scores. These heuristics produce false positives (entries
on similar topics but with compatible content). Injecting unconfirmed Contradicts edges
into the in-memory graph from day one would penalize valid entries via future scoring
paths that operate on all edge types (W3-1 GNN).

The `bootstrap_only` flag is the mechanism by which W1-2 NLI confirmation is deferred.
An edge transitions from bootstrap-only to confirmed via the W1-2 promotion path (see §4).

**Supersedes and CoAccess bootstrap edges carry `bootstrap_only=0`:**

- Supersedes edges from `entries.supersedes` are authoritative (explicit human/agent
  attribution). They are not heuristic. `bootstrap_only=0`.
- CoAccess edges bootstrapped from `co_access` at `count >= 3` are promoted from an
  authoritative signal (actual co-retrieval counts). `bootstrap_only=0`.

#### 4. Bootstrap-only promotion path (for W1-2)

W1-2 NLI promotes a `bootstrap_only=1` edge using DELETE + INSERT in a single transaction
on the direct `write_pool` path (not the analytics queue):

```sql
-- Step 1: Remove the unconfirmed bootstrap edge
DELETE FROM graph_edges
WHERE source_id = ?1 AND target_id = ?2 AND relation_type = ?3 AND bootstrap_only = 1;

-- Step 2: Insert the NLI-confirmed replacement
INSERT OR IGNORE INTO graph_edges
    (source_id, target_id, relation_type, weight, created_at, created_by, source, bootstrap_only)
VALUES (?1, ?2, ?3, ?4_nli_confidence, now(), ?5_nli_agent_id, 'nli', 0);
```

DELETE + INSERT is preferred over UPDATE because the confirmed edge has different
`created_by`, `source`, `created_at`, and `weight` values than the bootstrap edge.
UPDATE would leave stale attribution. The `UNIQUE(source_id, target_id, relation_type)`
constraint prevents duplicates if the INSERT is retried.

W1-2 must use the direct `write_pool` path for confirmed edge writes. The analytics
queue is shed-safe by policy; NLI-confirmed edges must never be silently dropped.

---

### Consequences

**What becomes easier:**

- W3-1 GNN can use `weight` as a per-edge feature for all five edge types without
  a schema migration. The field is persisted in v13.
- The filter boundary (`edges_of_type`) makes it safe to add new edge types without
  risking contamination of penalty logic — new types are invisible to penalty traversal
  until a new `edges_of_type` call site is added for them.
- The existing 25+ `graph.rs` unit tests pass unchanged because `edges_of_type` on a
  Supersedes-only graph produces the same traversal as the old `()` graph.
- W1-2 NLI has a defined promotion path. No schema changes are needed at W1-2 to
  support confirmed edge creation.
- Bootstrap edges are structurally excluded — no runtime check needed in penalty logic.

**What becomes harder:**

- `build_typed_relation_graph` now takes two arguments (`entries` and `edges`) where
  `build_supersession_graph` took one. All call sites must be updated.
- The `TypedGraphState` rebuild issues two SQL queries per tick where the old
  `SupersessionState` issued one. The added latency is bounded by the size of
  `GRAPH_EDGES`, which starts small and grows slowly.
- W1-2 cannot use the analytics queue for NLI-confirmed edge writes — it must implement
  a direct `write_pool` path. This is a known constraint documented here.

**Invariants preserved from ADR-004:**

- All six penalty cases (and their constants) are unchanged.
- `StableGraph` is retained (ADR-001, crt-014, entry #1601 — petgraph `stable_graph`
  feature only).
- Cycle detection on the Supersedes sub-graph remains via `petgraph::algo::is_cyclic_directed`.
- `FALLBACK_PENALTY` is returned when cycle detection fires — unchanged behavior.
- The `Arc<RwLock<_>>` tick-rebuild pattern (entry #1560) is extended, not replaced.
