## ADR-001: NLI Session Concurrency — Single Mutex<Session> with Dynamic Pool Floor

### Context

`NliProvider` runs ONNX inference on (query, passage) pairs. The ORT API requires `&mut self` on `Session::run()`, making serialization mandatory. ADR-001 from nxs-003 (Unimatrix entry #67) established `Mutex<Session>` as the correct pattern for `OnnxProvider`; the question here is whether the same pattern scales to NLI's workload profile.

The NLI workload differs from embedding in two critical ways:

1. **Batch size is larger**: search re-ranking scores up to `nli_top_k = 20` pairs per MCP call. Post-store detection scores up to `nli_post_store_k = 10` pairs per `context_store` call. These run sequentially through one session under a single mutex acquisition.

2. **Per-pair latency is higher**: `cross-encoder/nli-MiniLM2-L6-H768` takes 50–200ms per pair on CPU (vs ~10–30ms per embedding). At 20 pairs × 200ms worst case, one search re-rank call holds the mutex for ~4 seconds.

**Quantified hold-time analysis:**

| Scenario | Pairs | Per-pair latency | Mutex hold time |
|----------|-------|-----------------|-----------------|
| Search re-rank (best case) | 20 | 50ms | ~1.0s |
| Search re-rank (worst case) | 20 | 200ms | ~4.0s |
| Post-store detection (best case) | 10 | 50ms | ~0.5s |
| Post-store detection (worst case) | 10 | 200ms | ~2.0s |

**Pool saturation analysis** (shared pool, default formula `(num_cpus/2).max(4).min(8)` = 4–8 threads):

At 4-thread floor with concurrent MCP calls:
- Thread 1: search re-ranking NLI batch (holds mutex up to 4s)
- Thread 2: embedding inference for a concurrent search
- Threads 3–4: available for post-store NLI and other background work

With 2 concurrent MCP search calls and 1 concurrent context_store NLI task, 3 threads are potentially occupied with inference. A 4-thread pool has 1 thread free for embedding from new requests. The embedding call is short (~10–30ms), so it queues briefly before getting a thread.

The critical risk is a **4s NLI mutex hold stacking with an embedding request queue**: if 3 search calls arrive simultaneously, all running NLI on their 20-pair batches, they serialize through the single `Mutex<Session>`. The 4th call's NLI step waits up to 12s (3 × 4s) before the mutex is free. At `MCP_HANDLER_TIMEOUT` (not specified exactly in scope, assumed 30s), this remains within bounds.

**Session pool alternative analysis:**

A session pool of size `N` (e.g., N = pool_threads / 2 = 2) would allow 2 concurrent NLI batches to execute simultaneously. However:
- Each NLI model instance is ~85MB (MiniLM2). A pool of 2 = ~170MB vs single session ~85MB.
- Pool management (acquire/release) adds complexity.
- The actual bottleneck at Unimatrix's load profile (single-agent, sequential MCP calls from Claude) is the rayon thread count, not the mutex. Concurrent MCP calls are rare in practice.
- Work-stealing: rayon's work-stealing queue means post-store fire-and-forget tasks interleave with search re-ranking tasks at the thread level, not the session level.

**Pool floor decision:**

The existing floor (4, from crt-022 ADR-003, entry #2574) was set for the embedding + contradiction scan workload. NLI adds multi-pair batched inference in two locations:
- MCP path: `spawn_with_timeout` (bounded)
- Background: `spawn` (unbounded, fire-and-forget)

At `nli_top_k = 20` and the existing 4-thread floor, the worst-case scenario is 2 concurrent MCP search calls each running 20-pair NLI batches on 2 rayon threads, while the other 2 threads handle embedding for new requests. This is manageable. However, the `max_contradicts_per_tick = 10` cap on post-store tasks means background NLI tasks are bounded in scope per call.

The pool floor should be raised to 6 when NLI is enabled. Rationale: NLI adds a new sustained workload category (batched pairs) distinct from the short embedding calls. At 4 threads, a burst of 3 NLI re-ranking calls consuming 3 threads leaves only 1 thread for all embedding — starvation risk for new searches. At 6 threads, 3 concurrent NLI batches leave 3 threads for embedding, maintaining MCP responsiveness. The increase is conditional on `nli_enabled = true` at startup.

### Decision

**1. Single `Mutex<Session>` for `NliProvider`** — consistent with ADR-001 (entry #67) for `OnnxProvider`. The mutex serializes all NLI inference through one session instance. This bounds memory at ~85MB (MiniLM2) or ~180MB (deberta).

Rationale for rejecting session pool: Unimatrix's MCP workload is predominantly sequential (one Claude agent driving one session). True parallel MCP search calls are rare. The complexity and memory cost of a session pool is not justified by the actual concurrency profile.

**2. Pool floor raised to 6 when `nli_enabled = true`** — `InferenceConfig` gains a `nli_rayon_pool_floor: usize` field (default 6). At server startup, if `nli_enabled = true` and `InferenceConfig::rayon_pool_size` resolves below 6, it is raised to 6. If the operator explicitly sets `rayon_pool_size >= 6`, that value is used as-is.

The existing crt-022 ADR-003 formula `(num_cpus/2).max(4).min(8)` is preserved for non-NLI deployments. When NLI is enabled, the floor override applies only to the minimum: `pool_size = rayon_pool_size.max(6).min(8)`.

This is additive to the existing ADR-003 rule, not a replacement. No new ADR is needed for crt-022 because the change is a conditional floor adjustment, not a redesign.

**3. Post-store fire-and-forget uses `rayon_pool.spawn()` (no timeout)** — consistent with background task convention (rayon_pool.rs ADR-002 usage comment). The `max_contradicts_per_tick` cap limits the scope of each fire-and-forget task (at most 10 pairs), bounding its pool occupancy to ~2s worst case per task. This is acceptable for a background task that does not block MCP responses.

**4. `NliServiceHandle` detects mutex poisoning on `get_provider()`** — when the `Mutex<Session>` is poisoned by a panic in a rayon worker, the next `get_provider()` call must detect the poisoned state (via `try_lock()` returning `PoisonError`), transition to `Failed`, and initiate retry. This mirrors `EmbedServiceHandle`'s retry pattern.

```rust
// NliProvider session access pattern:
pub fn score_batch(&self, pairs: &[(&str, &str)]) -> Result<Vec<NliScores>> {
    // Tokenize all pairs outside the lock (lock-free)
    let encodings = self.tokenize_pairs(pairs)?;
    // Acquire lock once for the full batch
    let session = self.session.lock()
        .map_err(|_| NliError::SessionPoisoned)?;
    // Run all pairs sequentially through the session
    let logits = session.run_batch(&encodings)?;
    drop(session); // explicit early release
    // Softmax outside the lock
    Ok(logits.into_iter().map(softmax_3class).collect())
}
```

The full batch runs under one lock acquisition to minimize lock/unlock overhead across 20 pairs.

### Consequences

**Easier:**
- Single session = predictable ~85MB memory footprint; no per-pool-slot model overhead.
- Implementation mirrors `OnnxProvider` exactly (same `Mutex<Session>` + `Tokenizer` pattern).
- Mutex poison detection reuses the `EmbedServiceHandle` retry machinery.
- Pool floor raise is a 2-line config override at startup; no structural change to `RayonPool`.

**Harder:**
- Under sustained concurrent load (≥3 simultaneous MCP search calls all with NLI enabled), NLI batches serialize through one session. At 20 pairs × 200ms worst case, queue depth of 3 adds ~8s to the third caller's NLI step. This remains within MCP_HANDLER_TIMEOUT but degrades p99 latency.
- Mitigation: `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` caps the wait. On timeout, the search falls back to `rerank_score` — the user receives correct (if less accurate) results rather than an error.
- The pool floor raise to 6 adds 2 extra OS threads on NLI-enabled deployments. On CPU-constrained hosts (e.g., 2-core), `(2/2).max(4).min(8) = 4` → raised to 6, which may be suboptimal. Operators can override with explicit `rayon_pool_size` config.

**Related ADRs:** nxs-003 ADR-001 (entry #67, Mutex<Session> for OnnxProvider), crt-022 ADR-003 (entry #2574, pool floor = 4).
