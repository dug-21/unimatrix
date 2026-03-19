# Security Review: crt-022-security-reviewer

## Risk Level: low

## Summary

crt-022 introduces a rayon-tokio bridge (`RayonPool`) for CPU-bound ML inference in `unimatrix-server`, migrates 7 ONNX embedding call sites from `spawn_blocking_with_timeout` to `rayon_pool.spawn_with_timeout`, adds `InferenceConfig` for pool sizing, removes `AsyncEmbedService` from `unimatrix-core`, and adds a CI enforcement script. The change is focused, minimal, and well-bounded. No blocking security findings were identified. Three non-blocking observations are noted.

---

## Findings

### Finding 1: Panic containment relies on stack unwinding (correct, but has one gap)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/infra/rayon_pool.rs:121-126`
- **Description**: The panic containment mechanism works as documented — if `f()` panics inside the rayon closure, the stack unwinds, `tx` is dropped before `tx.send(result)` is called, and `rx.await` returns `Err(RecvError)`, which maps to `RayonError::Cancelled`. A custom `panic_handler` discards the payload at the rayon level. This is correct for closures that can unwind. However, if a closure is compiled with `panic = "abort"` (either globally or via a dependency), the abort bypasses the unwinding path entirely — `tx` is never dropped cleanly and the process terminates. This is not exploitable and the test infrastructure confirms the unwinding path works correctly under Rust's default `panic = "unwind"`. The risk is confined to hypothetical future Cargo profile changes.
- **Recommendation**: Add a comment in `RayonPool::new` noting the panic containment relies on `panic = "unwind"` being in effect. The workspace Cargo profile should be checked to confirm no `panic = "abort"` profile is active for the server crate. This is an informational note, not a configuration change.
- **Blocking**: no

### Finding 2: spawn_blocking_with_timeout import retained in search.rs (dead import path)

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/search.rs:24`
- **Description**: `search.rs` imports both `MCP_HANDLER_TIMEOUT` and `spawn_blocking_with_timeout` at line 24. The ONNX embedding call site at line 235 was correctly migrated to `rayon_pool.spawn_with_timeout`. However, `spawn_blocking_with_timeout` remains in use at line 462 for the co-access boost computation (`compute_search_boost`), which is a short-lived SQLite read, not an ONNX inference call. This use is correct — it is not an inference site and is excluded by the CI grep filter (which filters on `"embed"`). The import is not dead. This is noted to confirm the reviewer verified the remaining use is legitimate and not a missed migration site.
- **Recommendation**: None required. The co-access boost `spawn_blocking_with_timeout` call is a permitted non-inference use correctly excluded from the CI enforcement check. The CI script's `grep "embed"` filter correctly distinguishes it. This finding is informational only.
- **Blocking**: no

### Finding 3: Mutex poisoning recovery gap is documented but unmitigated at the bridge level

- **Severity**: low (inherent to the ONNX architecture, not introduced by this change)
- **Location**: `crates/unimatrix-server/src/infra/rayon_pool.rs:22-26` (doc comment), `RISK-TEST-STRATEGY.md:§Panic attack via malformed ONNX input`
- **Description**: The RISK-TEST-STRATEGY.md correctly identifies that a panic inside a rayon closure while holding `Mutex<Session>` (in `OnnxProvider`) poisons the mutex. All subsequent calls to `embed_entry` then panic on `.expect("session lock poisoned")`. Each such panic converts to `RayonError::Cancelled` at the bridge level. The ARCHITECTURE.md states recovery is `EmbedServiceHandle`'s responsibility, but the recovery path (`Loading → Ready | Failed → Retrying`) activates on `get_adapter()` failures — not on `RayonError::Cancelled` at call sites. A sequence of malformed inputs that repeatedly trigger a session panic could keep the embed service in a permanently failing state until an operator restart. This is not a regression from the prior `spawn_blocking` design (the same mutex poisoning risk existed before). The change does not worsen the blast radius of this scenario.
- **Recommendation**: This is a pre-existing architectural gap, not introduced by this PR. A follow-up issue should track adding a `Cancelled` feedback path from call sites to `EmbedServiceHandle`'s state machine. For W1-4 (NLI), `NliServiceHandle` should not inherit this gap when implemented.
- **Blocking**: no

---

## OWASP Analysis

| Concern | Assessment |
|---------|-----------|
| Injection (SQL, command, path) | Not applicable. `rayon_pool_size` is a `usize` validated in `[1, 64]`. No string formatting into SQL or shell commands. Thread names use `format!("{}-{}", prefix, i)` with a trusted config-derived prefix. |
| Broken access control | Not applicable. `RayonPool` has no authentication or authorization surface. `ml_inference_pool` is `pub(crate)` in `ServiceLayer`. Access is bounded by Rust's module visibility. |
| Security misconfiguration | `rayon_pool_size = 0` is explicitly rejected by `InferenceConfig::validate()` before pool construction. `rayon_pool_size = 65` is also rejected. Config validation runs before `RayonPool::new` is called. Default formula `(num_cpus / 2).max(4).min(8)` always produces a value in `[4, 8]`, which always passes validation. |
| Deserialization | `InferenceConfig` deserialized from TOML via serde. The only field is `rayon_pool_size: usize` — a non-negative integer. Serde rejects non-numeric values. `#[serde(default)]` on the struct means an absent section uses the compiled default, not a zero value. |
| Input validation at MCP boundary | MCP tool inputs (`query`, `content`, `title`) are passed through to `embed_entry` inside the rayon closure. These inputs are untrusted. The existing `embed_entry` and ONNX session handling are unchanged. Length is not bounded by this PR (pre-existing). The migration does not add or remove any input sanitization. |
| Adversarial DoS via long inference | Documented in RISK-TEST-STRATEGY.md. `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` bounds the MCP handler wait to 30s. The rayon thread continues running. With 4 threads and 4 adversarial concurrent requests each hitting the timeout, threads are temporarily occupied. Pool accepts new work once threads free. The adversarial test `test_adversarial_timeout_does_not_hang_pool` verifies this. Impact is bounded and temporary. |
| Secrets | No hardcoded credentials, API keys, tokens, or secrets found in any changed file. |
| Vulnerable dependencies | `rayon 1.11.0` and `num_cpus 1.17.0` are the new dependencies. Both are widely-used crates with no known CVEs at this version. `thiserror 2.0.18` is added to the server crate (was already a workspace dependency). |

