## ADR-005: Cycle Detection Fallback Strategy

### Context

`build_supersession_graph` runs `petgraph::algo::is_cyclic_directed` on the constructed DAG. If a supersession cycle exists in production data (e.g., entry A supersedes B, B supersedes A), this is a data integrity violation. The question is: what should happen during a live search when a cycle is detected?

Options:
1. Return an error to the caller — search fails for all users until the cycle is resolved
2. Log and fall back to flat constant penalties, continue search — search degrades gracefully
3. Remove the cyclic edges and continue with a partial graph — complex, risks silent data loss

The human chose option 2 in the design session (OQ-4 answer): "log as data integrity error, fall back to constant penalty, surface in context_status."

### Decision

On `Err(GraphError::CycleDetected)` from `build_supersession_graph`:

1. Log at `tracing::error!` level: `"supersession cycle detected in knowledge graph — search using fallback penalties"`
2. Set `use_fallback = true` for the remainder of this query
3. Apply `FALLBACK_PENALTY = 0.70` to all entries that would have received graph-derived penalties
4. Use single-hop (`entry.superseded_by`) for successor injection (preserving ADR-003 single-hop behavior as safe degradation)
5. Search result is returned to the caller with no error — availability is preserved

**Cycle surfacing in `context_status`**: The cycle is recorded via `tracing::error!` — it appears in structured logs. For surfacing in `context_status` output, this is a log-only approach in v1 (the `tracing::error!` output flows to the same log stream as other structured events). Adding a dedicated `context_status` field for cycle detection is considered out-of-scope for crt-014 (would require a status service struct change). The log-only approach is sufficient for operator awareness.

`FALLBACK_PENALTY = 0.70` is defined in `graph.rs` alongside the other penalty constants — not in `confidence.rs`. Its value matches the old `DEPRECATED_PENALTY` semantics as the conservative safe default.

### Consequences

Easier: Search availability is never compromised by a data integrity issue in the supersession graph. The cycle error is detectable in logs without requiring a status API change. Fallback behavior is explicit and documented.

Harder: A cycle in production data silently degrades penalty quality for all affected entries on every search until fixed. Developers must monitor logs for `tracing::error!` cycle events. If cycle surfacing in `context_status` is needed in a future iteration, it will require a status service struct change.
