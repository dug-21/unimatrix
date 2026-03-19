## ADR-002: Timeout Enforced via `spawn_with_timeout` on `RayonPool`, Not at Each Call Site

### Context

Seven call sites in `unimatrix-server` currently use `spawn_blocking_with_timeout`
with `MCP_HANDLER_TIMEOUT` (30 seconds). This timeout prevents MCP handlers from
suspending indefinitely when the tokio blocking pool is saturated or when a blocking
call hangs.

After migration to rayon, the `spawn_blocking_with_timeout` call is replaced. Three
options exist for restoring timeout coverage:

**Option A — No timeout; rely on rayon work-stealing**
Rayon's work-stealing prevents indefinite queuing but does not bound execution time.
A hung `session.run()` inside a rayon closure suspends `rx.await` in the async
caller indefinitely. This removes timeout coverage that currently exists. Entry
#1688 documents the lesson: timeout coverage gaps introduced at one call site
compound across the codebase. Option A is rejected.

**Option B — `tokio::time::timeout` at each call site**
Each of the 7 call sites wraps `rayon_pool.spawn(...).await` in
`tokio::time::timeout`. Functionally correct, but duplicates timeout logic across 7
sites. W1-4 (NLI) will add an eighth site. W2-4 (GGUF) will add more. There is no
enforcement mechanism to guarantee future sites include the timeout. Option B is
rejected as insufficiently robust against future omissions.

**Option C — `spawn_with_timeout` method on `RayonPool`**
`RayonPool` exposes two methods:
- `spawn<F,T>` — submits closure, awaits result, no timeout
- `spawn_with_timeout<F,T>(timeout: Duration, f: F)` — wraps `rx.await` in
  `tokio::time::timeout`

MCP handler call sites use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`.
Background tasks (contradiction scan, quality-gate loop) use `spawn` without timeout
— they are not on the MCP handler path and a 30-second timeout would incorrectly
kill a multi-minute scan.

`RayonError` carries two variants:
```rust
pub enum RayonError {
    Cancelled,          // panic in closure, or pool shutdown
    TimedOut(Duration), // timeout elapsed before closure completed
}
```

Both map to `ServiceError::EmbeddingFailed` at call sites.

### Decision

**Option C is adopted.** `RayonPool::spawn_with_timeout` is the primary entry point
for MCP handler inference calls. `RayonPool::spawn` (no timeout) is used only for
fire-and-forget background tasks that must not be interrupted by an arbitrary
wall-clock limit.

The timeout passed to `spawn_with_timeout` is explicit at each call site (the
duration is a parameter, not a hidden default). The current 7 sites all pass
`MCP_HANDLER_TIMEOUT` (30 seconds). This is the same value used today and preserves
the existing timeout semantics exactly.

**Caveat on semantics**: `tokio::time::timeout` around `rx.await` cancels the async
wait but does not terminate the rayon worker. A hung `session.run()` continues on
its rayon thread. The timeout protects the MCP handler from indefinite suspension;
it does not reclaim the rayon thread. Pool sizing (ADR-003) accounts for this by
provisioning enough threads that a small number of hung threads do not starve
concurrent MCP inference.

### Consequences

Easier:
- Timeout is enforced in one location (the bridge method) rather than 7 call sites
- Future call sites for W1-4 (NLI) can choose `spawn_with_timeout` for MCP paths
  and `spawn` for background paths — the correct choice is explicit in the API
- `MCP_HANDLER_TIMEOUT` remains the canonical constant; call sites pass it by name

Harder:
- `RayonPool` API is slightly larger (two methods instead of one)
- A future call site author must choose between `spawn` and `spawn_with_timeout`;
  both are correct depending on context. The convention must be documented.

**Convention**: MCP handler paths always use `spawn_with_timeout`. Background tasks
that must run to completion use `spawn`. This convention must be documented in the
`RayonPool` module-level rustdoc.
