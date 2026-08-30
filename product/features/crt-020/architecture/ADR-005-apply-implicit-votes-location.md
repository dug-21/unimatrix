## ADR-005: apply_implicit_votes Function Location — background.rs Free Function

### Context

The implicit vote sweep for crt-020 requires a function (`apply_implicit_votes`) that
queries the `sessions` and `injection_log` tables, applies `helpful_count` increments via
`record_usage_with_confidence`, and marks processed sessions with `implicit_votes_applied = 1`.
This function must be called from the `maintenance_tick` in `background.rs`.

Two locations were considered:

**Option A — `background.rs` free async function (server crate)**
Place `apply_implicit_votes` as a free `async fn` in
`crates/unimatrix-server/src/background.rs`, alongside `process_auto_quarantine` and
`run_maintenance`. It is called directly from `maintenance_tick`.

**Option B — `implicit_votes.rs` module in the store crate**
Place `apply_implicit_votes` in `crates/unimatrix-store/src/implicit_votes.rs`, exposing
it as a public API that `background.rs` calls via the store crate's public interface.

Key constraints informing this decision:
- C-04 (SPECIFICATION.md): The implicit vote sweep runs inside `spawn_blocking`. Async/await
  orchestration (awaiting the `spawn_blocking` handle, reading `ConfidenceStateHandle` before
  entering the blocking context) lives in the server crate's async runtime.
- C-08: `alpha0`/`beta0` must be snapshotted from `ConfidenceStateHandle` on the async thread
  before entering `spawn_blocking`. `ConfidenceStateHandle` is a server-crate type; the store
  crate has no visibility into it.
- The established pattern for sub-steps in `maintenance_tick` is a free async function in
  `background.rs` (see `process_auto_quarantine` at line 542, `run_maintenance`).
- The store crate must not import server-crate types (`ConfidenceStateHandle`,
  `EffectivenessStateHandle`, etc.) — doing so would invert the dependency graph and create
  a circular crate dependency.

### Decision

Implement `apply_implicit_votes` as a free `async fn` in
`crates/unimatrix-server/src/background.rs`, co-located with the `maintenance_tick` that
calls it. This matches the established pattern of `process_auto_quarantine` and is the only
option that avoids a circular crate dependency.

The function signature follows the same shape as `process_auto_quarantine`:

```rust
async fn apply_implicit_votes(
    store: &Store,
    confidence_state: &ConfidenceStateHandle,
) -> Result<ImplicitVoteSweepStats, ServiceError>
```

The `ConfidenceStateHandle` snapshot (`alpha0`, `beta0`) is read on the async thread before
the function delegates synchronous SQLite work into `spawn_blocking`. No lock is held across
an `await` point (C-08).

The store crate retains only the raw SQLite read/write primitives
(`scan_implicit_vote_candidates`, `record_usage_with_confidence`, `mark_implicit_votes_applied`)
as synchronous functions — consistent with ADR-004 (Synchronous API with spawn_blocking
Delegation, entry #61).

### Consequences

**Easier:**
- No circular crate dependency: the store crate remains unaware of server-layer types.
- Async orchestration (spawning, awaiting, lock discipline) is centralized in the server
  crate where the tokio runtime lives.
- The pattern is consistent with `process_auto_quarantine` — future contributors find the
  function exactly where they expect it.
- `ConfidenceStateHandle` access is straightforward; no threading of server state into the
  store crate is needed.

**Harder:**
- `apply_implicit_votes` cannot be unit-tested in isolation from the server crate's test
  harness. Integration-level tests (using a real `Store` + `ConfidenceStateHandle`) are
  required, matching the pattern used for `process_auto_quarantine` tests.
- The store crate's public API must expose three narrow synchronous helpers for the sweep;
  these are part of the store's public surface even though they serve a single caller.
