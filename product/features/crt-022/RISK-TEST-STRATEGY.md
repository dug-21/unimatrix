# Risk-Based Test Strategy: crt-022

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Panic in rayon closure propagates past bridge — tokio runtime crashes | High | Low | High |
| R-02 | `spawn_with_timeout` cancels async wait but rayon thread remains occupied; under repeated timeouts, pool exhausts silently | High | Med | Critical |
| R-03 | Hung `session.run()` after timeout leaves mutex poisoned; subsequent callers panic on `.expect("session lock poisoned")`, converting `TimedOut` into `Cancelled` for all future calls on that adapter | High | Low | High |
| R-04 | Call site migrated to `spawn` (no timeout) instead of `spawn_with_timeout` on an MCP handler path, removing timeout coverage | High | Med | Critical |
| R-05 | `AsyncEmbedService` removal breaks a workspace consumer not caught by server-only build (test binary, bench, integration harness) | Med | Low | Med |
| R-06 | A missed eighth `spawn_blocking` ONNX inference call site (not in the SCOPE.md enumeration) remains post-migration; CI grep step fails to catch it | Med | Med | High |
| R-07 | `rayon_pool_size = 0` or out-of-range config reaches pool construction — panic instead of structured startup error | Med | Low | Med |
| R-08 | Pool floor of 4 insufficient when contradiction scan + quality-gate loop run concurrently with 3+ MCP embedding calls; effective queue starvation | Med | Med | High |
| R-09 | A second `RayonPool` instantiated ad-hoc in W1-4 wiring because the `AppState` field is not discovered; doubles thread count without operator awareness | Med | Low | Med |
| R-10 | `OnnxProvider::new` accidentally migrated to rayon; model I/O blocks rayon thread during startup instead of tokio blocking pool | Low | Low | Low |
| R-11 | `rayon = "1"` version drift — a future cargo update resolves to a rayon 2.x with a breaking `ThreadPoolBuilder` API change | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Panic in rayon closure propagates past bridge
**Severity**: High
**Likelihood**: Low
**Impact**: Tokio runtime abort; all MCP sessions terminate; server process exits uncleanly.

**Test Scenarios**:
1. Spawn a closure containing `panic!("deliberate test panic")` via `RayonPool::spawn`; assert the returned `Result` is `Err(RayonError::Cancelled)`; assert the test runtime does not abort (AC-03).
2. Spawn a closure via `RayonPool::spawn_with_timeout` with a long timeout; panic inside; assert `Err(RayonError::Cancelled)` is returned before the timeout fires.
3. Spawn 10 successive closures after a panic on the same pool; assert all 10 complete successfully (pool not damaged by prior panic).

**Coverage Requirement**: Panic containment must be verified for both `spawn` and `spawn_with_timeout`. Pool must remain operational after panic. No `std::panic::catch_unwind` needed — channel drop is sufficient, but the test must confirm this property.

---

### R-02: Rayon threads silently occupied after timeout
**Severity**: High
**Likelihood**: Med
**Impact**: Pool slowly fills with hung threads; effective pool capacity drops; MCP embedding latency spikes; no error surfaced to operator until all threads hang.

**Test Scenarios**:
1. Create a `RayonPool` with 2 threads; dispatch 2 closures that sleep longer than the timeout; call `spawn_with_timeout` a third time and assert it returns `Err(RayonError::TimedOut)` promptly (not a hang).
2. After the above: dispatch a short closure; assert it completes (pool still functional for new work even with prior threads occupied — demonstrates work-stealing queue accepts new submissions).
3. Verify `pool_size()` accessor returns the configured count (not the number of idle threads); no shrinkage of pool after hung threads.

**Coverage Requirement**: Timeout semantics must be verified: the async wait is cancelled, not the rayon thread. Tests must confirm the pool does not deadlock or stop accepting submissions after threads are occupied by hung closures.

---

### R-03: Mutex poisoning after OrtSession hang and timeout
**Severity**: High
**Likelihood**: Low
**Impact**: After one `session.run()` panic, the `Mutex<Session>` is poisoned; every subsequent `embed_entry` call panics on `.expect("session lock poisoned")`; all embedding inference fails until the embed service reloads.

**Test Scenarios**:
1. Mock an `EmbedAdapter`-equivalent that panics inside its inference closure; dispatch via `RayonPool::spawn`; assert `Err(RayonError::Cancelled)` is returned at the bridge level.
2. Confirm the bridge's panic containment does not require the mutex to be healthy — the panic inside the closure (which holds the mutex) poisons it, drops `tx`, and `rx.await` returns `Err(Cancelled)`. The bridge itself is not affected.
3. Document via comment in `rayon_pool.rs` that mutex poisoning in `OnnxProvider` is the embed service's recovery concern, not the bridge's; the bridge contract is: panic → `Cancelled`.

