## ADR-003: read_pool() for Graph Cohesion SQL Queries

**Supersedes initial draft that recommended write_pool_server().**

### Context

`unimatrix-store` exposes two SQLite connection pools:

- `read_pool()` — a read-only pool (`SqliteConnectOptions` with `read_only: true`).
  Used for high-concurrency query paths: search, lookup, get, and — importantly —
  `compute_status_aggregates()`.
- `write_pool_server()` — a write-capable pool with `max_connections = 1` (single
  serialization point). Write-adjacent reads that must see just-committed data use
  this pool (e.g., `query_bootstrap_contradicts()`, `get_content_via_write_pool()`).

The initial draft of this ADR recommended `write_pool_server()` based on:
1. WAL staleness concern: read-only connections may see a slightly older snapshot if
   a checkpoint has not occurred since the last write.
2. Consistency: other GRAPH_EDGES-reading functions use `write_pool_server()`.

Both arguments were re-examined and found to be insufficient:

**Staleness:** Under WAL mode with `wal_autocheckpoint = 1000`, read staleness is
bounded by checkpoint frequency. For a diagnostic aggregate — called from
`context_status`, not from a write path — a rare, non-corrupting lag in reported
counts is an acceptable trade-off. There is no write-after-read operation here. The
concern applies to write-adjacent reads (e.g., "did my just-inserted edge make it
into the NLI circuit breaker?"), not to independent diagnostic invocations.

**Consistency:** The GRAPH_EDGES functions that use `write_pool_server()` do so
because they are called immediately after a write in the same logical operation and
must see that write (`query_bootstrap_contradicts` in the NLI promotion path,
`get_content_via_write_pool` in content retrieval). This is a call-site property,
not a table-level rule. `compute_status_aggregates` — the direct structural analogue
for `compute_graph_cohesion_metrics` — uses `read_pool()`. The correct precedent
is `compute_status_aggregates`, not write-adjacent graph reads.

**Write pool contention (the real risk of using write_pool_server()):**
The write pool's `max_connections = 1` means every call to `compute_graph_cohesion_metrics()`
via `write_pool_server()` competes with NLI inference writes, usage recording, audit
log inserts, and all write tool calls. The acquire timeout is 5 seconds. Placing a
diagnostic aggregate — called from `context_status` — on the write serialization
point creates chronic contention risk during the exact moments when an operator is
trying to inspect the system: active inference runs. This is a worse trade-off than
accepting bounded WAL staleness.

### Decision

Use `read_pool()` for both SQL queries in `compute_graph_cohesion_metrics()`.

This matches `compute_status_aggregates()` — the correct precedent — and avoids
occupying the single-connection write serialization point with a read-only diagnostic
aggregate.

### Consequences

Easier:
- No contention with write paths during `context_status` invocations
- Consistent with `compute_status_aggregates()` — the correct analogue
- Separates diagnostic reads from write-path reads, which is the existing pattern

Harder:
- In WAL mode, cohesion metrics may reflect a snapshot that lags behind the most
  recent GRAPH_EDGES writes by up to one checkpoint interval. This is acceptable for
  a diagnostic tool: operators use `context_status` to assess graph health over time,
  not to verify a specific just-committed edge. If fresh-snapshot semantics are ever
  required, the correct mechanism is a WAL checkpoint before status computation, not
  routing reads through the write serialization point.
