# Agent Report: col-023-agent-9b-registry-wiring-rework

**Feature:** col-023
**Task:** Gate 3b REWORKABLE FAIL — wire DomainPackRegistry into SqlObservationSource retrospective call sites
**Outcome:** PASS — all 5 call sites fixed, cargo test --workspace clean

---

## Changes Made

### Files Modified

1. **`crates/unimatrix-server/src/services/status.rs`**
   - Added `use unimatrix_observe::domain::DomainPackRegistry;`
   - Added `observation_registry: Arc<DomainPackRegistry>` field to `StatusService`
   - `StatusService::new()` gains 8th parameter `observation_registry: Arc<DomainPackRegistry>`
   - Phase 6 call site: `SqlObservationSource::new_default(...)` → `SqlObservationSource::new(..., Arc::clone(&self.observation_registry))`
   - Test helper `make_status_service()` passes `DomainPackRegistry::with_builtin_claude_code()`

2. **`crates/unimatrix-server/src/services/mod.rs`**
   - `ServiceLayer::new()` and `ServiceLayer::with_rate_config()` gain final parameter `observation_registry: Arc<DomainPackRegistry>`
   - Forwarded through to `StatusService::new()`

3. **`crates/unimatrix-server/src/server.rs`**
   - Added `pub observation_registry: Arc<DomainPackRegistry>` field to `UnimatrixServer`
   - Default-initialized with `with_builtin_claude_code()` (overwritten by main.rs startup)
   - `ServiceLayer::new()` call passes built-in default

4. **`crates/unimatrix-server/src/mcp/tools.rs`**
   - First call site (~line 1115, `load_feature_observations`): `new_default(store)` → `new(store, Arc::clone(&self.observation_registry))`
   - Second call site (~line 1368, `discover_sessions_for_feature`): same fix

5. **`crates/unimatrix-server/src/main.rs`**
   - Daemon path: `_observation_registry` → `observation_registry`; threaded into `ServiceLayer::new()` and assigned to `server.observation_registry`
   - Stdio path: identical rename and threading

6. **`crates/unimatrix-server/src/background.rs`**
   - `StatusService::new()` call updated with `DomainPackRegistry::with_builtin_claude_code()` (background tick uses `load_maintenance_snapshot`, not `compute_report`, so configured registry is never consulted here)

7. **`crates/unimatrix-server/src/test_support.rs`**
   - `ServiceLayer::with_rate_config()` call updated with built-in default registry

8. **`crates/unimatrix-server/src/eval/profile/layer.rs`**
   - `ServiceLayer::with_rate_config()` call updated with built-in default registry

9. **`crates/unimatrix-server/src/infra/shutdown.rs`** (2 test-only call sites)
   - Both `ServiceLayer::new()` calls updated with built-in default registry

10. **`crates/unimatrix-server/src/uds/listener.rs`** (1 test-only call site)
    - `ServiceLayer::new()` call updated with built-in default registry

---

## Test Results

```
cargo check --workspace  — 0 errors, 0 new warnings
cargo test --workspace   — all test results: ok (zero failures across all crates)
```

---

## Key Finding

`cargo check` only compiles non-test code paths. The 3 remaining E0061 errors after the initial production fix were in `#[cfg(test)]` helpers in `infra/shutdown.rs` and `uds/listener.rs` — invisible to `cargo check`, only caught by `cargo test`.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server ServiceLayer wiring startup` — found pattern #2328 (dsn-001 Startup Config Injection) as nearest match; no existing pattern covered the cargo-check-vs-test blind spot for test-only call sites
- Stored: entry #2938 "Adding a new Arc parameter to ServiceLayer::new() requires updating 5+ call sites across unimatrix-server" via `/uni-store-pattern` — captures the `cargo check` vs `cargo test` compilation scope difference that caused the missed test helpers
