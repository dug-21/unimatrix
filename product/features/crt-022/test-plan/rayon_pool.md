# RayonPool + RayonError — Unit Test Plan

**Component**: `crates/unimatrix-server/src/infra/rayon_pool.rs` (new file)
**Risks addressed**: R-01, R-02, R-03, R-07 (pool construction side), R-08
**AC addressed**: AC-02, AC-03, AC-11 (tests 1–4)

All tests use `#[tokio::test]` because `RayonPool::spawn` and `spawn_with_timeout` are async.
Place in a `#[cfg(test)] mod tests` block at the bottom of `rayon_pool.rs`.

---

## §pool-init — Pool Initialisation (AC-11 tests 3 & 4)

### test_pool_init_single_thread (AC-11 #3)
```
// Arrange: RayonPool::new(1, "test_pool")
// Act: assert pool construction is Ok(_)
// Assert: pool.pool_size() == 1
// Assert: pool.name() == "test_pool"
```
Verifies minimum valid pool size (rayon allows 1). No deadlock expected.

### test_pool_init_eight_threads (AC-11 #4)
```
// Arrange: RayonPool::new(8, "test_pool")
// Act: assert construction is Ok(_)
// Assert: pool.pool_size() == 8
```

### test_pool_init_default_formula
```
// Arrange: compute expected = (num_cpus::get() / 2).max(4).min(8)
// Act: InferenceConfig::default() → rayon_pool_size
// Assert: rayon_pool_size == expected
```
Locks in the ADR-003 formula. If the formula regresses, this test catches it.

### test_pool_init_name_retained
```
// Arrange: RayonPool::new(4, "ml_inference_pool")
// Assert: pool.name() == "ml_inference_pool"
```
Pool name is observable for logging/diagnostics (ADR-003 convention).

---

## §spawn-dispatch — Successful Dispatch (AC-11 test 1, AC-02)

### test_spawn_returns_closure_value (AC-11 #1)
```
// Arrange: pool = RayonPool::new(2, "test").unwrap()
// Act: result = pool.spawn(|| 42u64).await
// Assert: result == Ok(42u64)
```
Basic round-trip: closure executes on rayon thread, value returned via oneshot channel.

### test_spawn_with_timeout_returns_closure_value
```
// Arrange: pool = RayonPool::new(2, "test").unwrap()
// Act: result = pool.spawn_with_timeout(Duration::from_secs(5), || 99u64).await
// Assert: result == Ok(99u64)
```
The timeout-bearing variant must also return the value when the closure completes within budget.

### test_spawn_sends_complex_type
```
// Arrange: closure returns a Vec<u8> of known content
// Assert: returned value matches the original Vec
```
Verifies `T: Send + 'static` bound is satisfied for heap-allocated types.

---

## §panic-containment — Panic Safety (AC-11 test 2, R-01, AC-03)

### test_spawn_panic_returns_cancelled (AC-11 #2, AC-03)
```
// Arrange: pool = RayonPool::new(2, "test").unwrap()
// Act: result = pool.spawn(|| panic!("deliberate test panic")).await
// Assert: result == Err(RayonError::Cancelled)
// Assert: test does not abort (the #[tokio::test] harness continues)
```
This is the primary AC-03 test. The panic must be converted to `RayonError::Cancelled`
via the oneshot channel drop; the tokio runtime must not see the panic.

### test_spawn_with_timeout_panic_returns_cancelled_not_timeout
```
// Arrange: pool = RayonPool::new(2, "test").unwrap()
// Act: result = pool.spawn_with_timeout(Duration::from_secs(5), || panic!("test")).await
// Assert: result == Err(RayonError::Cancelled)
// Assert: result is NOT Err(RayonError::TimedOut(_))
```
Panic must produce `Cancelled` regardless of whether a timeout is active. The channel
drop fires before the timeout fires when the panic is immediate.

### test_pool_functional_after_panic (R-01 scenario 3)
```
// Arrange: pool = RayonPool::new(2, "test").unwrap()
// Act step 1: pool.spawn(|| panic!("p1")).await — discard result
// Act step 2: pool.spawn(|| panic!("p2")).await — discard result
// Act step 3–12: 10x pool.spawn(|| 1u32).await
// Assert: all 10 results are Ok(1u32)
```
The pool must remain operational after panicking closures. Rayon recycles the panicking
thread; the pool is not damaged.

### test_spawn_panic_with_mutex_held
```
// Arrange: mutex = Arc::new(Mutex::new(0u32))
//          clone mutex into closure
// Act: pool.spawn(move || { let _guard = mutex.lock(); panic!("poison") }).await
// Assert: result == Err(RayonError::Cancelled)
// Assert: outer mutex attempt to lock returns Err (mutex poisoned by rayon thread)
```
Verifies R-03: bridge panic containment works regardless of what the closure holds.
The mutex poisoning is the closure's concern; the bridge contract (panic → Cancelled) holds.

---

## §timeout-semantics — Timeout Behaviour (R-02, Critical)