**Coverage Requirement**: The bridge's panic containment path is correct regardless of what internal state (mutex, etc.) the closure holds. Test must confirm the bridge produces `Cancelled` even when the panicking closure holds a lock.

---

### R-04: MCP call site uses `spawn` instead of `spawn_with_timeout`
**Severity**: High
**Likelihood**: Med
**Impact**: MCP handler suspends indefinitely on a hung ONNX session; MCP connection times out at the transport layer without a clean error; lesson #1688 (timeout gaps compound) materialises exactly as documented.

**Test Scenarios**:
1. Code review checklist item: all 7 migrated call sites (search, store_ops, store_correct, status, warmup, quality-gate, contradiction scan for MCP-path variants) use `spawn_with_timeout` not `spawn`. Verify via grep in CI step.
2. Contradiction scan and quality-gate background paths use `spawn` (no timeout) — verify these are NOT on the `spawn_with_timeout` code path. These are fire-and-forget background tasks; applying `MCP_HANDLER_TIMEOUT` to them would incorrectly kill long-running scans.
3. `RayonPool` module-level rustdoc must document the convention: MCP paths → `spawn_with_timeout`, background paths → `spawn`. Verify doc comment exists.

**Coverage Requirement**: Static coverage via CI grep confirming the 7 MCP call sites use `spawn_with_timeout`. ADR-002 convention must be documented in module rustdoc. Background paths must not have spurious timeouts.

---

### R-05: `AsyncEmbedService` removal breaks workspace consumer
**Severity**: Med
**Likelihood**: Low
**Impact**: `cargo check --workspace` fails on a test binary or bench after the struct is deleted; build breaks CI.

**Test Scenarios**:
1. After removal, run `cargo check --workspace` and assert exit code 0 (AC-05, NFR-07).
2. Run `grep -r "AsyncEmbedService" crates/` and assert zero results (AC-05).
3. Confirm `AsyncVectorStore` is present and unchanged: `grep -r "AsyncVectorStore" crates/unimatrix-core/` returns the struct definition.

**Coverage Requirement**: Workspace-level build must pass with zero errors. Both positive (AsyncVectorStore present) and negative (AsyncEmbedService absent) assertions required.

---

### R-06: Missed ONNX spawn_blocking call site post-migration
**Severity**: Med
**Likelihood**: Med
**Impact**: The site retains its old behaviour; tokio blocking pool still consumed by ONNX inference at that site; AC-07 is silently violated; lesson from entry #1688 (coverage gaps compound) repeats.

**Test Scenarios**:
1. CI grep step scans `crates/unimatrix-server/src/services/` and `crates/unimatrix-server/src/background.rs` for `spawn_blocking`; asserts zero results that are not in the permitted non-inference allow-list. Step must fail the build if any inference site remains.
2. Grep `crates/unimatrix-core/src/async_wrappers.rs` for `spawn_blocking`; assert zero results post-removal of `AsyncEmbedService` (the only `spawn_blocking` user in that file was `AsyncEmbedService`).
3. Grep entire workspace for `spawn_blocking_with_timeout` in `services/`; assert zero results (the timeout wrapper is fully replaced by `spawn_with_timeout`).

**Coverage Requirement**: CI step is the enforcement mechanism (C-09). The step must run on every PR against main and must be able to distinguish inference sites from permitted non-inference sites.

---

### R-07: Invalid `rayon_pool_size` reaches pool construction
**Severity**: Med
**Likelihood**: Low
**Impact**: Value of 0 causes `ThreadPoolBuilder` to build a zero-thread pool (undefined behaviour or immediate deadlock); value of 65 creates an oversized pool consuming excessive CPU. Without validation, the operator has no diagnostic message.

**Test Scenarios**:
1. `InferenceConfig::validate()` with `rayon_pool_size = 0` → structured `ConfigError`; startup aborts (AC-09).
2. `InferenceConfig::validate()` with `rayon_pool_size = 65` → structured `ConfigError`; startup aborts (AC-09).
3. `InferenceConfig::validate()` with `rayon_pool_size = 1` → passes (lower bound inclusive).
4. `InferenceConfig::validate()` with `rayon_pool_size = 64` → passes (upper bound inclusive).
5. Absent `[inference]` section in config → default applied; `rayon_pool_size` equals `(num_cpus / 2).max(2).min(8)` (AC-09 default, FR-06).
6. Server integration test: start server with `rayon_pool_size = 0` in config; assert server exits with non-zero status and a structured error message naming the offending field.

