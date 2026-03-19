# crt-022: Rayon Thread Pool + Embedding Migration — Architecture

## System Overview

Unimatrix's CPU-bound ML inference has accumulated three documented tokio blocking
pool saturation incidents (#735, #1628, #1688). All stem from the same root cause:
`tokio::task::spawn_blocking` routes CPU-bound ONNX work through a pool sized for
short-lived I/O-bound tasks. Wave 1–3 adds NLI (W1-4), GGUF (W2-4), and GNN (W3-1)
— each new model compounds the saturation problem on the same shared pool.

crt-022 establishes the structural fix: a dedicated `rayon::ThreadPool` in
`unimatrix-server` for all CPU-bound ML inference, bridged to tokio via a oneshot
channel. This feature migrates the existing ONNX embedding model as the first and
lowest-stakes consumer, validating the bridge pattern before W1-4 (NLI) and W2-4
(GGUF) depend on it.

A second structural correction is made in the same feature: `AsyncEmbedService`,
which wraps ONNX inference in `spawn_blocking`, is removed from `unimatrix-core`.
Execution scheduling is a deployment concern; the domain layer stays lean.

## Component Breakdown

### New: `unimatrix-server/src/infra/rayon_pool.rs` — `RayonPool`

The rayon-tokio bridge. Owns an `Arc<rayon::ThreadPool>` and exposes a single
async method `spawn` that submits a closure to rayon and awaits the result via a
`tokio::sync::oneshot` channel. A panic in the closure drops the sender, which
causes `rx.await` to return `Err(RayonError::Cancelled)` — no panic propagation
across thread boundaries.

Responsibilities:
- Wrap a named `rayon::ThreadPool` (pool name: `ml_inference_pool`)
- Expose `pub async fn spawn<F, T>(&self, f: F) -> Result<T, RayonError>`
- Guarantee panic isolation: closure panic → `RayonError::Cancelled`, not tokio crash
- Provide `pool_size()` and `name()` accessors for observability

### Modified: `unimatrix-server/src/infra/config.rs` — `InferenceConfig`

New `[inference]` section added to `UnimatrixConfig`. Follows the same
`#[serde(default)]` pattern as all other sections. Absent section uses compiled
defaults. Field `rayon_pool_size: usize` validated in range `[1, 64]`; out-of-range
aborts startup with a structured `ConfigError`.

### Modified: `unimatrix-server/src/infra/mod.rs`

Exports `RayonPool` and `RayonError` from the `infra` module.

### Removed: `AsyncEmbedService` from `unimatrix-core/src/async_wrappers.rs`

`AsyncEmbedService` wraps `EmbedService` in `spawn_blocking`. It has zero consumers
in `unimatrix-server` (all server call sites use `EmbedAdapter` directly, confirmed
by grep in SCOPE.md research). It is removed entirely. `AsyncVectorStore` is retained
unchanged — HNSW operations remain on `spawn_blocking` (short-duration,
memory-mapped index, not the problem workload).

### Modified: 7 `spawn_blocking` embedding call sites in `unimatrix-server`

Each site replaces `spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, ...)` with
`rayon_pool.spawn(...).await`, mapping `RayonError::Cancelled` to the existing
`ServiceError::EmbeddingFailed` at each call site. See Call-Site Migration Pattern
section for the exact substitution.

### Retained: `unimatrix-server/src/infra/embed_handle.rs` — `EmbedServiceHandle`

`OnnxProvider::new(config)` remains on `spawn_blocking` (model file I/O + ONNX
session initialization — this is I/O and initialization work, not steady-state
inference). The state machine (`Loading → Ready | Failed → Retrying`) is unchanged.
`get_adapter()` continues to return `Arc<EmbedAdapter>`. Only what happens after
`get_adapter()` succeeds is changed: the adapter is passed into `rayon_pool.spawn`
instead of `spawn_blocking`.

## Component Interactions

```
MCP Handler (async, tokio)
    │
    │  get_adapter().await          → EmbedServiceHandle (arc, tokio RwLock)
    │                               → Arc<EmbedAdapter>
    │
    │  rayon_pool.spawn(move || {   → RayonPool
    │      adapter.embed_entry(...)     │
    │  }).await                         │  oneshot::channel
    │                                   │  rayon worker thread
    │  ← Result<Vec<f32>, RayonError>  │  EmbedAdapter → OnnxProvider
    │                                   │    Mutex<Session>.lock()
    │                                   │    session.run(inputs)
    │                                   │  → Vec<f32>
    │                                   │  tx.send(result)
    │  rx.await resolves ◄──────────────┘
    │
    │  map RayonError::Cancelled → ServiceError::EmbeddingFailed
```

Background tasks (contradiction scan, quality-gate embedding) follow the same
pattern but are spawned from background tick tasks rather than MCP handlers. The
`Arc<RayonPool>` is shared across all consumers via `AppState`.

## Technology Decisions

See ADR files for full rationale. Summary:

- **ADR-001**: Rayon pool placed in `unimatrix-server` only (deployment scheduling
  concern, not domain abstraction)
- **ADR-002**: Timeout semantics — `tokio::time::timeout` wrapping `rx.await` at
  `RayonPool::spawn` level, with MCP_HANDLER_TIMEOUT as default parameter
- **ADR-003**: Contradiction scan stays as a single rayon task (not per-entry
  decomposed); pool floor set at 4 threads to bound monopolisation
- **ADR-004**: `AppState` owns `Arc<RayonPool>` (single distribution point for
  W1-4, W2-4, W3-1 reuse)

## Integration Points

- **W1-4 (NLI)**: Shares `ml_inference_pool` established here. NliServiceHandle
  (future) will call `rayon_pool.spawn` exactly as the embedding call sites do now.
- **W2-4 (GGUF)**: Gets a _separate_ pool sized for long-duration inference. The
  `[inference]` section naming accommodates GGUF parameters without rename.
- **W3-1 (GNN)**: Shares `ml_inference_pool` for training runs on the tick.
- **`unimatrix-core`**: Loses `AsyncEmbedService`; gains nothing new. The `async`
  feature and tokio dependency are retained for `AsyncVectorStore`.
- **Config system**: `InferenceConfig` struct follows the dsn-001 `#[serde(default)]`
  pattern established by all other config sections.

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| `RayonPool::spawn` | `pub async fn spawn<F,T>(&self, f: F) -> Result<T, RayonError>` | `infra/rayon_pool.rs` (new) |
| `RayonPool::spawn_with_timeout` | `pub async fn spawn_with_timeout<F,T>(&self, timeout: Duration, f: F) -> Result<T, RayonError>` | `infra/rayon_pool.rs` (new) |
| `RayonError` | `enum { Cancelled, TimedOut }` | `infra/rayon_pool.rs` (new) |
| `RayonPool::new` | `fn new(num_threads: usize, name: &str) -> Result<Self, rayon::ThreadPoolBuildError>` | `infra/rayon_pool.rs` (new) |
| `InferenceConfig` | `struct { rayon_pool_size: usize }` with `Default` | `infra/config.rs` (modified) |
| `UnimatrixConfig::inference` | `pub inference: InferenceConfig` | `infra/config.rs` (modified) |
| `EmbedAdapter::embed_entry` | `fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError>` | `unimatrix-core/src/adapters.rs` (unchanged) |
| `EmbedAdapter::embed_entries` | `fn embed_entries(&self, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>, CoreError>` | `unimatrix-core/src/adapters.rs` (unchanged) |
| `OnnxProvider` | `Send + Sync` (confirmed by `test_send_sync` in `onnx.rs`) | `unimatrix-embed/src/onnx.rs` (unchanged) |
| `EmbedAdapter` is `Send + 'static` | Wraps `Arc<dyn EmbeddingProvider>` — provider is `OnnxProvider` which is `Send+Sync` | `unimatrix-core/src/adapters.rs` |
| `AsyncEmbedService` | **REMOVED** — no consumers in server | `unimatrix-core/src/async_wrappers.rs` |

---

## §thread-safety — SR-01 Resolution: OrtSession Thread Safety Under Rayon

### Analysis

`OnnxProvider` in `unimatrix-embed/src/onnx.rs` wraps ONNX inference with a
`Mutex<Session>` field. The inference methods `embed_single` and `embed_batch_internal`
acquire the mutex (`self.session.lock()`) for the duration of `session.run()`, then
release it before returning the result data.

From the source:
```rust
pub struct OnnxProvider {
    session: Mutex<Session>,      // ← serialises all inference calls
    tokenizer: Tokenizer,         // ← lock-free (&self methods only)
    model: EmbeddingModel,
    config: EmbedConfig,
}
```

The existing test `test_send_sync` in `onnx.rs` asserts `OnnxProvider: Send + Sync`
at compile time:
```rust
fn assert_send_sync<T: Send + Sync>() {}
assert_send_sync::<OnnxProvider>();
```

`EmbedAdapter` wraps `Arc<dyn EmbeddingProvider>`. For `OnnxProvider` as the
provider, `EmbedAdapter` is `Send + 'static` (arc over `Send + Sync` type).

### Decision

**No synchronization change is needed.** The existing `Mutex<Session>` correctly
serialises ONNX inference. Under rayon's work-stealing scheduler, multiple rayon
workers may hold `Arc<EmbedAdapter>` clones concurrently and call `embed_entry`
concurrently. Each call independently acquires `Mutex<Session>` and blocks until
the prior call completes. This is correct serial behaviour with concurrent callers
waiting at the mutex boundary.

The concurrency model does not get worse under rayon compared to multiple
`spawn_blocking` tasks — in both cases, multiple threads may call `embed_entry`
concurrently, and in both cases the `Mutex<Session>` serialises them. The difference
is that rayon's pool is sized for sustained CPU work, not tokio's I/O pool.

**Thread-safety guarantee for downstream agents**: `Arc<EmbedAdapter>` may be
cloned and passed into any number of concurrent rayon closures. Calls to
`embed_entry` on the same underlying `OnnxProvider` are serialised by
`Mutex<Session>`. No data race is possible. Panics in `session.run()` will poison
the mutex; the `.expect("session lock poisoned")` call in `onnx.rs` will panic the
rayon worker thread, which the oneshot bridge converts to `RayonError::Cancelled`.

For W1-4 NLI: the same pattern applies. `NliProvider` (not yet implemented) must
follow the same `Mutex<Session>` wrapping used by `OnnxProvider`.

---

## §timeout-semantics — SR-03 Resolution: Timeout After Rayon Migration

### Problem

`spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, ...)` enforces a 30-second
timeout at 7 ONNX embedding call sites today. The timeout fires via
`tokio::time::timeout` wrapping the `JoinHandle` future. After migration to rayon,
`rayon_pool.spawn(...)` returns a future that resolves when the rayon worker sends
on the oneshot channel. Without a timeout, a hung ONNX session (ORT deadlock,
adversarial input causing extreme inference time) suspends the MCP handler
indefinitely.

### Analysis of Options

**Option A — No timeout at rayon level; rely on work-stealing**

Rayon's work-stealing prevents indefinite *queuing* (a submitted closure will
eventually start executing) but provides no bound on *execution time*. A hung
`session.run()` call inside a rayon closure occupies the worker thread indefinitely
while the `rx.await` in the async caller suspends indefinitely. This removes timeout
coverage that currently exists, regressing against the lesson documented in entry
#1688.

