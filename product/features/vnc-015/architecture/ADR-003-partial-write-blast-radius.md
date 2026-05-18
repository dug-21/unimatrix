## ADR-003: Partial-Write Blast Radius — Infrastructure Error, Not Rolled Back

### Context

`context_store` and `context_correct` insert the entry in one DB operation and write edges in
subsequent separate `write_graph_edge` calls. There is no single transaction spanning both. If
an edge write fails (SQL error from `write_graph_edge` returning `false` on the Err path) after
the entry insert, the entry exists in the DB but has no edges. The caller receives a success
response (the entry was created), not an error. The edge write failure is logged inside
`write_graph_edge` but is not propagated as a call failure.

SR-03 in the scope risk assessment identifies this as a medium-severity risk and asks: "acceptable
as infrastructure error (logged, not rolled back) as scoped, or wrapped in an explicit transaction
boundary?"

The existing handler pipeline for `context_store` and `context_correct` does not use explicit
transactions wrapping the full operation. Introducing a transaction boundary would require
refactoring the StoreService insert path, which is out of scope for this feature.

### Decision

The partial-write posture is accepted: entry insert + edge writes are not in a single DB
transaction. If edge writes fail after entry insert, the entry exists without edges. This is
treated as an infrastructure error (the DB connection or write pool failed, not a validation
error). The failure is logged once inside `write_graph_edge`; the edge_write loop does not
double-log and does not roll back the entry.

The rationale:
1. `write_graph_edge` uses `INSERT OR IGNORE` with the unique constraint `(source_id, target_id,
   relation_type)`. UNIQUE conflicts return `false` and are idempotent — not infrastructure errors.
2. True SQL errors (connectivity, pool exhaustion) from `write_graph_edge` are already logged
   inside the function with full context (source_id, target_id, relation_type, source, error).
3. The entry is independently useful without edges. An agent can re-declare edges via
   `context_correct` if the original edge write failed.
4. Introducing a full transaction would require changes to the write pool architecture
   (write_pool_server is a serializing single-connection pool; the analytics queue is fire-and-forget).
   This is a separate concern outside vnc-015 scope.

The blast radius is narrow: an entry with missing declared edges is a degraded state, not a
corrupt state. PPR operates on existing edges; missing edges simply mean the edge doesn't
contribute to traversal until re-declared.

### Consequences

Easier: no changes to the write pool architecture or StoreService transaction model. Edge write
failures degrade gracefully without impacting the caller response or the entry's existence.

Harder: agents cannot distinguish "edge write was accepted" from "edge write silently failed"
from the call response. Monitoring of edge write failures relies on server-side logs, not
caller-observable errors. A future `context_graph` traversal tool (Phase 2) would surface
the absence of expected edges, providing an indirect signal.

Related: ADR-001 (validation-first), ADR-002 (confidence floor posture).
Supersedes: none.