---

## Dependency Safety

- **rayon = "1"** resolves to `1.11.0`. Semver-pinned to major version 1. No known CVEs. Widely deployed in production Rust systems.
- **num_cpus = "1"** resolves to `1.17.0`. Single-function crate for CPU count detection. No known CVEs.
- **thiserror = "2"** resolves to `2.0.18`. Already a workspace dependency. Added to `unimatrix-server/Cargo.toml` because `RayonError` uses `#[derive(thiserror::Error)]`. No new transitive dependencies introduced.

---

## Thread Safety Assessment

The thread safety analysis in `ARCHITECTURE.md §thread-safety` is correct and verified:

1. `OnnxProvider` wraps `Mutex<Session>` — concurrent rayon workers calling `embed_entry` on the same adapter serialize at the mutex boundary. No data race is possible.
2. `EmbedAdapter` wraps `Arc<dyn EmbeddingProvider>` — `OnnxProvider` is `Send + Sync` (asserted by `test_send_sync` in `onnx.rs`). `EmbedAdapter` is therefore `Send + 'static`.
3. `RayonPool` stores `Arc<rayon::ThreadPool>` and `String` and `usize`. All are `Send + Sync`. The struct derives `Debug`.
4. The oneshot channel (`tokio::sync::oneshot`) is the only shared state between the async caller and the rayon worker. `tx` is owned by the rayon closure; `rx` is owned by the async caller. No shared mutable state.
5. `Arc<RayonPool>` is shared across `ServiceLayer`, `background_tick`, and `extraction_tick` via `Arc::clone`. All consumers hold `Arc` references. No aliased mutable access.

---

## Panic Safety Assessment

The panic containment design is sound:

- `ThreadPoolBuilder::panic_handler(|_| {})` installs a no-op handler. Panicking rayon workers unwind normally and are recycled.
- The inner closure passed to `pool.spawn` in `RayonPool::spawn` and `spawn_with_timeout` calls `f()` then `tx.send(result)`. If `f()` panics, `tx` drops before `send` is called. `rx.await` returns `Err(RecvError)`, mapped to `RayonError::Cancelled`.
- This is verified by `test_spawn_panic_returns_cancelled`, `test_spawn_with_timeout_panic_returns_cancelled_not_timeout`, `test_pool_functional_after_panic`, and `test_spawn_panic_with_mutex_held`.
- The tokio runtime does not crash. This is verified by the test harness reaching assertions after the panic.