**Option B — `tokio::time::timeout` at each call site**

Each of the 7 call sites wraps `rayon_pool.spawn(...).await` in
`tokio::time::timeout(MCP_HANDLER_TIMEOUT, ...)`. This is functionally correct but
duplicates the timeout logic across 7 sites with no enforcement mechanism. Per entry
#1688, coverage gaps compound across the codebase. A future W1-4 call site will add
an eighth site and may omit the timeout.

**Option C — `spawn_with_timeout` on `RayonPool` (selected)**

`RayonPool` exposes two methods:
- `spawn<F,T>(&self, f: F) -> Result<T, RayonError>` — no timeout
- `spawn_with_timeout<F,T>(&self, timeout: Duration, f: F) -> Result<T, RayonError>`
  — wraps `rx.await` with `tokio::time::timeout`

The 7 MCP call sites (search, store, correct, status, warmup, quality-gate) use
`spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`. The timeout duration is passed
explicitly at each call site — the same pattern used by the current
`spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, ...)` calls today. No hidden
defaults; callers express their timeout requirement explicitly.

The fire-and-forget background tasks (contradiction scan, background quality-gate
loop) use `spawn` without timeout — they are not on the MCP handler path and must
not be killed by a 30-second timeout.

`RayonError` gains a `TimedOut` variant alongside `Cancelled`:
```rust
#[derive(Debug, thiserror::Error)]
pub enum RayonError {
    #[error("rayon worker cancelled (panic or pool shutdown)")]
    Cancelled,
    #[error("rayon inference timed out after {0:?}")]
    TimedOut(std::time::Duration),
}
```

