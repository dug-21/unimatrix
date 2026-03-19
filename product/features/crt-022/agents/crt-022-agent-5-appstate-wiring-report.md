# crt-022 Agent 5 Report: AppState/ServiceLayer Wiring (Wave 2)

**Agent ID**: crt-022-agent-5-appstate-wiring
**Task**: Wire `Arc<RayonPool>` into `AppState`/`ServiceLayer` (Wave 2 — Dependency Layer)

---

## Files Modified

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/error.rs` | Added `ServerError::InferencePoolInit(String)` variant with Display + `From<ServerError> for ErrorData` arm |
| `crates/unimatrix-server/src/services/mod.rs` | Added `ml_inference_pool: Arc<RayonPool>` field to `ServiceLayer`; updated `new` and `with_rate_config` signatures; added `// TODO(W2-4)` comment |
| `crates/unimatrix-server/src/main.rs` | Added `RayonPool` import + `// TODO(W3-1)` comment; wired `InferenceConfig::validate()` + `RayonPool::new()` + `Arc::new(pool)` in both `tokio_main_daemon` and `tokio_main_stdio`; passed `Arc::clone(&ml_inference_pool)` to `ServiceLayer::new` |
| `crates/unimatrix-server/src/test_support.rs` | Updated `ServiceLayer::with_rate_config` call to pass `RayonPool::new(1, "test-pool")` |
| `crates/unimatrix-server/src/server.rs` | Updated `ServiceLayer::new` call to pass `RayonPool::new(1, "test-pool")` |
| `crates/unimatrix-server/src/uds/listener.rs` | Updated test helper `make_services` to pass `RayonPool::new(1, "test-pool")` |
| `crates/unimatrix-server/src/infra/shutdown.rs` | Updated both `ServiceLayer::new` calls in test helpers to pass `RayonPool::new(1, "test-pool")` |

---

## Compile Result

**PASS** — zero errors, zero new warnings.

```
cargo build --workspace: Finished `dev` profile in 23.60s (no errors)
cargo check -p unimatrix-server: no output (clean)
cargo clippy -p unimatrix-server: no output (clean)
```

---

## Test Result

**1482 passed / 0 new failures**

Pre-existing failure (unrelated to this change):
- `uds::listener::tests::col018_long_prompt_truncated` — present before crt-022 branch started (confirmed via `git show 760c7da`). Not introduced or touched by this agent.

---

## Design Decisions Followed

- **ADR-004**: `Arc<RayonPool>` placed on `ServiceLayer` (the server-layer shared state container); single construction in `main.rs`; distributed via `Arc::clone` to all consumers.
- **C-05**: Exactly one pool constructed; field named `ml_inference_pool`.
- **C-10**: Single construction site in `main.rs` (two async entry points share identical wiring).
- **Pseudocode spec**: `ServiceLayer::new` and `with_rate_config` both updated with `ml_inference_pool: Arc<RayonPool>` as final parameter; `TODO(W2-4)` comment placed on the struct field; `TODO(W3-1)` at OnnxProvider import site.
- **Error handling**: `ConfigError` from `validate()` and `ThreadPoolBuildError` from `RayonPool::new()` both map to `ServerError::InferencePoolInit(e.to_string())` — structured startup abort, no `.unwrap()` in production code.

---

## Scope Note for Wave 3 Agents

`self.rayon_pool` is now available on `ServiceLayer` as `pub(crate) ml_inference_pool: Arc<RayonPool>`.

Wave 3 call-site agents need to:
1. Clone `Arc::clone(&self.ml_inference_pool)` into each service struct (`SearchService`, `StoreService`, `StatusService`)
2. Pass `Arc::clone` of the pool to `spawn_background_tick` and `start_uds_listener` signatures

The `ServiceLayer::with_rate_config` already receives the pool and stores it on the struct — Wave 3 agents can access it via `ml_inference_pool` when constructing child services.

---

## Knowledge Stewardship

- **Queried**: `/uni-query-patterns` for `unimatrix-server` — found entry #316 (ServiceLayer extraction pattern) and #1560 (Arc<RwLock> background tick pattern). Both confirmed the established `Arc<...>` field distribution pattern used here.
- **Stored**: entry #2552 "ServiceLayer::new signature extension: always add new Arc fields at the END of the parameter list" via `/uni-store-pattern` — documents all 5 test call sites that need updating and the test-pool construction idiom. Not previously documented.
