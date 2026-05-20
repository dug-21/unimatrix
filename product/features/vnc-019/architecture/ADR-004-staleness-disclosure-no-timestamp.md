## ADR-004: Staleness Disclosure via Tool Description Text — No `graph_rebuilt_at` Field

### Context

SR-01 from the scope risk assessment identifies in-memory BFS staleness as a high
severity / high likelihood risk: edges written within the current tick interval are
invisible to subgraph mode, and at depth 3 with `both` direction this is silent data
loss from the caller's perspective.

ADR-005 vnc-018 mandates staleness disclosure text in the tool description for any
mode that uses in-memory BFS (neighbors depth>1). Subgraph mode is entirely in-memory
BFS and must include the same disclosure.

The scope risk assessment raises whether a `graph_rebuilt_at` timestamp field in the
`SubgraphResponse` envelope would help callers reason about graph freshness. Two options:

(A) Tool description text only (same as neighbors mode). No response field.
    The staleness contract is documented at the API layer. Callers who require
    freshness guarantees use depth=1 neighbors (SQL) or direct entry retrieval.

(B) Add `graph_rebuilt_at: Option<i64>` to `SubgraphResponse`. Populated from
    `TypedGraphState` at the time the lock is acquired. Callers can compute
    `now - graph_rebuilt_at` to assess staleness.

Arguments for (B): gives callers a concrete signal to act on; supports monitoring use
cases where agents report graph lag.

Arguments against (B):
1. The tick interval is not configurable per-call — knowing the timestamp does not
   let callers change the behavior. They cannot force a rebuild.
2. `TypedGraphState` does not currently track a `rebuilt_at` timestamp. Adding it
   requires a struct change and a write in the background tick — scope expansion.
3. The tick interval (30-60s) is already documented. The timestamp adds precision
   (exact age) but not a qualitatively different signal.
4. `depth_reached` and `truncated` together already provide the caller with the
   effective traversal bounds. Graph age is a separate concern.
5. neighbors mode does not expose this field. Inconsistency across modes is
   undesirable without a clear gain.

The staleness is a property of the system architecture (tick-rebuild cache), not a
property of a single traversal. It belongs in the tool description, not in the
response envelope. Future features that add monitoring or freshness guarantees can
add this as a cross-cutting concern (a new field on `TypedGraphState`, a dedicated
status endpoint, or a `graph_cache_age_ms` field on the generic context_status tool).

### Decision

Staleness disclosure is provided via tool description text only. `SubgraphResponse`
does NOT include a `graph_rebuilt_at` or `graph_age_ms` field.

The `context_graph` tool description is updated to include, in the subgraph mode
section:

> "subgraph mode uses the in-memory graph cache for BFS traversal. The cache is
> rebuilt each tick (typically 30-60 seconds). Edges written within the current tick
> interval may not appear in the result. This is the same staleness contract as
> neighbors mode at depth>1. The `depth_reached` field in the response reports the
> actual maximum BFS depth traversed; `truncated: true` indicates the `max_nodes` cap
> was reached before BFS completed. Seed IDs not present in the graph return an empty
> result — not an error."

This text is sufficient to fulfill ADR-005 vnc-018's disclosure mandate.

`depth_reached` and `truncated` remain in `SubgraphResponse` as the caller's signals
for traversal bounds. They are not staleness signals, but they provide the traversal
outcome context that callers need to interpret an empty or partial result.

### Consequences

Easier: No change to `TypedGraphState` struct. No background tick modification.
Consistent with neighbors mode disclosure model. Simpler `SubgraphResponse` struct.

Harder: Callers who want to programmatically assess graph freshness cannot do so from
the response envelope. They must rely on the documented tick interval or use a separate
freshness check. This is acceptable given the tick interval is stable and documented.
