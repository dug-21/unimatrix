# Agent Report: crt-022-agent-3-rayon-pool

**Feature**: crt-022 — Rayon Thread Pool + Embedding Migration
**Wave**: W1 (Foundation)
**Component**: `RayonPool` + `RayonError` module

---

## Deliverables

### Files Created

- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/rayon_pool.rs` — new file, 637 lines

### Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/Cargo.toml` — added `rayon = "1"`, `num_cpus = "1"`, `thiserror = "2"`
- `/workspaces/unimatrix/crates/unimatrix-server/src/infra/mod.rs` — added `pub mod rayon_pool;` and `pub use rayon_pool::{RayonError, RayonPool};`

Note: `InferenceConfig` struct, `ConfigError::InferencePoolSizeOutOfRange` variant, `UnimatrixConfig::inference` field, and `merge_configs` inference handling were already committed by agent-4 (crt-022-agent-4-config-and-removal-report) in commit `97c38ee` before this agent ran. This agent consumed `InferenceConfig` from `crate::infra::config` in tests.

---

## Implementation Summary

`RayonPool` wraps `Arc<rayon::ThreadPool>` with:

- `new(num_threads, name)` — builds named pool via `ThreadPoolBuilder`; thread names are `{name}-{i}` for observability
- `spawn<F,T>(f)` — tokio oneshot bridge; no timeout; for background tasks
- `spawn_with_timeout<F,T>(timeout, f)` — wraps `rx.await` with `tokio::time::timeout`; for MCP handler paths
- `pool_size()` and `name()` accessors

`RayonError` enum:
- `Cancelled` — tx dropped (closure panicked or pool shut down)
- `TimedOut(Duration)` — timeout elapsed before rx resolved

Module-level rustdoc documents the MCP vs background usage convention (ADR-002).

---

## Tests

**21/21 pass.** All test plan scenarios from `test-plan/rayon_pool.md` implemented:

| Section | Tests |
|---------|-------|
| §pool-init | 4 tests (single/eight threads, default formula, name retained) |
| §spawn-dispatch | 3 tests (basic, with-timeout variant, complex type) |
| §panic-containment | 4 tests (spawn panic, spawn_with_timeout panic, pool functional after panic, mutex held) |
| §timeout-semantics | 4 tests (fires on excess, duration preserved, pool accepts submissions after, size accessor unchanged) |
| §concurrency | 2 tests (full occupancy barrier, two-background-two-mcp) |
| §shutdown | 1 test (pool drop cancels queued) |
| §error-display | 2 tests (Cancelled display, TimedOut display) |
| §adversarial | 1 test (4 adversarial timeouts, pool still accepts new work) |

Build: `cargo build --workspace` — zero errors. Workspace lib tests: 2570 pass, 0 fail.

---

## Non-Deviation from Pseudocode

Implementation follows `pseudocode/rayon_pool.md` exactly. One deviation from the IMPLEMENTATION-BRIEF.md `RayonPool` struct layout: the brief showed only `ml_inference_pool: Arc<rayon::ThreadPool>`, but the pseudocode specified three fields (`ml_inference_pool`, `pool_name`, `pool_threads`). The pseudocode is the authoritative spec — three-field struct implemented as specified. The `pool_size()` accessor returns `self.pool_threads` (the configured count) rather than querying `current_num_threads()` on the pool; this matches the pseudocode spec.

---

## Blocker Encountered and Resolved

**SIGABRT in test harness from rayon's default panic handler.**

Rayon's `ThreadPoolBuilder` has a default panic handler that calls `std::process::abort()` when a worker thread panics. This fires at pool drop time even when the panic is fully contained by the oneshot channel drop (`tx` dropped → `rx.await` returns `Err` → `RayonError::Cancelled`). Result: panic-containment tests passed individually, but the process aborted with `signal: 6, SIGABRT` at the end of each test that triggered a panic.

Fix: install `.panic_handler(|_payload| {})` on `ThreadPoolBuilder`. This discards the secondary propagation; the oneshot boundary remains the sole signal path. The pool recycles the panicking thread normally.

---

## Commit

`8f57c22` — `impl(rayon_pool): RayonPool + RayonError module with tokio oneshot bridge (#317)`

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` with query "tokio rayon bridge oneshot async implementation patterns" — found entry #2491 (Rayon-Tokio bridge pattern, crt-022 tagged) already stored from design phase. No new design-phase patterns to apply.
- Stored: entry #2543 "Rayon panic_handler required to prevent SIGABRT in test harness" via `/uni-store-pattern` — documents the `.panic_handler(|_| {})` requirement that is invisible in source code and would be rediscovered by every future rayon pool implementer.
