## ADR-006: graph_expand Traversal Direction — Outgoing Only, Specified Behaviorally

### Context

SR-06 from SCOPE-RISK-ASSESSMENT.md identified a recurring problem with graph traversal
direction specifications in this codebase. crt-030 ADR-003 originally specified
`Direction::Incoming` for PPR traversal. The spec, architecture, and ADR all said "traverse
Incoming direction." The implementation used `Direction::Outgoing` — which was mathematically
correct for reverse/transpose PPR. The mismatch between conceptual direction (Incoming, from
the algorithm's perspective) and iteration variable direction (Outgoing, in the power-iteration
accumulation loop) caused four spec artifacts to require post-merge correction (lesson entry
#3754, lesson entry #3750).

This ADR deliberately specifies `graph_expand`'s traversal direction both by:
1. **Behavioral outcome** — what entries surface given what edges exist (authoritative)
2. **Direction enum value** — what `edges_of_type()` receives (derived, must match behavior)

**Direction::Outgoing for graph_expand**:

`graph_expand` is a BFS expander, not a reverse/transpose accumulator. It starts at seed nodes
and explores outward along edges. For a seed node S and edge S → T (`RelationType::Informs`,
source S points to target T), `Direction::Outgoing` from S returns T as a neighbor.

`graph_expand` is NOT implementing reverse PPR. It is implementing standard BFS: "given this
seed, what nodes can I reach by following edges forward?" The `Direction::Outgoing` value in
`graph_expand` is conceptually consistent with the implementation — unlike PPR where they were
inverted. There is no ambiguity here.

**Behavioral outcomes** (authoritative test contracts):

- Seed: `{A}`. Edge `A → B` (A Informs B). `graph_expand` at depth 1 returns `{B}`.
  Behavioral reading: entry B surfaces when entry A is an HNSW seed and A informs B.

- Seed: `{B}`. Edge `A → B` (A Informs B). `graph_expand` at depth 1 returns `{}` (empty).
  Behavioral reading: entry A does NOT surface when B is an HNSW seed and only A→B exists
  (no reverse edge B→A). A is a predecessor of B, not reachable by Outgoing traversal.

- Seed: `{B}`. Edge `A → B` AND `B → A` (bidirectional CoAccess). `graph_expand` at depth 1
  returns `{A}`.
  Behavioral reading: CoAccess partner A surfaces when B is a seed, because crt-035 writes
  both directions for CoAccess edges.

**Why Outgoing-only solves bidirectionality at the write side**:

Symmetric relations (CoAccess) store both directions at write time (crt-035 back-fill, entry
#3891). Any entry connected by a symmetric CoAccess edge to a seed is reachable via Outgoing
traversal from either endpoint.

For asymmetric relations (Informs from S1/S2), single-direction writes mean only one traversal
direction works from a given seed. This is why the S1/S2 directionality check is a hard
pre-implementation gate (see ARCHITECTURE.md Integration Points). If S1/S2 writes only A→B
(where A < B), then Outgoing from B does not reach A. This is not a traversal-direction bug —
it is a write-side deficiency. The fix is at the write site (back-fill both directions), not
by using `Direction::Incoming` or `Direction::Both` (which would break reverse PPR semantics).

**Why not Direction::Both**:

Using `Direction::Both` would traverse both edge directions simultaneously, making it possible
to reach predecessors of seeds even without reverse edges. However:
1. `edges_of_type()` accepts a single `Direction` value (petgraph API). `Direction::Both`
   would require two calls per edge type per node.
2. More importantly, using `Direction::Both` in `graph_expand` while `Direction::Outgoing` is
   used in `graph_ppr.rs` creates directional inconsistency across graph traversal functions.
   The existing graph traversal convention is Outgoing-only throughout (`graph_ppr.rs`,
   `graph_suppression.rs`). Introducing a mixed-direction expander creates a class of subtle
   bugs when the two functions interact.
3. The correct fix for half-visible graph topology is always at the write side.

### Decision

`graph_expand` uses `Direction::Outgoing` exclusively in all `edges_of_type()` calls.
The module-level doc comment in `graph_expand.rs` states the behavioral contract in
behavioral terms first, followed by the enum value:

```
/// Traversal direction: Outgoing only.
///
/// Behavioral contract:
///   Entry T surfaces when seed S exists and edge S → T exists (S points to T).
///   Entry S does NOT surface when seed T exists and only S → T exists (no reverse edge).
///
/// This implements standard BFS, not reverse/transpose PPR. Direction::Outgoing in
/// edges_of_type() correctly expresses "follow edges forward from this node."
/// See: lesson entry #3754 for why behavioral specification is authoritative.
/// See: ADR-003 crt-030 (entry #3750) for the PPR reverse-traversal distinction.
```

All unit tests for `graph_expand` are written in behavioral form: "given edge A→B and seed A,
B appears in result" — not "given Direction::Outgoing traversal, node at target() appears."
Behavioral tests catch direction implementation errors; enum-value assertions do not.

### Consequences

- The direction enum value and behavioral contract are co-documented, preventing the crt-030
  ambiguity from recurring.
- Test contracts specified behaviorally are robust against refactoring that changes the
  accumulation formula while preserving observable behavior.
- Single-direction S1/S2 edges are a write-side problem, not a traversal-direction problem.
  The architecture correctly separates these concerns and prescribes the write-side fix.
- Future graph traversal functions in this codebase should follow the same pattern: behavioral
  contract first, direction enum value second.

Related: lesson entry #3754, ADR-003 crt-030 (entry #3750), CoAccess back-fill entry #3891.