**Coverage Requirement**: All boundary values (0, 1, 64, 65) tested. Absent section tested. Startup abort with structured error tested end-to-end.

---

### R-08: Pool exhaustion under concurrent background + MCP load
**Severity**: Med
**Likelihood**: Med
**Impact**: MCP embedding calls queue behind contradiction scan and quality-gate loop; visible latency spike; operators with default pool size 4 on large knowledge bases observe degraded search performance during background ticks.

**Test Scenarios**:
1. Unit test: create pool with 4 threads; dispatch 2 long-running closures (simulating scan + quality-gate); concurrently dispatch 2 short closures (simulating MCP calls); assert both short closures complete within 2× their uncontested duration (not indefinitely queued).
2. Verify pool is not deadlocked when all 4 threads are occupied: the 5th submission enqueues without panic and completes when a thread becomes free.
3. Document: pool size 4 is the default floor; operators should increase `rayon_pool_size` on deployments with large knowledge bases (>1000 entries). This is a doc/comment coverage requirement, not a failing test.

**Coverage Requirement**: Pool does not deadlock under full occupancy. New submissions queue correctly. Short MCP closures are not starved indefinitely — rayon work-stealing ensures fair scheduling once a thread is available.

---

### R-09: Second `RayonPool` instantiated in future wiring
**Severity**: Med
**Likelihood**: Low
**Impact**: Thread count doubles without operator awareness; config `rayon_pool_size` no longer reflects actual thread usage; W1-4 inference pool budget is violated.

**Test Scenarios**:
1. Structural assertion: `RayonPool::new` must only be called once in the startup wiring path. Verify via code review that `RayonPool::new` appears exactly once in `main.rs` (or startup entry point).
2. `AppState` / `ServiceLayer` struct has exactly one `ml_inference_pool` field of type `Arc<RayonPool>`. Verify via `grep "ml_inference_pool" crates/unimatrix-server/src/` — one definition site, N reference sites.
3. `Arc::strong_count` check in an integration test: after server startup, assert the pool's strong count equals the number of expected consumers (no stray second instantiation).

**Coverage Requirement**: Single instantiation enforced structurally via `AppState`. Code review confirms one `RayonPool::new` call site. ADR-004 convention documented.

---

### R-10: `OnnxProvider::new` accidentally migrated to rayon
**Severity**: Low
**Likelihood**: Low
**Impact**: Model loading (file I/O + ONNX session init) runs on a rayon thread during startup; rayon thread is occupied for the entire model load duration; this is the wrong pool for I/O-bound work but is functionally harmless in most cases.

**Test Scenarios**:
1. Grep: `grep -n "spawn_blocking" crates/unimatrix-server/src/infra/embed_handle.rs` must return exactly one result — the `OnnxProvider::new` call — and no rayon-related call (AC-08).

**Coverage Requirement**: Single grep assertion sufficient.

---

### R-11: Rayon version drift breaks `ThreadPoolBuilder` API
**Severity**: Low
**Likelihood**: Low
**Impact**: A future `cargo update` resolves `rayon = "1"` to a rayon 2.x (if Cargo.toml is not pinned); `ThreadPoolBuilder` API change breaks compilation.

**Test Scenarios**:
1. `unimatrix-server/Cargo.toml` specifies `rayon = "1"` (semver-pinned to major version 1). Cargo will not resolve to 2.x without an explicit version bump. Verify the exact specifier in `Cargo.toml` is `"1"` not `"*"` or `">= 1"`.

**Coverage Requirement**: `Cargo.toml` inspection sufficient. No runtime test needed.

---

## Integration Risks

### Bridge-to-ServiceError mapping
Each of the 7 call sites maps `RayonError` → `ServiceError::EmbeddingFailed`. The mapping is two-layer (outer: bridge error, inner: `CoreError`). If any call site omits the outer `?` or applies the layers in the wrong order, `TimedOut(Duration)` or `Cancelled` is silently swallowed or surfaces as the wrong error type to the MCP handler.

**Scenario**: At each of the 7 migrated sites, verify the double `.map_err` pattern matches the migration pattern in `ARCHITECTURE.md §Call-Site Migration Pattern`. A `RayonError::TimedOut` must not be downcasted to `Ok(result)`.

