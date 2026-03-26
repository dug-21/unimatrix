## ADR-003: write_pool_server() for Graph Cohesion SQL Queries

### Context

`unimatrix-store` exposes two SQLite connection pools:

- `read_pool()` — a read-only pool intended for high-concurrency query paths
  (search, lookup, get). Uses `SqliteConnectOptions` with `read_only: true`.
- `write_pool_server()` — a write-capable pool exposed to the server crate for
  operations that need write access or must serialise with writes.

The `compute_status_aggregates()` function — the direct precedent for
`compute_graph_cohesion_metrics()` — uses `read_pool()` for its aggregate queries
(confirmed at lines 959, 983 of `read.rs`). This appears to allow read-only queries
to use the lighter pool.

However, `query_bootstrap_contradicts()` and other GRAPH_EDGES-reading functions use
`write_pool_server()`. The SCOPE.md constraint explicitly states: "all queries use
`write_pool_server()`" and cites this as confirmed by `compute_status_aggregates`
(though `compute_status_aggregates` actually uses `read_pool()`).

The risk of using `read_pool()` for `compute_graph_cohesion_metrics()`:
- GRAPH_EDGES is written by the NLI inference service, which uses `write_pool_server()`
  through the write path. Under SQLite WAL mode, read connections may see a
  slightly stale snapshot depending on checkpoint state.
- `context_status` is a diagnostic tool. Operators invoke it to see current state.
  Returning stale cohesion values (e.g., showing 0 inferred edges right after NLI
  inference ran) would be confusing.
- Consistency: other GRAPH_EDGES queries (`query_bootstrap_contradicts`,
  `find_nli_count_for_circuit_breaker`) already use `write_pool_server()`. Using a
  different pool for cohesion creates an inconsistent query pattern on the same table.

### Decision

Use `write_pool_server()` for both SQL queries in `compute_graph_cohesion_metrics()`.

This matches the pattern of all other GRAPH_EDGES-reading methods in `read.rs` and
ensures the cohesion snapshot is consistent with the current write state.

Note: the SCOPE.md assertion that `compute_status_aggregates` uses `write_pool_server()`
is incorrect — it uses `read_pool()`. The architecture documents the actual decision
separately. Cohesion queries use `write_pool_server()` because GRAPH_EDGES is a
write-active table during inference runs, not because of a universal rule.

### Consequences

Easier:
- Cohesion metrics reflect the same GRAPH_EDGES state as writes from NLI inference
- Consistent with existing GRAPH_EDGES query methods
- No pool contention with `context_status`'s other operations, which also use
  `write_pool_server()` for Phase 1 aggregates

Harder:
- `write_pool_server()` has a stricter acquire timeout (5s vs 2s for `read_pool()`).
  Under high write load this could slow down `context_status`. Acceptable trade-off
  given the diagnostic, infrequent nature of this call.
