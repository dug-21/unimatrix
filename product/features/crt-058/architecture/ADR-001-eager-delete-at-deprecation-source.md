## ADR-001: Eager Delete at the Deprecation Event, Tick as Backstop

### Context

At `context_deprecate` (bare, successor-less flip), agent-authored `graph_edges` touching the entry become live dangling references. They are currently removed only by the `EveryTick` orphaned-edge compaction (`background.rs:805`), up to ~900s later. `context_correct` already resolves the analogous condition eagerly by repointing inbound edges to the Active successor (`repoint_deprecated_target_edges`); deprecation has no successor, so the parallel corrective action is to **delete**.

Options: (A) leave it entirely to the tick (status quo); (B) delete eagerly at the deprecation event for the one entry, keeping the tick as backstop.

### Decision

**Option B.** In the `context_deprecate` handler, after the step-5 idempotency return and the step-6 flip, run one synchronous, non-fatal, indexed delete of the entry's agent-authored edges (both directions) on `write_pool_server()`. The `EveryTick` compaction is **unchanged** and remains the backstop for: entries deprecated before this feature shipped, machine-generated edges (left to the tick by design), and any eager-delete failure.

The eager delete is a pure latency optimization — it does *now* what the tick does within ~900s. It is **non-fatal**: on error, log once at `warn!`, omit the count advisory, and return the normal deprecation success. It never propagates an error into the deprecation result (mirrors `confidence.recompute` and `audit_fire_and_forget`).

**Standing coupling invariant (SR-05):** the swallowed-failure safety depends on `run_orphaned_edge_compaction` remaining the blanket backstop. Any future change that removes or narrows the compaction must re-verify that a swallowed eager-delete failure is still swept. The `warn!` on failure is a genuine failure signal, not an expected-suppressed one — keep it at `warn`, not `debug` (#3448).

### Consequences

Easier: dependency data is accurate the instant an entry is retired, not up to a tick later. No new table, migration, or lifecycle; reuses `graph_edges`, `write_pool_server()`, and existing endpoint indexes. Idempotent re-deprecation does no work (sits past the step-5 guard).

Harder: introduces a second delete pass over `graph_edges` keyed differently from the tick, creating a divergence risk that must be actively constrained (eager ⊆ tick — ADR-003). Adds a standing coupling to the compaction that future maintainers must respect (SR-05).
