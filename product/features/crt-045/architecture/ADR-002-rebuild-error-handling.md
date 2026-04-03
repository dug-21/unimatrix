## ADR-002: Degraded Mode (use_fallback=true) on Rebuild Failure, Not Abort

### Context

`TypedGraphState::rebuild()` can fail in two distinct ways:

1. **Cycle detected** (`StoreError::InvalidInput { reason: "supersession cycle detected" }`) —
   returned when `build_typed_relation_graph()` finds a cycle in the Supersedes graph. This is
   a data integrity condition, not a transient error.

2. **Store I/O error** (`StoreError::*`) — returned when `store.query_all_entries()` or
   `store.query_graph_edges()` fails due to SQLite errors or connection issues.

`EvalServiceLayer::from_profile()` currently returns `Err(EvalError)` only for hard invariant
violations (live-DB path collision, invalid confidence weights, unrecognized NLI model name).
The question is whether a rebuild failure should abort construction or degrade gracefully.

The snapshot database is static — no writes occur during eval. A rebuild failure at
construction time will not self-correct on retry; the graph will remain unavailable for the
entire eval run. However, `from_profile()` returning `Err` would prevent the eval run from
starting at all, blocking collection of any metrics (including baseline metrics unrelated to
the graph). The SCOPE.md Acceptance Criterion AC-05 explicitly requires degraded mode.

The live server background tick already implements this pattern: on rebuild failure, it logs a
warning and retains the previous state (or cold-start state on first tick). The eval path has
no retry capability but the same conservative behavior applies.

### Decision

On any `Err` from `TypedGraphState::rebuild()`:

1. Log at `tracing::warn!` with the profile name and error message.
2. Set `rebuilt_state = None` (or leave the cold-start handle as-is).
3. Return `Ok(layer)` with `use_fallback = true` in the typed graph handle.

Log at `tracing::info!` on successful rebuild (OQ-02 resolution: rebuild is the operation that
makes the profile meaningful; visible without debug mode).

The specific log messages:
- Success: `info!(profile = %profile.name, entries = N, "eval: TypedGraphState rebuilt")`
- Cycle: `warn!(profile = %profile.name, "eval: TypedGraphState rebuild skipped — cycle detected; use_fallback=true")`
- I/O error: `warn!(profile = %profile.name, error = %e, "eval: TypedGraphState rebuild failed; use_fallback=true")`

### Consequences

Easier:
- `from_profile()` never aborts due to graph state issues — eval runs can always proceed to
  collect baseline metrics even when the graph is unavailable.
- The degraded mode behavior is identical to the live server tick; no new error semantics.
- AC-05 is satisfied structurally — no special test configuration needed to exercise this path.

Harder:
- A developer who sees bit-identical baseline/PPR results and does not check logs will not
  know the graph rebuild failed. The `tracing::warn!` must be present and visible (not
  suppressed by log level).
- The test for AC-06 must seed a valid (cycle-free, non-empty) graph to confirm the success
  path; a separate test may be added for the cycle/error degraded path if desired.
