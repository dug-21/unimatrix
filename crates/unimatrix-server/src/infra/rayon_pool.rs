//! Rayon-tokio bridge for CPU-bound ML inference.
//!
//! # Usage convention (ADR-002)
//!
//! - MCP handler call sites MUST use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, f)`.
//!   This preserves the timeout coverage that `spawn_blocking_with_timeout` provided
//!   before migration. A hung ONNX session will not suspend the MCP handler indefinitely.
//!
//! - Background tasks (contradiction scan, quality-gate embedding loop) MUST use
//!   `spawn(f)` with no timeout. These are fire-and-forget tasks that must run to
//!   completion regardless of wall-clock time. A 30-second timeout would incorrectly
//!   kill a multi-minute contradiction scan.
//!
//! # Panic containment
//!
//! A panic inside any closure passed to `spawn` or `spawn_with_timeout` is contained
//! at the oneshot channel boundary. The panicking rayon worker drops `tx`; `rx.await`
//! returns `Err(RecvError)`; the bridge maps this to `Err(RayonError::Cancelled)`.
//! No `std::panic::catch_unwind` is needed. The pool remains operational after a panic.
//!
//! # Mutex poisoning
//!
//! If the closure holds a `Mutex` (e.g., `OnnxProvider::Mutex<Session>`) and panics,
//! that mutex is poisoned. Recovery from mutex poisoning in `OnnxProvider` is the
//! responsibility of `EmbedServiceHandle`, not of this bridge. This bridge's contract
//! is: panic → `Cancelled`.

use std::sync::Arc;
use std::time::Duration;

// ---------------------------------------------------------------------------
// RayonError
// ---------------------------------------------------------------------------

/// Errors produced by the rayon-tokio bridge.
///
/// Both variants map to `ServiceError::EmbeddingFailed(e.to_string())` at every
/// call site.
#[derive(Debug, thiserror::Error)]
pub enum RayonError {
    /// The rayon worker was cancelled — either the closure panicked (dropping
    /// the sender before sending) or the pool shut down before the closure ran.
    #[error("rayon worker cancelled (panic or pool shutdown)")]
    Cancelled,

    /// `spawn_with_timeout` elapsed before the rayon worker sent its result.
    /// The rayon thread continues running; only the async wait is cancelled.
    #[error("rayon inference timed out after {0:?}")]
    TimedOut(Duration),
}

// ---------------------------------------------------------------------------
// RayonPool
// ---------------------------------------------------------------------------

/// A named rayon thread pool bridged to tokio via oneshot channels.
///
/// Held as `Arc<RayonPool>` on `AppState` / `ServiceLayer` (ADR-004).
/// Never constructed more than once per server process (C-05).
#[derive(Debug)]
pub struct RayonPool {
    ml_inference_pool: Arc<rayon::ThreadPool>,
    pool_name: String,
    pool_threads: usize,
}

impl RayonPool {
    /// Construct a named rayon thread pool with the given thread count.
    ///
    /// Thread names are set to `{name}-{i}` for observability in profilers
    /// and crash reports.
    ///
    /// # Errors
    ///
    /// Returns `rayon::ThreadPoolBuildError` if rayon cannot spawn the threads.
    /// Callers in `main.rs` should wrap this as `ServerStartupError::InferencePoolInit`.
    pub fn new(num_threads: usize, name: &str) -> Result<Self, rayon::ThreadPoolBuildError> {
        let pool_name = name.to_string();
        let thread_prefix = pool_name.clone();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .thread_name(move |i| format!("{}-{}", thread_prefix, i))
            // Install a no-op panic handler so that panics in rayon worker threads do
            // not propagate to the calling thread or abort the process. Panic containment
            // is handled via the oneshot channel drop: if `f()` panics, `tx` is dropped
            // before `send` is called, causing `rx.await` to return `Err(RecvError)`,
            // which the bridge maps to `RayonError::Cancelled`. No additional propagation
            // is needed or desired.
            .panic_handler(|_payload| {
                // Intentionally discard. The oneshot channel drop signals Cancelled to
                // the async caller. The rayon worker thread unwinds normally and is
                // recycled by the pool.
            })
            .build()?;

        Ok(RayonPool {
            ml_inference_pool: Arc::new(pool),
            pool_name,
            pool_threads: num_threads,
        })
    }