### test_spawn_with_timeout_fires_when_closure_exceeds_timeout
```
// Arrange: pool = RayonPool::new(2, "test").unwrap()
// Act: result = pool.spawn_with_timeout(
//     Duration::from_millis(50),
//     || { std::thread::sleep(Duration::from_secs(10)); 0u64 }
// ).await
// Assert: result == Err(RayonError::TimedOut(Duration::from_millis(50)))
// Assert: this completes within ~1 second (not 10 seconds)
```
Verifies the timeout cancels the async wait, not the rayon thread. The thread continues
sleeping; the `rx.await` times out.

### test_spawn_timeout_duration_preserved
```
// Act: result = pool.spawn_with_timeout(Duration::from_millis(100), || {
//     std::thread::sleep(Duration::from_secs(5)); 0u32
// }).await
// Assert: matches!(result, Err(RayonError::TimedOut(d)) if d == Duration::from_millis(100))
```
The `TimedOut` variant carries the actual timeout duration that was configured.

### test_pool_accepts_new_submissions_after_timeout (R-02 scenario 2)
```
// Arrange: pool = RayonPool::new(2, "test").unwrap()
// Step 1: spawn 2 long-running closures (sleep 5s each) with 50ms timeout
//         → both return TimedOut promptly
// Step 2: pool.spawn(|| 42u32).await with 5s timeout
// Assert: step 2 result is Ok(42u32) within reasonable time
//         (demonstrates pool still accepts new work even with occupied threads)
```
R-02 core scenario: pool does not deadlock or stop accepting submissions after threads
are occupied by hung closures. Note: rayon threads are still sleeping; new work enqueues
and a new thread is NOT created (pool is fixed-size). Rayon's work-stealing queue accepts
the submission; it will not execute until a thread frees. For a 2-thread pool both
occupied for 5s, the 3rd submission completes after ~5s. The test must not assert
fast completion — it asserts no hang/panic/deadlock, and eventual Ok(42).

### test_pool_size_accessor_unchanged_after_timeout
```
// After the above scenario, assert: pool.pool_size() == 2
```
Pool capacity is fixed; the accessor does not reflect idle-thread count.

---

## §concurrency — Pool Exhaustion and Queue Behaviour (R-08)

### test_pool_does_not_deadlock_under_full_occupancy (R-08)
```
// Arrange: pool = RayonPool::new(4, "test").unwrap()
//          barrier = Arc::new(Barrier::new(5)) — 4 workers + 1 coordinator
// Act:
//   – Spawn 4 closures: each waits at barrier, completing fills the barrier,
//     then returns immediately.
//   – Spawn 1 more closure after all 4 are submitted: returns 99u32.
//   – Release barrier.
// Assert: 5th closure result is Ok(99u32) — it enqueued and completed.
// Assert: no panic, no hang (test timeout = 5 seconds).
```
Directly validates R-08: the 5th submission enqueues without panic and completes when
a thread becomes free.

### test_two_background_and_two_mcp_concurrent (R-08 scenario 1)
```
// Arrange: pool = RayonPool::new(4, "test").unwrap()
//          simulate: 2 slow closures (100ms each) + 2 fast closures (1ms each)
//          spawn all 4 concurrently via tokio::join!
// Assert: all 4 complete in under 200ms total
//         (2 run immediately on 2 threads; others queue behind them or run on remaining 2)
// Assert: fast closures are not starved (complete within 2x uncontested time)
```
Validates that rayon's work-stealing scheduler allows progress for MCP calls even
when background tasks are running concurrently.

---

## §shutdown — Pool Drop Behaviour (Edge Case)

### test_pool_drop_queued_closures_return_cancelled
```
// Arrange: pool = Arc::new(RayonPool::new(1, "test").unwrap())
//          1 thread occupied by a long sleep
// Act: while thread is occupied, drop the pool (Arc strong_count → 0)
//      attempt another spawn — should return Cancelled (channel dropped)
// Assert: the submission after drop returns Err(RayonError::Cancelled) (or panics with
//         a useful message — either is acceptable; the test confirms no hang)
```
Edge case from RISK-TEST-STRATEGY.md §Edge Cases: pool shutdown while closure queued.

---

## §error-display — RayonError Display

### test_rayon_error_cancelled_display
```
// Assert: RayonError::Cancelled.to_string() contains "cancelled"
```

### test_rayon_error_timed_out_display
```
// Assert: RayonError::TimedOut(Duration::from_secs(30)).to_string() contains "30"
//         (the duration is rendered in the error message)
```

---

## §adversarial — Security / Blast Radius (R-Security)

### test_adversarial_timeout_does_not_hang_pool
```
// Arrange: pool = RayonPool::new(4, "test").unwrap()
// Act: spawn 4 closures via spawn_with_timeout(50ms each) that sleep for 5 seconds
//      → all 4 return TimedOut after ~50ms
// Immediately after: spawn 1 short closure via spawn(|| 1u32)
// Assert: short closure eventually returns Ok(1u32) (may wait for one of the 4 threads)
// Assert: no panic during any of the above
```
Bounded blast radius: adversarial input exhausts pool temporarily; server remains
functional after timeout window. Pool accepts new work once a thread frees.

---

## Test Module Placement

```rust
// In crates/unimatrix-server/src/infra/rayon_pool.rs

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier, Mutex};
    use std::time::Duration;

    // All tests use #[tokio::test]
    // Use tokio::time::timeout as the outer deadline for tests
    // that involve real time (set a 10s outer timeout on any test
    // using real sleeps to prevent CI hang)
}
```
