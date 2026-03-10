## ADR-002: Fire-and-Forget Semantics for query_log Writes

### Context

Every search invocation (UDS and MCP) must write a `query_log` row capturing query text, result metadata, retrieval mode, and source transport. This write happens after search results are computed and must not add observable latency to the response.

Two precedents exist:
1. **injection_log** (col-010): UDS path uses `spawn_blocking_fire_and_forget`. Failures logged at `warn` level. Search response unaffected.
2. **Usage recording** (crt-001, ADR-004 Unimatrix #101): Fire-and-forget. Usage is a side effect, not the purpose of the tool call.

Risk SR-05 from the scope risk assessment identified that failure behavior was unspecified. This ADR resolves it.

Unimatrix #735 documents a prior incident where unbatched fire-and-forget writes saturated the blocking pool. However, the query_log write is a single INSERT (not a batch of 3-4 tasks), and both UDS and MCP paths process requests sequentially, so pool saturation is not a concern at current concurrency levels.

### Decision

Use fire-and-forget semantics for query_log writes in both search paths:

1. **UDS path** (`handle_context_search`): Use `spawn_blocking_fire_and_forget` (existing helper). Write after the injection_log write.
2. **MCP path** (`context_search` tool): Use `tokio::task::spawn_blocking` with dropped JoinHandle. Write after usage recording.

**Failure behavior**: If `Store::insert_query_log` returns an error, log at `tracing::warn` with session_id, query text length, and error message. Do not propagate the error. Do not retry. The search response is returned to the caller regardless.

**Guard conditions**: Skip the query_log write if:
- UDS: `session_id` is None or empty (matches injection_log guard pattern)
- MCP: Never skip. Use empty string for session_id if not available. MCP queries are always analytically valuable.

### Consequences

- **Zero latency impact on search**: The query_log write runs asynchronously after the response is prepared.
- **Possible data gaps**: If the store is locked or write fails, that query is not logged. Acceptable for telemetry data -- crt-019 analysis tolerates gaps.
- **Consistent with existing patterns**: Both search paths follow the same fire-and-forget contract as injection_log and usage recording.
- **Observable failures**: `warn`-level logging surfaces persistent write failures for operational investigation.