    /// Submit a closure to the rayon pool and await its result via a oneshot channel.
    ///
    /// No timeout. Use for background tasks (contradiction scan, quality-gate loop)
    /// that must not be killed by a wall-clock deadline.
    ///
    /// # Panic containment
    ///
    /// If `f` panics, the sender is dropped before `send` is called; `rx.await`
    /// returns `Err(RecvError)`, which is mapped to `Err(RayonError::Cancelled)`.
    /// The rayon pool recycles the panicking thread and remains operational.
    pub async fn spawn<F, T>(&self, f: F) -> Result<T, RayonError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<T>();
        let pool = Arc::clone(&self.ml_inference_pool);

        pool.spawn(move || {
            let result = f();
            // If rx was already dropped (caller cancelled), send returns Err(result).
            // Silently discard — the pool thread completes normally.
            let _ = tx.send(result);
        });

        match rx.await {
            Ok(value) => Ok(value),
            // tx was dropped: closure panicked or pool shut down before closure ran.
            Err(_) => Err(RayonError::Cancelled),
        }
    }

    /// Submit a closure to the rayon pool and await its result with a timeout.
    ///
    /// Use for MCP handler paths (search, store, correct, status, warmup) to preserve
    /// the timeout coverage that `spawn_blocking_with_timeout` provided before migration.
    ///
    /// # Timeout semantics
    ///
    /// `tokio::time::timeout` cancels the async *wait*, not the rayon worker. A hung
    /// `session.run()` continues on its rayon thread until ORT unblocks. Pool sizing
    /// (floor = 4, ADR-003) ensures a small number of hung threads do not starve all
    /// MCP inference.
    pub async fn spawn_with_timeout<F, T>(&self, timeout: Duration, f: F) -> Result<T, RayonError>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel::<T>();
        let pool = Arc::clone(&self.ml_inference_pool);

        pool.spawn(move || {
            let result = f();
            let _ = tx.send(result);
        });

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(value)) => Ok(value),
            // tx was dropped: closure panicked or pool shut down.
            Ok(Err(_)) => Err(RayonError::Cancelled),
            // Timeout elapsed before rx resolved. The rayon thread continues running.
            Err(_elapsed) => Err(RayonError::TimedOut(timeout)),
        }
    }

    /// Returns the configured number of threads in the pool.
    ///
    /// This reflects the construction-time value, not current idle thread count.
    pub fn pool_size(&self) -> usize {
        self.pool_threads
    }

    /// Returns the pool name supplied at construction time.
    pub fn name(&self) -> &str {
        &self.pool_name
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier, Mutex};
    use std::time::Duration;

    // -----------------------------------------------------------------------
    // §pool-init — Pool Initialisation
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pool_init_single_thread() {
        let pool = RayonPool::new(1, "test_pool").expect("pool construction must succeed");
        assert_eq!(pool.pool_size(), 1);
        assert_eq!(pool.name(), "test_pool");
    }

    #[tokio::test]
    async fn test_pool_init_eight_threads() {
        let pool = RayonPool::new(8, "test_pool").expect("pool construction must succeed");
        assert_eq!(pool.pool_size(), 8);
    }

    #[tokio::test]
    async fn test_pool_init_default_formula() {
        use crate::infra::config::InferenceConfig;
        let expected = (num_cpus::get() / 2).max(4).min(8);
        let config = InferenceConfig::default();
        assert_eq!(
            config.rayon_pool_size, expected,
            "InferenceConfig default must match ADR-003 formula (num_cpus/2).max(4).min(8)"
        );
    }

    #[tokio::test]
    async fn test_pool_init_name_retained() {
        let pool = RayonPool::new(4, "ml_inference_pool").unwrap();
        assert_eq!(pool.name(), "ml_inference_pool");
    }

    // -----------------------------------------------------------------------
    // §spawn-dispatch — Successful Dispatch
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_spawn_returns_closure_value() {
        let pool = RayonPool::new(2, "test").unwrap();
        let result = pool.spawn(|| 42u64).await;
        assert_eq!(result.unwrap(), 42u64);
    }

    #[tokio::test]
    async fn test_spawn_with_timeout_returns_closure_value() {
        let pool = RayonPool::new(2, "test").unwrap();
        let result = pool
            .spawn_with_timeout(Duration::from_secs(5), || 99u64)
            .await;
        assert_eq!(result.unwrap(), 99u64);
    }

    #[tokio::test]
    async fn test_spawn_sends_complex_type() {
        let pool = RayonPool::new(2, "test").unwrap();
        let expected: Vec<u8> = vec![1, 2, 3, 4, 5];
        let data = expected.clone();
        let result = pool.spawn(move || data).await;
        assert_eq!(result.unwrap(), expected);
    }

    // -----------------------------------------------------------------------
    // §panic-containment — Panic Safety
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_spawn_panic_returns_cancelled() {
        let pool = RayonPool::new(2, "test").unwrap();
        let result = pool.spawn(|| panic!("deliberate test panic")).await;
        assert!(
            matches!(result, Err(RayonError::Cancelled)),
            "panic in closure must produce Cancelled, got: {:?}",
            result
        );
        // Test runtime continues — if we reach here, tokio did not abort.
    }

    #[tokio::test]
    async fn test_spawn_with_timeout_panic_returns_cancelled_not_timeout() {
        let pool = RayonPool::new(2, "test").unwrap();
        let result = pool
            .spawn_with_timeout(Duration::from_secs(5), || -> u64 { panic!("test panic") })
            .await;
        assert!(
            matches!(result, Err(RayonError::Cancelled)),
            "panic must produce Cancelled (not TimedOut), got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_pool_functional_after_panic() {
        let pool = RayonPool::new(2, "test").unwrap();

        // Trigger two panics.
        let _ = pool.spawn(|| -> u32 { panic!("p1") }).await;
        let _ = pool.spawn(|| -> u32 { panic!("p2") }).await;

        // Pool must remain operational — run 10 normal closures.
        for _ in 0..10 {
            let result = pool.spawn(|| 1u32).await;
            assert_eq!(
                result.unwrap(),
                1u32,
                "pool must be functional after panics"
            );
        }
    }

    #[tokio::test]
    async fn test_spawn_panic_with_mutex_held() {
        let pool = RayonPool::new(2, "test").unwrap();
        let mutex = Arc::new(Mutex::new(0u32));
        let mutex_clone = Arc::clone(&mutex);

        let result = pool
            .spawn(move || {
                let _guard = mutex_clone.lock().unwrap();
                panic!("poison the mutex");
            })
            .await;

        assert!(
            matches!(result, Err(RayonError::Cancelled)),
            "bridge contract: panic → Cancelled regardless of what closure holds"
        );

        // The mutex should now be poisoned.
        assert!(
            mutex.lock().is_err(),
            "mutex must be poisoned after panic inside rayon closure"
        );
    }

    // -----------------------------------------------------------------------
    // §timeout-semantics — Timeout Behaviour
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_spawn_with_timeout_fires_when_closure_exceeds_timeout() {
        let pool = RayonPool::new(2, "test").unwrap();

        // Outer deadline: 10 seconds. The closure sleeps for 10 seconds but the
        // timeout should fire after 50ms.
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            pool.spawn_with_timeout(Duration::from_millis(50), || {
                std::thread::sleep(Duration::from_secs(10));
                0u64
            }),
        )
        .await
        .expect("outer timeout must not fire — 10s is plenty for the 50ms internal timeout");

        assert!(
            matches!(result, Err(RayonError::TimedOut(_))),
            "expected TimedOut, got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_spawn_timeout_duration_preserved() {
        let pool = RayonPool::new(2, "test").unwrap();
        let timeout_dur = Duration::from_millis(100);

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            pool.spawn_with_timeout(timeout_dur, || {
                std::thread::sleep(Duration::from_secs(5));
                0u32
            }),
        )
        .await
        .expect("outer timeout must not fire");

        match result {
            Err(RayonError::TimedOut(d)) => assert_eq!(
                d, timeout_dur,
                "TimedOut must carry the configured duration"
            ),
            other => panic!("expected TimedOut(100ms), got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_pool_accepts_new_submissions_after_timeout() {
        // 2-thread pool: both threads occupied by long-running closures (5s each).
        // Spawn them with 50ms timeout → both return TimedOut promptly.
        // Then spawn a short closure. It enqueues and completes when a thread frees.
        let pool = Arc::new(RayonPool::new(2, "test").unwrap());

        let p1 = Arc::clone(&pool);
        let p2 = Arc::clone(&pool);

        // Spawn both long closures — they time out after 50ms but threads stay occupied.
        let h1 = tokio::spawn(async move {
            p1.spawn_with_timeout(Duration::from_millis(50), || {
                std::thread::sleep(Duration::from_secs(5));
                0u32
            })
            .await
        });
        let h2 = tokio::spawn(async move {
            p2.spawn_with_timeout(Duration::from_millis(50), || {
                std::thread::sleep(Duration::from_secs(5));
                0u32
            })
            .await
        });

        let r1 = h1.await.unwrap();
        let r2 = h2.await.unwrap();
        assert!(matches!(r1, Err(RayonError::TimedOut(_))));
        assert!(matches!(r2, Err(RayonError::TimedOut(_))));

        // Both threads are still sleeping (for ~5s). Submit another closure.
        // It enqueues and completes once a thread frees. Allow up to 10 seconds.
        let short_result = tokio::time::timeout(Duration::from_secs(10), pool.spawn(|| 42u32))
            .await
            .expect("short closure must complete within 10s (after threads free)");

        assert_eq!(short_result.unwrap(), 42u32);
    }

    #[tokio::test]
    async fn test_pool_size_accessor_unchanged_after_timeout() {
        let pool = RayonPool::new(2, "test").unwrap();
        let _ = pool
            .spawn_with_timeout(Duration::from_millis(10), || {
                std::thread::sleep(Duration::from_millis(200));
                0u32
            })
            .await;
        // Pool capacity is fixed; accessor does not reflect idle-thread count.
        assert_eq!(pool.pool_size(), 2);
    }

    // -----------------------------------------------------------------------
    // §concurrency — Pool Exhaustion and Queue Behaviour
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pool_does_not_deadlock_under_full_occupancy() {
        // 4-thread pool; 4 closures hold a barrier. 5th closure enqueues and completes
        // after the barrier releases the 4 threads.
        let pool = Arc::new(RayonPool::new(4, "test").unwrap());

        // barrier.wait() requires all 5 participants (4 workers + coordinator).
        let barrier = Arc::new(Barrier::new(5));

        let mut handles = Vec::new();
        for _ in 0..4 {
            let p = Arc::clone(&pool);
            let b = Arc::clone(&barrier);
            handles.push(tokio::spawn(async move {
                p.spawn(move || {
                    b.wait(); // blocks until coordinator also calls wait()
                })
                .await
            }));
        }

        // Give the 4 closures time to start and reach the barrier.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Submit 5th closure — it enqueues (all 4 threads are blocked at barrier).
        let p = Arc::clone(&pool);
        let fifth = tokio::spawn(async move { p.spawn(|| 99u32).await });

        // Release all 4 barrier waiters by having the coordinator participate.
        barrier.wait();

        // All 4 closures complete; 5th can now execute.
        for h in handles {
            h.await.unwrap().unwrap();
        }

        let fifth_result = tokio::time::timeout(Duration::from_secs(5), fifth)
            .await
            .expect("fifth closure must complete within 5s")
            .unwrap()
            .unwrap();

        assert_eq!(fifth_result, 99u32);
    }

    #[tokio::test]
    async fn test_two_background_and_two_mcp_concurrent() {
        // 4-thread pool: 2 slow closures (100ms) + 2 fast closures (1ms).
        // All 4 run on available threads; none should be starved.
        let pool = Arc::new(RayonPool::new(4, "test").unwrap());

        let p1 = Arc::clone(&pool);
        let p2 = Arc::clone(&pool);
        let p3 = Arc::clone(&pool);
        let p4 = Arc::clone(&pool);

        let start = std::time::Instant::now();

        let (r1, r2, r3, r4) = tokio::join!(
            tokio::spawn(async move {
                p1.spawn(|| {
                    std::thread::sleep(Duration::from_millis(100));
                    1u32
                })
                .await
            }),
            tokio::spawn(async move {
                p2.spawn(|| {
                    std::thread::sleep(Duration::from_millis(100));
                    2u32
                })
                .await
            }),
            tokio::spawn(async move { p3.spawn(|| 3u32).await }),
            tokio::spawn(async move { p4.spawn(|| 4u32).await }),
        );

        assert_eq!(r1.unwrap().unwrap(), 1u32);
        assert_eq!(r2.unwrap().unwrap(), 2u32);
        assert_eq!(r3.unwrap().unwrap(), 3u32);
        assert_eq!(r4.unwrap().unwrap(), 4u32);

        // All 4 should complete in under 200ms (the two slow ones run in parallel).
        let elapsed = start.elapsed();
        assert!(
            elapsed < Duration::from_millis(500),
            "4-thread pool should complete 2x100ms + 2x1ms in under 500ms, took: {:?}",
            elapsed
        );
    }

    // -----------------------------------------------------------------------
    // §shutdown — Pool Drop Behaviour
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_pool_drop_queued_closures_return_cancelled() {
        // 1-thread pool: occupy the thread with a long sleep, then drop the pool.
        // Any queued closure's tx will be dropped when the pool shuts down.
        let pool = Arc::new(RayonPool::new(1, "test").unwrap());

        // Occupy the single thread.
        let p = Arc::clone(&pool);
        let _occupied =
            tokio::spawn(
                async move { p.spawn(|| std::thread::sleep(Duration::from_secs(5))).await },
            );

        // Give the thread time to start.
        tokio::time::sleep(Duration::from_millis(20)).await;

        // Drop the pool (Arc strong_count → 0 after this block).
        // The single thread is sleeping; this submission queues behind it.
        // When the Arc is dropped, the pool begins shutdown.
        let p2 = Arc::clone(&pool);
        let queued = tokio::spawn(async move { p2.spawn(|| 1u32).await });

        // Drop our strong reference to the pool.
        drop(pool);

        // The queued closure may either complete (if the thread frees before shutdown)
        // or return Cancelled. Either is acceptable — the key invariant is no hang.
        let result = tokio::time::timeout(Duration::from_secs(10), queued)
            .await
            .expect("queued closure must not hang after pool drop");

        // We accept either Ok(1) or Err(Cancelled) — no hang is the invariant.
        let _ = result.unwrap(); // JoinError means tokio panicked — that is a failure.
    }

    // -----------------------------------------------------------------------
    // §error-display — RayonError Display
    // -----------------------------------------------------------------------

    #[test]
    fn test_rayon_error_cancelled_display() {
        let msg = RayonError::Cancelled.to_string();
        assert!(
            msg.contains("cancelled"),
            "Cancelled display must contain 'cancelled', got: {msg}"
        );
    }

    #[test]
    fn test_rayon_error_timed_out_display() {
        let msg = RayonError::TimedOut(Duration::from_secs(30)).to_string();
        assert!(
            msg.contains("30"),
            "TimedOut(30s) display must contain '30', got: {msg}"
        );
    }

    // -----------------------------------------------------------------------
    // §adversarial — Security / Blast Radius
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_adversarial_timeout_does_not_hang_pool() {
        // 4 closures each sleep 5s with 50ms timeout → all return TimedOut promptly.
        // Then a short closure must eventually complete (pool still accepts new work).
        let pool = Arc::new(RayonPool::new(4, "test").unwrap());

        let mut handles = Vec::new();
        for _ in 0..4 {
            let p = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                p.spawn_with_timeout(Duration::from_millis(50), || {
                    std::thread::sleep(Duration::from_secs(5));
                    0u32
                })
                .await
            }));
        }

        // All 4 must time out promptly.
        for h in handles {
            let result = h.await.unwrap();
            assert!(
                matches!(result, Err(RayonError::TimedOut(_))),
                "expected TimedOut, got: {:?}",
                result
            );
        }

        // Short closure must eventually complete. All 4 threads are still sleeping,
        // so this will wait until one frees (~5s). Allow up to 10s.
        let p = Arc::clone(&pool);
        let short_result = tokio::time::timeout(Duration::from_secs(10), p.spawn(|| 1u32))
            .await
            .expect("short closure must complete within 10s");

        assert_eq!(
            short_result.unwrap(),
            1u32,
            "pool must accept new work after adversarial timeouts"
        );
    }
}