Both variants map to `ServiceError::EmbeddingFailed` at call sites — consistent
with the existing error propagation path.

### Decision

**Option C is selected.** `spawn_with_timeout` on `RayonPool` is the correct
implementation. It:
1. Preserves the timeout coverage that `spawn_blocking_with_timeout` provides today
2. Centralises the timeout mechanism in one place (the bridge) rather than 7 call sites
3. Makes the timeout explicit at each call site (duration passed as argument)
4. Separates background tasks (no timeout) from MCP handler tasks (with timeout)
5. Extends naturally to W1-4 and W2-4 which will add their own call sites

**Important note on timeout semantics**: `tokio::time::timeout` around `rx.await`
cancels the async _wait_ but does not terminate the rayon worker. A hung
`session.run()` continues running on its rayon thread until the ORT session
eventually unblocks (or the process exits). The timeout protects the MCP handler
from indefinite suspension, but the rayon thread remains occupied. This is the
correct tradeoff: pool sizing (see §pool-sizing) ensures the pool is never
fully consumed by hung threads under normal operation.

---

## §pool-sizing — SR-04 Resolution: Contradiction Scan Monopolisation

### Problem

The contradiction scan (`background.rs:543`) wraps its entire entry iteration loop
in a single `rayon_pool.spawn()`. It calls `adapter.embed_entry` for each entry in
sequence, holding one rayon thread for the full scan duration. On a large knowledge
base, this could take minutes.

