# Component Pseudocode: `RayonPool` + `RayonError`

**File to create**: `crates/unimatrix-server/src/infra/rayon_pool.rs`

---

## Purpose

The rayon-tokio bridge. Owns a named `rayon::ThreadPool` and exposes two async methods
that submit closures to the pool, then await the result via a `tokio::sync::oneshot`
channel. A panic inside the closure drops the sender; `rx.await` returns
`Err(RecvError)`, which the bridge maps to `RayonError::Cancelled` — no panic
propagates into the tokio runtime.

This is the single shared pool for all CPU-bound ML inference (ONNX embedding,
future NLI at W1-4, future GNN at W3-1). MCP handler paths must use
`spawn_with_timeout`; background tasks must use `spawn` (no timeout).

---

## Module-level Rustdoc Convention (required by ADR-002 / R-04)

The module-level doc comment must state the usage convention:

```
/// Rayon-tokio bridge for CPU-bound ML inference.
///
/// # Usage convention (ADR-002)
///
/// - MCP handler call sites MUST use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, f)`.
///   This preserves the timeout coverage that `spawn_blocking_with_timeout` provided
///   before migration. A hung ONNX session will not suspend the MCP handler indefinitely.
///
/// - Background tasks (contradiction scan, quality-gate embedding loop) MUST use
///   `spawn(f)` with no timeout. These are fire-and-forget tasks that must run to
///   completion regardless of wall-clock time. A 30-second timeout would incorrectly
///   kill a multi-minute contradiction scan.
///
/// # Panic containment
///
/// A panic inside any closure passed to `spawn` or `spawn_with_timeout` is contained
/// at the oneshot channel boundary. The panicking rayon worker drops `tx`; `rx.await`
/// returns `Err(RecvError)`; the bridge maps this to `Err(RayonError::Cancelled)`.
/// No `std::panic::catch_unwind` is needed. The pool remains operational after a panic.
///
/// # Mutex poisoning
///
/// If the closure holds a `Mutex` (e.g., `OnnxProvider::Mutex<Session>`) and panics,
/// that mutex is poisoned. Recovery from mutex poisoning in `OnnxProvider` is the
/// responsibility of `EmbedServiceHandle`, not of this bridge. This bridge's contract
/// is: panic → `Cancelled`.
```

---

## Data Structures

### `RayonPool`

```
struct RayonPool {
    ml_inference_pool: Arc<rayon::ThreadPool>,
    // name and size stored for observability accessors
    pool_name: String,
    pool_threads: usize,
}
```

Held as `Arc<RayonPool>` on `AppState` / `ServiceLayer` (ADR-004). Never constructed
more than once per server process (C-05).

### `RayonError`

```
#[derive(Debug, thiserror::Error)]
enum RayonError {
    #[error("rayon worker cancelled (panic or pool shutdown)")]
    Cancelled,

    #[error("rayon inference timed out after {0:?}")]
    TimedOut(std::time::Duration),
}
```

Both variants map to `ServiceError::EmbeddingFailed(e.to_string())` at every call site.

---

## New / Modified Functions

### `RayonPool::new`

```
pub fn new(num_threads: usize, name: &str) -> Result<Self, rayon::ThreadPoolBuildError>
```

Algorithm:

```
1. Call rayon::ThreadPoolBuilder::new()
2.   .num_threads(num_threads)
3.   .thread_name(|i| format!("{}-{}", name, i))  -- names threads for observability
4.   .build()
5.   returns Err(rayon::ThreadPoolBuildError) on failure (propagated to caller)
6. On Ok(pool):
7.   return Ok(RayonPool {
8.       ml_inference_pool: Arc::new(pool),
9.       pool_name: name.to_string(),
10.      pool_threads: num_threads,
11.  })
```

Caller (main.rs) propagates `ThreadPoolBuildError` as
`ServerStartupError::InferencePoolInit(err)`. No silent fallback.

### `RayonPool::spawn`

```
pub async fn spawn<F, T>(&self, f: F) -> Result<T, RayonError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
```

Algorithm:

```
1. let (tx, rx) = tokio::sync::oneshot::channel::<T>()
2. let pool = Arc::clone(&self.ml_inference_pool)
3. pool.spawn(move || {
4.     let result = f()          -- execute the closure on the rayon worker thread
5.     let _ = tx.send(result)   -- send result; if rx is dropped (timeout), this is silently ignored
6. })
7. match rx.await {
8.     Ok(value)  => return Ok(value)
9.     Err(_)     => return Err(RayonError::Cancelled)
10.                  -- Err(_) means tx was dropped:
11.                  -- either closure panicked (dropped tx before send)
12.                  -- or pool shut down before closure ran
13. }
```

Note on step 5: `tx.send` is called with the closure result. If `rx` has been
dropped (e.g., because the caller was cancelled), `send` returns `Err(result)` and
the result is discarded. This is not an error — the pool thread completes normally.

Note on step 4+panic: if `f()` panics, the unwinding drops `tx` without calling
`tx.send`. `rx.await` then returns `Err(RecvError)`, producing `Cancelled`. The
rayon pool recycles the panicking thread and remains operational.

### `RayonPool::spawn_with_timeout`

```
pub async fn spawn_with_timeout<F, T>(
    &self,
    timeout: Duration,
    f: F,
) -> Result<T, RayonError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
```

Algorithm:

```
1. let (tx, rx) = tokio::sync::oneshot::channel::<T>()
2. let pool = Arc::clone(&self.ml_inference_pool)
3. pool.spawn(move || {
4.     let result = f()
5.     let _ = tx.send(result)
6. })
7. match tokio::time::timeout(timeout, rx).await {
8.     Ok(Ok(value))   => return Ok(value)
9.     Ok(Err(_))      => return Err(RayonError::Cancelled)
10.                       -- tx was dropped; closure panicked or pool shut down
11.    Err(_timeout)   => return Err(RayonError::TimedOut(timeout))
12.                       -- timeout elapsed before rx resolved
13.                       -- IMPORTANT: the rayon worker continues running
14.                       -- the pool thread is occupied until ORT session unblocks
15. }
```

The timeout protects the MCP handler from indefinite suspension. It does NOT
terminate the rayon worker. Pool sizing (floor=4, ADR-003) ensures a small number
of hung threads do not starve all MCP inference.

### `RayonPool::pool_size`

```
pub fn pool_size(&self) -> usize {
    self.pool_threads
}
```

Returns the configured thread count. Does not query the pool for idle thread count.
Used in observability and in integration tests (R-02 scenario 3).

### `RayonPool::name`

```
pub fn name(&self) -> &str {
    &self.pool_name
}
```

Returns the pool name (`"ml_inference_pool"`). Used in log messages and tests.

---

## State Machines

None. `RayonPool` is stateless after construction. The rayon thread pool manages its
own internal work-stealing queue. `RayonPool` is a thin wrapper over `Arc<rayon::ThreadPool>`.

---

## Initialization Sequence

`RayonPool` is constructed in `main.rs` startup wiring, after `InferenceConfig::validate()`
succeeds, before `ServiceLayer::new` is called:

```
validate InferenceConfig (abort on out-of-range)
↓
pool = RayonPool::new(config.inference.rayon_pool_size, "ml_inference_pool")
         on Err → ServerStartupError::InferencePoolInit(e) → return Err (abort startup)
         on Ok  → pool
