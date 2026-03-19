## ADR-002: EvalServiceLayer Suppresses Analytics Queue at Construction (AnalyticsMode::Suppressed)

### Context

`ServiceLayer` construction (in both `main.rs` and `TestHarness`) wires up the
analytics write queue. Internally, `SqlxStore::open()` spawns a drain task
(`spawn_drain_task`) that consumes from a bounded `mpsc::channel` and writes to the
analytics tables (`co_access`, `sessions`, `injection_log`, `query_log`, `injection_log`).

When the eval runner (`eval run`) constructs an `EvalServiceLayer` against a snapshot
database opened with `?mode=ro`, the SQLite layer will ultimately reject any write
attempts from the drain task. However:

1. The in-memory `analytics_tx` channel still accepts events from `enqueue_analytics`.
2. The drain task is still spawned, runs, attempts writes, and accumulates errors.
3. The shed counter increments spuriously.
4. Fire-and-forget calls to `enqueue_analytics` do not surface errors to the caller,
   so the read-only enforcement appears to be working while silently failing in the
   drain task (SR-07).

The requirement is that `eval run` produces no writes to the snapshot database (AC-05).
The read-only SQLite mode alone does not satisfy the intent of AC-05 — it only
prevents the physical write; it does not stop the in-memory queue from being populated
or the drain task from running.

Two options were evaluated:

**Option A — No-op at the `SqlxStore` level**: Add an `open_readonly` constructor to
`SqlxStore` that creates a no-op `analytics_tx` sender (channel capacity 0, or an
unconnected sender). This modifies the store API.

**Option B — `AnalyticsMode` enum on `EvalServiceLayer`**: Define `AnalyticsMode` in
the eval module. When `Suppressed`, `EvalServiceLayer::from_profile()` does not call
`SqlxStore::open()` at all — it constructs a raw read-only `sqlx::SqlitePool` and
builds a `ServiceLayer` variant that never calls `enqueue_analytics`. The SCOPE.md
already resolved that eval would use a raw pool rather than `SqlxStore::open()`.

### Decision

Use `AnalyticsMode::Suppressed` via a raw read-only `sqlx::SqlitePool` (Option B).

The SCOPE.md pre-design resolution states: "The eval runner constructs a raw
`sqlx::SqlitePool` with `SqliteConnectOptions::read_only(true)` directly. This keeps
eval infrastructure self-contained." Option B is consistent with that resolution and
avoids modifying the `SqlxStore` API (which is used throughout the production path).

`EvalServiceLayer::from_profile()` constructs a read-only pool, builds a
`ServiceLayer` using `ServiceLayer::with_rate_config` (the `TestHarness` path, not
the `SqlxStore::open()` path), and never registers or calls `enqueue_analytics`. The
drain task is never spawned. The `AnalyticsMode::Suppressed` variant is the explicit
type-level declaration that this `ServiceLayer` instance does not write analytics.

The `AnalyticsMode::Live` variant exists for future use (e.g., an `eval live` mode
against a live database where analytics recording is acceptable), but is not used in
nan-007.

### Consequences

- AC-05 is fully satisfied: the snapshot database receives zero writes during `eval run`.
- No `SqlxStore` API changes required.
- Eval infrastructure is self-contained in `crates/unimatrix-server/src/eval/`.
- The `AnalyticsMode` type is visible documentation that eval suppresses analytics —
  this cannot be accidentally removed without changing the type.
- Usage recording (confidence feedback, helpfulness signals) does not occur during eval
  replay. This is correct behaviour — eval replay should not affect production confidence
  scores.
