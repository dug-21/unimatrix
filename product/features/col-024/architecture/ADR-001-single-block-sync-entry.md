## ADR-001: Single `block_sync` Entry per `load_cycle_observations` Invocation

### Context

`ObservationSource` is a sync trait whose implementations bridge async sqlx via
`block_sync` (which calls `tokio::task::block_in_place` when inside a runtime, or
spins up a transient runtime when outside one). All existing methods call
`block_sync` once and execute their full async work — including multi-step SQL
loops — inside a single `block_sync` closure.

`load_cycle_observations` must execute three SQL operations:
1. Fetch `cycle_events` rows for the `cycle_id`.
2. For each time window, fetch distinct `session_id` values from `observations`
   via `topic_signal` + timestamp range.
3. Fetch all observation records for the collected session IDs, filtered by the
   combined window range.

SR-02 raises the risk that a per-window loop calling `block_sync` repeatedly
causes multiple entries into `block_in_place`, or, outside a runtime, creates and
drops multiple transient runtimes. The existing `load_unattributed_sessions`
method already executes a two-query loop (sessions then observations) inside one
`block_sync` closure without issue.

### Decision

The entire `load_cycle_observations` implementation — including the Step 2
per-window query loop — runs inside a single `block_sync` call. The async closure
passed to `block_sync` awaits all three steps sequentially using `.await` within
the single async block.

This matches the pattern already established by `load_feature_observations` and
`load_unattributed_sessions`, which also execute two queries inside one `block_sync`
without per-query re-entry.

Concretely:
```rust
fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>> {
    let pool = self.store.write_pool_server();
    block_sync(async {
        // Step 1: fetch windows
        // Step 2: per-window session discovery (loop with .await inside async block)
        // Step 3: load observations + Rust filter
        // return Ok(records)
    })
}
```

### Consequences

- Easier: `block_sync` is entered once per `load_cycle_observations` call,
  preserving the same blocking behaviour as all other trait methods and avoiding
  double-`block_in_place` panics.
- Easier: the implementation is structurally consistent with existing methods,
  reducing review surface.
- Harder: the async closure may hold the async block open across multiple SQL
  round-trips; this is acceptable because `context_cycle_review` is called once
  per retrospective (not on the hot observation-write path).