### Background task coordinator receives `RayonError`
The contradiction scan and quality-gate loop are dispatched via `spawn` (no timeout) and their `RayonError::Cancelled` must be handled by the background task coordinator. If `Cancelled` is silently ignored (`.ok()` or `let _ =`), a panicked ONNX closure produces no log, no metric, and no recovery trigger.

**Scenario**: Verify error handling at background task dispatch sites: `Cancelled` from the contradiction scan must at minimum emit a tracing `error!` log event; it must not silently discard the failure.

### `spawn_with_timeout` timeout duration propagation
`MCP_HANDLER_TIMEOUT` is the canonical constant. If any migrated call site hard-codes a different duration (e.g., `Duration::from_secs(5)` copied from another context), that site diverges from the intended behaviour.

**Scenario**: Grep all 7 MCP call sites for `spawn_with_timeout`; confirm each passes `MCP_HANDLER_TIMEOUT` by name, not a literal duration.

---

## Edge Cases

- **Single-core container**: `num_cpus::get() = 1`; default formula yields `max(0, 4) = 4`. Pool creates 4 threads on a single CPU. Assert this case: no panic, pool starts, inference functions (CPU contention is a performance concern, not a correctness one).
- **`rayon_pool_size = 1`**: Valid config. A single-thread pool serialises all inference. Contradiction scan occupies the sole thread; all MCP calls queue. Assert: pool starts; closures execute serially; no deadlock.
- **Zero-length input to `embed_entry`**: Not changed by this migration; the rayon bridge does not alter `EmbedAdapter` behaviour. However, if the ONNX session panics on empty input, the bridge must convert it to `Cancelled`. Verify panic containment covers this case.
- **Pool shutdown while closure is queued**: `RayonPool` dropped (server shutdown) while closures are queued. Queued closures may not execute; `rx.await` returns `Err(RecvError)` → `Cancelled`. Assert: no panic during pool drop; callers receive `Cancelled`, not a hang.
- **Multiple concurrent `spawn_with_timeout` calls all timing out simultaneously**: Assert pool remains healthy after N simultaneous timeouts — threads still occupied, but new `spawn` calls succeed (queue accepts new work).

---

## Security Risks

### Untrusted input via MCP `context_search` / `context_store`
MCP tool inputs (`query`, `content`, `title`) flow through to `embed_entry` inside the rayon closure. The ONNX model processes this content. A maliciously crafted long string could cause extreme inference time, occupying a rayon thread for the timeout window (`MCP_HANDLER_TIMEOUT = 30s`). With 4 threads and 4 concurrent adversarial requests, all threads are held for 30 seconds apiece; the pool is temporarily exhausted.

**Blast radius**: Degraded embedding throughput for the timeout window. MCP requests queue. Server remains functional after thread recovery. Impact is bounded by `spawn_with_timeout` — the MCP handler is released after 30s; only the rayon thread is consumed.

**Mitigation already in design**: `spawn_with_timeout` (ADR-002) contains the MCP-path blast radius. Pool sizing at 4+ (ADR-003) ensures single adversarial session does not fully exhaust the pool.

**Test scenario**: Send 5 concurrent requests each designed to consume maximum inference time; assert server continues to accept and respond to a 6th short request after `MCP_HANDLER_TIMEOUT` elapses on the first 5 (pool recovery after timeout).

### Panic attack via malformed ONNX input
A sufficiently malformed input that triggers a panic in `session.run()` converts to `RayonError::Cancelled` via the bridge. However, if `Mutex<Session>` is poisoned by the panic, subsequent calls fail with `Cancelled` permanently until the embed service reloads. A sequence of malformed inputs could poison the session and deny embedding service to all future callers.

**Blast radius**: Embedding inference unavailable until `EmbedServiceHandle` detects failure and retries. All MCP tools depending on embedding (search, store, correct, status) return `EmbeddingFailed`.

**Mitigation gap**: The architecture does not address mutex poisoning recovery at the bridge level. The `EmbedServiceHandle` retry state machine (`Loading → Ready | Failed → Retrying`) is the recovery path, but it triggers on `get_adapter()` failures, not on `RayonError::Cancelled` at call sites. A poison-triggered sequence of `Cancelled` responses may not feed back to the state machine.

**Test scenario**: Simulate mutex poison by manually poisoning the lock; verify that a subsequent `embed_entry` call panics, the bridge returns `Cancelled`, and the embed service eventually retries and recovers.

### CI grep step bypass
The AC-07 CI grep step could be bypassed by a `spawn_blocking` call hidden in a macro expansion or a re-export. Malicious or careless code that adds a `spawn_blocking` ONNX call inside a declarative macro would not be caught by a plain text grep.

