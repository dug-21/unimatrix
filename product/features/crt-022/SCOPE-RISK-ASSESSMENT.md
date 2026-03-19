# Scope Risk Assessment: crt-022

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `ort = "2.0.0-rc.9"` is a release candidate — RC crates may have undocumented thread-safety constraints. Moving ONNX sessions from `spawn_blocking` (serial mutex-gated, ADR-001/ADR-002 in entries #67, #68) to rayon's work-stealing scheduler changes the concurrency model. If `OrtSession` is not `Send` or has per-thread affinity requirements, rayon workers will panic or produce UB at runtime. | High | Med | Architect must verify `OrtSession` / `EmbedAdapter` are `Send + 'static` before finalising the bridge signature. Confirm thread-safety guarantees in `ort` RC changelog. |
| SR-02 | Rayon is a new first-party dependency (confirmed absent from all workspace `Cargo.toml` files). Rayon's `ThreadPoolBuilder` can fail at construction time. Startup failure path must produce a structured error and abort, not panic or silently fall back to `spawn_blocking`. | Low | Low | Architect must wire `ThreadPoolBuildError` into the server's startup error propagation chain (same pattern as config validation abort). |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | OQ-2 (deferred timeout semantics) is an open architectural question that affects every migrated call site. `spawn_blocking_with_timeout` enforced `MCP_HANDLER_TIMEOUT` on embedding calls. Removing that wrapper without a replacement leaves the MCP handler without a timeout bound on inference — a hung ONNX session would suspend the handler indefinitely. Entry #1688 documents the lesson that timeout coverage gaps introduced at one callsite compound across the codebase. | High | Med | Architect must decide timeout strategy before call-site migration begins. Options: (a) `tokio::time::timeout` wrapping `rx.await` at each call site; (b) a `spawn_with_timeout` variant in `RayonPool`; (c) explicit acceptance that rayon work-stealing prevents indefinite queuing and document the reasoning. |
| SR-04 | The contradiction scan closure (`background.rs:543`) is the longest-running ONNX call — it iterates over all entries calling `embed_entry` in a loop, all inside a single `rayon_pool.spawn(...)`. This monopolises a rayon thread for the full scan duration. If the pool is sized at `max(num_cpus/2, 2).min(8)` and one thread is consumed by a scan, concurrent search and store embedding calls share the remaining threads. Under light pool sizing (e.g., 2 threads), a scan + two concurrent MCP calls saturates the pool. | Med | Med | Architect should evaluate whether the contradiction scan should yield between entries (inner-loop `rayon_spawn` per entry) or accept bounded monopolisation with a documented pool-size floor. The `[inference] rayon_pool_size` default formula must account for this workload mix. |
| SR-05 | AC-07 requires a post-ship grep audit: "no `spawn_blocking` for ONNX inference anywhere". Eight call sites are enumerated in SCOPE.md. If a ninth site exists (e.g., a test helper or a background path not reached by the search), the constraint will be silently violated. Lesson from entry #1688: coverage gaps compound. | Med | Low | Spec writer should mandate a compile-time or CI enforcement mechanism (e.g., a `deny(spawn_blocking_onnx)` lint comment or a grep-based CI step) rather than relying on post-ship audit. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | `Arc<RayonPool>` must be threaded into every inference consumer at startup wiring (`main.rs`). W1-4 (NLI) and W3-1 (GNN) will also need this pool. If the wiring is done ad-hoc per consumer rather than from a single owned `AppState`, future callers may instantiate a second pool inadvertently, violating the "single shared pool" constraint (Constraint 5 in SCOPE.md). | Med | Med | Architect should place `Arc<RayonPool>` in a top-level `AppState` struct distributed at startup rather than passing it as a discrete parameter to each subsystem. This also simplifies W1-4 integration. |
| SR-07 | `AsyncEmbedService` removal (`unimatrix-core/src/async_wrappers.rs`) is confirmed to have zero consumers in `unimatrix-server`. However, if any external crate, test binary, or integration test imports it, removal will produce a compile error not caught by the server's own build. | Low | Low | Architect should run `cargo check --workspace` as an explicit AC before shipping. Spec writer should add a workspace-level compilation test to AC-05. |

## Assumptions

- **SCOPE.md §Background Research — "AsyncEmbedService Is Not the Server's Primary Embedding Path"**: Assumes no import of `AsyncEmbedService` in any server source. If this grep was performed against the current HEAD and a branch has added such an import, the assumption is stale.
- **SCOPE.md §Background Research — "Current Crate Dependencies"**: Assumes rayon's transitive dependencies do not conflict with `ort` or `tokio`. This is very likely safe but was not verified in the scope document.
- **SCOPE.md §Proposed Approach — Phase 1**: Default pool size formula `(num_cpus::get() / 2).max(2).min(8)` assumes the deployment machine is not single-core. On a single-core machine the formula yields 2 threads, which may be excessive relative to the available CPU. This is an edge case but real in container environments.

## Design Recommendations

- **SR-03 (priority 1)**: Resolve OQ-2 before architecture is drafted. The timeout decision is load-bearing — it changes the `RayonPool::spawn` signature and every call-site migration pattern. Deferring it into implementation is the primary schedule risk for this feature.
- **SR-01 (priority 2)**: Validate `OrtSession` / `EmbedAdapter` thread safety against the `ort` RC before committing to the bridge API. A `Send` bound failure requires a mutex-per-call fallback that changes the architecture substantially.
- **SR-04 (priority 3)**: Specify the contradiction scan threading model explicitly in the architecture. The pool-size default is not defensible until the monopolisation envelope is bounded.
- **SR-06**: Use a single `AppState` for pool distribution to avoid re-instantiation in W1-4 and W3-1.
- **SR-05**: Convert AC-07 from an audit checklist item to a CI enforcement step in the specification.