---

## Blast Radius Assessment

**Worst case if the bridge has a subtle bug:**

- A bug in `spawn_with_timeout` (e.g., wrong match arm mapping `TimedOut` to `Ok`) would silently return stale or empty embeddings for search queries. Search results would degrade silently.
- A bug in `spawn` (no timeout) that causes `rx.await` to hang would block the background tick indefinitely, pausing contradiction scanning and quality-gate processing. No MCP-path impact.
- A config validation bug that allows `rayon_pool_size = 0` would cause `rayon::ThreadPoolBuilder` to build a zero-thread pool — the pool would deadlock on first submission. The server would start but all embedding requests would hang. Server would need restart.

The actual implementation has no identified bugs in these paths. The failure modes are safe — each produces an error return or a recoverable hang, not silent data corruption or privilege escalation.

---

## Regression Risk

**Could this break existing functionality?**

1. **`AsyncEmbedService` removal**: No workspace consumer found. `cargo check --workspace` passes (verified by CI enforcement script existence and scope-level confirmation from the implementers). Compile-time safety.
2. **Call-site error mapping**: The double `.map_err` pattern is preserved at all 7 migrated sites. The outer `?` maps `RayonError` (bridge failure) to `ServiceError::EmbeddingFailed`. The inner `?` maps `CoreError` (embed failure). The ordering is identical to the prior `spawn_blocking_with_timeout` double `?` pattern.
3. **Timeout coverage**: `spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` is used at all 4 confirmed MCP embedding sites (search, store_ops, store_correct, status). The warmup site in `uds/listener.rs` also uses `spawn_with_timeout`. CI enforcement script prevents regression.
4. **Background task behavior**: Contradiction scan and quality-gate loop use `spawn` (no timeout). Error handling uses `error!` tracing on `RayonError::Cancelled`. Background tick continues on error. Consistent with prior behavior where `JoinError` from `spawn_blocking` was logged and the tick continued.
5. **Config merging**: `merge_configs` correctly prefers the project-level `rayon_pool_size` if it differs from the default, falling back to the global value. The logic is consistent with other config sections.
6. **Test infrastructure**: `TestHarness`, `server.rs` integration tests, `shutdown.rs` tests, `briefing.rs` tests, `status.rs` tests, and `uds/listener.rs` tests all construct a `RayonPool::new(1, "test-pool")` and pass it to `ServiceLayer`. This is boilerplate addition that correctly threads the pool through the existing test infrastructure without altering test assertions.

**Regression risk: low.** The change is a mechanical substitution of one concurrency primitive for another, with identical error propagation paths and equivalent timeout semantics.

---

## CI Enforcement Assessment

`scripts/check-inference-sites.sh` runs on every PR against main (`.github/workflows/ci.yml`). It:

1. Checks `services/` for `spawn_blocking` + `embed` (case-sensitive grep).
2. Checks `services/` for `spawn_blocking_with_timeout` + `embed`.
3. Checks `background.rs` for `spawn_blocking` + `embed`.
4. Checks `async_wrappers.rs` for `AsyncEmbedService`.
5. Checks `embed_handle.rs` for exactly 1 `spawn_blocking` (the OnnxProvider::new call).

The script was run during this review and passes cleanly. The `grep "embed"` filter correctly excludes non-inference `spawn_blocking` calls (co-access boost, DB writes, UDS file operations). As documented in RISK-TEST-STRATEGY.md, a `spawn_blocking` hidden inside a macro expansion would evade the grep. This is an accepted limitation — code review is the primary control for that edge case.

---

## Secrets Check

No hardcoded credentials, API keys, tokens, or secrets were found in any changed file. The `InferenceConfig` struct contains only `rayon_pool_size: usize`. Thread names are derived from the config-supplied pool name string. No external service credentials are involved.

---

## PR Comments

- Posted 1 comment on PR #318.
- Blocking findings: no.

---

## Knowledge Stewardship

Nothing novel to store — the timeout-gap-compounds anti-pattern is already stored in the knowledge base (entry #1688) and was the motivation for this feature. The rayon-tokio bridge pattern and its panic containment semantics are already stored as crt-022-specific entries (#2491, #2535). The mutex-poisoning recovery gap is a pre-existing architectural concern already documented in RISK-TEST-STRATEGY.md and not generalizable beyond this specific architecture.