↓
arc_pool = Arc::new(pool)
↓
ServiceLayer::new(..., Arc::clone(&arc_pool), ...)
```

The `Arc<RayonPool>` is stored on `AppState` / `ServiceLayer` and distributed
to all consumers. No subsystem constructs a second `RayonPool` (C-05, ADR-004).

---

## Error Handling

| Error | Source | Propagation |
|-------|--------|-------------|
| `rayon::ThreadPoolBuildError` | `RayonPool::new` | Wrapped in `ServerStartupError::InferencePoolInit`; process exits |
| `RayonError::Cancelled` | closure panic or pool shutdown | `.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?` at call site |
| `RayonError::TimedOut(d)` | `spawn_with_timeout` timeout | `.map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?` at call site |

Background tasks handle `RayonError::Cancelled` by emitting `error!()` tracing
and continuing (not propagating to abort the background tick). See call_site_migration.md.

---

## Module Export

`infra/mod.rs` must add:

```
pub mod rayon_pool;
pub use rayon_pool::{RayonPool, RayonError};
```

---

## Key Test Scenarios (AC-11, Risk Register)

1. **Successful dispatch** (AC-11 #1): call `RayonPool::spawn`, closure returns a value;
   assert `Ok(value)` received.

2. **Panic containment via `spawn`** (AC-11 #2, AC-03, R-01): spawn a closure containing
   `panic!("deliberate")` via `spawn`; assert result is `Err(RayonError::Cancelled)`;
   assert the test runtime does not abort.

3. **Panic containment via `spawn_with_timeout`** (R-01 scenario 2): same as above but
   using `spawn_with_timeout` with a long timeout; assert `Cancelled` before the timeout fires.

4. **Pool with 1 thread** (AC-11 #3): `RayonPool::new(1, "test")` succeeds; dispatch a
   closure; assert `Ok`.

5. **Pool with 8 threads** (AC-11 #4): `RayonPool::new(8, "test")` succeeds; dispatch
   8 concurrent closures; assert all resolve `Ok`.

6. **Timeout fires** (R-02 scenario 1): create pool with 1 thread; dispatch a closure that
   sleeps longer than the timeout; `spawn_with_timeout` must return `Err(RayonError::TimedOut)`.

7. **Pool remains functional after timeout** (R-02 scenario 2): after a timed-out closure
   occupies the thread, dispatch a new short closure on a larger pool; it completes.

8. **Pool does not deadlock under full occupancy** (R-08 scenario 2): create pool with 4
   threads; dispatch 4 long closures; dispatch a 5th short closure; assert 5th completes
   after threads free.

9. **Pool remains operational after panic** (R-01 scenario 3): spawn 1 panicking closure,
   then spawn 10 normal closures on the same pool; assert all 10 succeed.

10. **Pool shutdown drops queued tx** (edge case from RISK-TEST-STRATEGY.md): drop the
    `RayonPool`; assert any pending `rx.await` returns `Err(Cancelled)`, not a hang.
