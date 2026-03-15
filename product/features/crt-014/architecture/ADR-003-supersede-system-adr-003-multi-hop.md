## ADR-003: Supersede System ADR-003 — Enable Multi-Hop Supersession Traversal

### Context

System ADR-003 (stored in Unimatrix, established during crt-004/crt-010) imposed a single-hop supersession limit in `search.rs`. The rationale at the time: "multi-hop traversal requires cycle detection infrastructure not yet in place."

The consequence of this limit: when entries form a chain A→B→C (A is superseded by B, B is superseded by C), the search service injects B as the successor of A — even if B is itself superseded and thus incorrect. The correct active entry is C, but it is never reached. This is documented in `search.rs:251` as `"Single-hop only (ADR-003, AC-06)"`.

crt-014 provides the cycle detection infrastructure (via `petgraph::algo::is_cyclic_directed`) that was the prerequisite for lifting this restriction.

### Decision

Remove the single-hop limit. Replace the `entry.superseded_by.is_none()` guard in Step 6b (successor injection) with `find_terminal_active(entry.id, &graph, &all_entries)`, which follows directed edges depth-first to the first Active, non-superseded node.

Depth is capped at `MAX_TRAVERSAL_DEPTH = 10` to prevent pathological traversal times on deep or malformed chains.

The `search.rs:251` comment `"Single-hop only (ADR-003, AC-06)"` is removed and replaced with `"Multi-hop via graph — see ADR-003 (crt-014)"`.

This ADR supersedes system ADR-003. Store a deprecation notice in Unimatrix via `context_deprecate` referencing this ADR as the replacement.

### Consequences

Easier: Search now injects the correct active terminal successor for multi-hop chains. Knowledge graph chains of any depth (within MAX_TRAVERSAL_DEPTH) are correctly resolved. Agents receive the current active entry, not an intermediate superseded one.

Harder: Successor injection now depends on the graph being correctly built. If graph construction fails (cycle error), the fallback is single-hop behavior (preserving the old ADR-003 semantics as the safe degradation path).
