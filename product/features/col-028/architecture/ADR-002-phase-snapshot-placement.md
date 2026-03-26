## ADR-002: Phase Snapshot Must Be First Statement in Handler Body Before Any Await

### Context

`SessionState.current_phase` is written by `context_cycle(phase-start/phase-end)` events
which can arrive concurrently with any read-side tool call. The Tokio async runtime may
yield at any `.await` point and allow a phase-change event to advance `current_phase`
between the time the handler starts and the time it reads from `SessionState`.

`context_store` (crt-025 ADR-001, pattern #3027) established the canonical contract:
phase is read synchronously before any `await`. `get_state` returns a `Clone`, so the
snapshot is isolated from all subsequent `SessionState` mutations — the lock is acquired
and released before the handler does any async work.

This same constraint applies to all four read-side tools in col-028. Without it, there
is a race condition where the phase captured in `UsageContext.current_phase` and written
to `query_log.phase` reflects a phase that was not active when the agent made the query.

### Decision

The call to `current_phase_for_session` (ADR-001) must be placed as the first statement
in each of the four handler bodies, before any `.await`:

```rust
// Must be BEFORE any .await in this handler
let current_phase = current_phase_for_session(
    &self.session_registry,
    ctx.audit_ctx.session_id.as_deref(),
);
```

The `context_search` handler also has a `query_log` write downstream. The same
`current_phase` binding is passed to both `UsageContext.current_phase` and
`QueryLogRecord::new(...)`. A single `get_state` call provides both values — this is
not a micro-optimisation; it is a correctness requirement (SR-06 in the risk assessment).
Two separate `get_state` calls could theoretically diverge if a phase-end event arrives
between them.

For `context_search` specifically, the snapshot also satisfies the existing
`spawn_blocking` capture requirement from crt-025: the phase value is captured before
the spawn closure is constructed.

### Consequences

- Phase captured in `UsageContext` and in `query_log` reflects the agent's actual phase
  at call time, not an arbitrary later point.
- The single-snapshot contract prevents double lock acquisition at `context_search`.
- If a future handler needs both `current_phase` and another field from `SessionState`,
  it must obtain both from the same `get_state` call in the same synchronous block —
  this ADR makes that pattern explicit.
- Implementors who add a new `get_state` call for phase after an `await` will violate
  this constraint; AC-12 (synchronous snapshot test) is the gate check.

Related: ADR-001 (phase helper function), crt-025 ADR-001 (#2998), pattern #3027.
Supersedes: nothing (this extends the crt-025 contract to four new call sites).