If the pool has only 2 threads (minimum per SCOPE.md default formula) and the
contradiction scan occupies one, only one thread is available for all concurrent
MCP embedding calls (search, store, correct, status, warmup). Under moderate load,
this can produce queueing delays.

### Analysis of Decomposition Option

Decomposing the contradiction scan into per-entry `rayon_pool.spawn` calls would
enable parallel entry processing but introduces substantial complexity:
- Requires collecting results across parallel tasks (channel or
  `Arc<Mutex<Vec<...>>>`)
- Increases rayon submission overhead by `N` channel round-trips for `N` entries
- The contradiction logic (`scan_contradictions`) is an integrated loop with state;
  decomposition requires refactoring the loop internals
- Parallel embedding calls contend on `Mutex<Session>`, serialising to the same
  throughput as sequential calls anyway — no parallelism benefit for ONNX inference
  (single session, serial access)

Decomposition adds complexity without throughput benefit for ONNX inference.

### Pool Size Derivation

Constraints:
1. The contradiction scan occupies at most 1 rayon thread at a time (single task)
2. The quality-gate embedding loop occupies at most 1 rayon thread at a time
3. MCP embedding calls need at least 2 concurrent threads to avoid queuing under
   moderate load (2 concurrent MCP sessions each embedding in parallel)
4. Total: 1 (scan) + 1 (quality-gate, runs separately) + 2 (MCP) = 4 threads minimum

**Default formula**: `max(num_cpus / 2, 4).min(8)`

This supersedes the SCOPE.md formula `max(num_cpus / 2, 2).min(8)`. The floor is
raised from 2 to 4 to satisfy the constraint above. On a single-core container
(`num_cpus = 1`), the formula yields 4. This is intentionally conservative: ML
inference is CPU-bound and benefits from dedicated threads.

Config range: `[1, 64]`. Operators can tune downward on resource-constrained
deployments (the floor of 4 is a default, not a hard minimum — config validation
allows `rayon_pool_size = 1`).

### Decision

**The contradiction scan remains as a single rayon task.** Pool floor is raised from
2 to 4 to guarantee at least 3 threads available for MCP embedding calls even when
both the contradiction scan and quality-gate embedding loop are active simultaneously.

The `[inference]` config section allows operators to tune `rayon_pool_size` upward
on well-resourced deployments.

---

## Call-Site Migration Pattern

The migration substitution at every ONNX embedding call site is:

**Before:**
```rust
let result = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
    let adapter = Arc::clone(&adapter);
    move || adapter.embed_entry(&title, &content)
})
.await
.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;
```

**After:**
```rust
let result = self.rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, {
        let adapter = Arc::clone(&adapter);
        move || adapter.embed_entry(&title, &content)
    })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?;
```

The double `.map_err` pattern is unchanged: the outer `?` maps `RayonError` (bridge
failure) and the inner `?` maps `CoreError` (embed failure). The error types at each
layer are the same as before.

For background tasks without timeout:
```rust
let result = rayon_pool
    .spawn({
        let adapter = Arc::clone(&adapter);
        move || adapter.embed_entry(&title, &content)
    })
    .await
    .map_err(|e| /* handle cancellation */)?;
```

---

## Pool Distribution via `AppState`

`Arc<RayonPool>` is held on a top-level `AppState` struct (or equivalent startup
wiring struct) and passed to all inference consumers at startup. This prevents
accidental re-instantiation and satisfies Constraint 5 (single shared pool for W1-2).

W1-4 (NLI), W2-4 (GGUF's separate pool), and W3-1 (GNN) all extend `AppState`
with their own pool or service handles. The pattern is established here.

---

## Phase Summary

| Phase | Deliverable |
|-------|-------------|
| 1 — Pool infrastructure | `rayon_pool.rs` + `InferenceConfig` + startup wiring |
| 2 — Crate cleanup | Remove `AsyncEmbedService` from `unimatrix-core` |
| 3 — Call-site migration | 7 sites in `unimatrix-server` |
| 4 — CI enforcement | grep-based check: no `spawn_blocking` for ONNX inference |

---

## Open Questions

None. All architectural questions from SCOPE.md and the risk assessment have been
resolved by this document and the associated ADRs:
- OQ-1 (pool naming): resolved as `ml_inference_pool` (human-approved)
- OQ-2 (timeout semantics): resolved in §timeout-semantics (ADR-002)
- SR-01 (OrtSession thread safety): resolved in §thread-safety
- SR-03 (timeout coverage gap): resolved in §timeout-semantics (ADR-002)
- SR-04 (scan monopolisation): resolved in §pool-sizing (ADR-003)
- SR-06 (pool distribution): resolved in Pool Distribution section (ADR-004)