**Blast radius**: Tokio blocking pool saturation at that undiscovered site; #1688-class incident for that path.

**Mitigation**: The grep step is a defence-in-depth tool, not a complete proof. Code review of the implementation PR is the primary control for macro-hidden calls.

---

## Failure Modes

| Failure | Expected Behaviour |
|---------|-------------------|
| `RayonPool::new` fails at startup | Structured `ServerStartupError::InferencePoolInit(...)` with `rayon_pool_size` in message; process exits with non-zero status; no silent fallback to `spawn_blocking` |
| Closure panics in rayon worker | `RayonError::Cancelled` returned to caller; pool remains operational; panicking thread is recycled by rayon |
| `spawn_with_timeout` timeout fires | `RayonError::TimedOut(duration)` returned; MCP handler maps to `ServiceError::EmbeddingFailed`; rayon thread continues running until ORT session unblocks naturally |
| Mutex poisoned in `OnnxProvider` | All subsequent `embed_entry` calls panic inside rayon closure; all return `Cancelled`; `EmbedServiceHandle` retry state machine is the recovery path |
| Pool exhausted (all threads occupied) | New `spawn` / `spawn_with_timeout` calls enqueue in rayon's work-stealing queue; no panic; calls block until a thread is free; `spawn_with_timeout` fires if wait + execution exceeds timeout |
| `AsyncEmbedService` call from external code | Compile error — struct does not exist; `cargo check --workspace` catches this at build time |
| Background task `Cancelled` | `error!` tracing event emitted; background tick logs failure and continues; no crash; next tick reattempts the scan |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — OrtSession thread safety under rayon | R-01, R-03 | Resolved in `ARCHITECTURE.md §thread-safety`: `OnnxProvider` wraps `Mutex<Session>`; `EmbedAdapter` is `Send + 'static`; `test_send_sync` asserts this at compile time. Mutex poisoning on panic is R-03, mitigated by bridge panic containment but with a gap in embed service recovery path. |
| SR-02 — `ThreadPoolBuildError` on startup | R-07 | Resolved via `FR-04` / `NFR-03`: startup error propagated as structured `ServerStartupError`; no panic, no silent fallback. Tested by R-07 scenarios. |
| SR-03 — Timeout coverage gap after migration | R-04 | Resolved by ADR-002 (`spawn_with_timeout` on `RayonPool`); the 7 MCP call sites use `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`. R-04 tests that the convention is enforced. |
| SR-04 — Contradiction scan monopolisation | R-08 | Resolved by ADR-003: scan stays single task; pool floor raised to 4. R-08 tests queue behaviour under full-occupancy. |
| SR-05 — AC-07 grep audit inadequacy | R-06 | Resolved by C-09: CI grep step replaces post-ship audit. R-06 tests the CI step itself. Security Risks section notes macro-expansion gap. |
| SR-06 — Ad-hoc pool re-instantiation in W1-4 | R-09 | Resolved by ADR-004: `AppState` owns `Arc<RayonPool>`; single construction site in `main.rs`. R-09 structural tests enforce this. |
| SR-07 — `AsyncEmbedService` external consumer | R-05 | Resolved by `cargo check --workspace` as explicit AC (AC-05, NFR-07). R-05 tests the workspace build. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-02, R-04) | 5 scenarios |
| High | 4 (R-01, R-03, R-06, R-08) | 11 scenarios |
| Med | 4 (R-05, R-07, R-09, security) | 13 scenarios |
| Low | 2 (R-10, R-11) | 2 scenarios |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection rayon tokio thread pool" — found #1688 (spawn_blocking timeout gaps compound), #735 (blocking pool saturation). Both elevated R-04 (timeout convention gap) to Critical priority.
- Queried: `/uni-knowledge-search` for "outcome rework spawn_blocking migration" — found #1700 (spawn_blocking_with_timeout outer error type), #1627 (MCP instability from crt-014 store scan). Informed R-03 (mutex poisoning path) and R-08 (scan monopolisation).
- Queried: `/uni-knowledge-search` for "risk pattern rayon pool panic timeout" — found #2491 (Rayon-Tokio bridge pattern), #2535 (rayon pool monopolisation by background scans), #2537 (ADR-002 timeout decision). Both crt-022-specific patterns already stored by researcher/architect; no novel pattern to store from this risk pass.
- Stored: nothing novel to store — all identified patterns (#2491, #2535) are crt-022-specific and were already stored by prior agents in this feature's design phase.
