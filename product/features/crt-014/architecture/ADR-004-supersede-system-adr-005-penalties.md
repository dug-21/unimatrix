## ADR-004: Supersede System ADR-005 — Replace Hardcoded Penalties with Topology-Derived Scoring

### Context

System ADR-005 (stored in Unimatrix, established during crt-010) introduced `DEPRECATED_PENALTY = 0.7` and `SUPERSEDED_PENALTY = 0.5` as constants in `unimatrix-engine/src/confidence.rs`. The rationale acknowledged these were "judgment calls, not empirically derived" and deferred topology-aware scoring to "when petgraph integration is available."

The constants impose uniform penalties regardless of graph position:
- A deprecated entry with no successor (orphan) receives the same 0.7x penalty as one that is part of an active supersession chain
- A superseded entry that is 1 hop from an active successor gets the same 0.5x as one that is 3 hops away
- A partially-superseded entry (split into two successors) gets the same penalty as a fully-superseded one

crt-014 makes petgraph available. The prerequisite for ADR-005 is now met.

### Decision

Remove `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` from `confidence.rs`. Replace with `graph_penalty(node_id, &graph, &all_entries) -> f64` in `graph.rs`.

Penalty is now a function of topology:
- Orphan deprecated (no successors): `ORPHAN_PENALTY = 0.75` (softer — no known replacement)
- Active terminal reachable, depth 1: `CLEAN_REPLACEMENT_PENALTY = 0.40` (harsher — clean replacement)
- Active terminal reachable, depth ≥2: `CLEAN_REPLACEMENT_PENALTY * HOP_DECAY_FACTOR^(d-1)` (harsher per additional hop)
- Partial supersession (>1 successor): `PARTIAL_SUPERSESSION_PENALTY = 0.60` (softer — incomplete replacement)
- No active reachable: `DEAD_END_PENALTY = 0.65`

All constants are named and located in `graph.rs`. No constants remain in `confidence.rs` related to status penalties. The four tests in `confidence.rs` that asserted exact constant values are removed; behavioral ordering assertions in `graph.rs` replace them.

This ADR supersedes system ADR-005. Store a deprecation notice in Unimatrix via `context_deprecate` referencing this ADR.

### Consequences

Easier: Penalty is semantically correct for the entry's actual position in the supersession graph. Partially-superseded entries are not over-penalized. Multi-hop-outdated entries are more aggressively penalized. All penalty logic is co-located in `graph.rs` with its graph construction logic.

Harder: Penalty values are no longer constant across queries — they depend on the current graph state. Tests must use behavioral assertions (ordering invariants) rather than exact value assertions. If the penalty formula needs empirical tuning, developers must change `graph.rs` constants rather than two lines in `confidence.rs`.
