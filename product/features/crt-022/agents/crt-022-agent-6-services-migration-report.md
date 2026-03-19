# Agent Report: crt-022-agent-6-services-migration

**Task**: Wave 3 — Migrate 4 MCP handler call sites off `spawn_blocking_with_timeout` onto `rayon_pool.spawn_with_timeout`

---

## Files Modified

- `crates/unimatrix-server/src/services/search.rs`
- `crates/unimatrix-server/src/services/store_ops.rs`
- `crates/unimatrix-server/src/services/store_correct.rs`
- `crates/unimatrix-server/src/services/status.rs`
- `crates/unimatrix-server/src/services/mod.rs`
- `crates/unimatrix-server/src/services/briefing.rs` (test helper only)
- `crates/unimatrix-server/src/background.rs` (plumbing — `StatusService::new` call site)
- `crates/unimatrix-server/src/main.rs` (two `spawn_background_tick` call sites)

---

## Changes Made

### Site 1 — `services/search.rs` ~line 228
- Added `rayon_pool: Arc<RayonPool>` field to `SearchService` struct
- Updated `SearchService::new` to accept `rayon_pool: Arc<RayonPool>` parameter
- Replaced `spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, ...)` with `self.rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`
- Retained `spawn_blocking_with_timeout` for the co-access boost at ~line 462 (DB read, not inference — must NOT be migrated)
- Updated import: kept `spawn_blocking_with_timeout` (needed for co-access), added `RayonPool` import

### Site 2 — `services/store_ops.rs` ~line 113
- Added `rayon_pool: Arc<RayonPool>` field to `StoreService` struct
- Updated `StoreService::new` to accept `rayon_pool: Arc<RayonPool>` parameter
- Replaced `spawn_blocking_with_timeout` with `self.rayon_pool.spawn_with_timeout`

### Site 3 — `services/store_correct.rs` ~line 50
- `StoreCorrectService` is `impl StoreService` — inherits the new `rayon_pool` field automatically
- Replaced `spawn_blocking_with_timeout` with `self.rayon_pool.spawn_with_timeout`
- Updated import: removed `spawn_blocking_with_timeout`, kept `MCP_HANDLER_TIMEOUT`

### Site 4 — `services/status.rs` ~line 542
- Added `rayon_pool: Arc<RayonPool>` field to `StatusService` struct
- Updated `StatusService::new` to accept `rayon_pool: Arc<RayonPool>` parameter
- Replaced `spawn_blocking_with_timeout` with `self.rayon_pool.spawn_with_timeout`
- Test helper `make_status_service` updated to supply `RayonPool::new(1, "test_pool")`

### `services/mod.rs` — `ServiceLayer::with_rate_config`
- Passed `Arc::clone(&ml_inference_pool)` to `SearchService::new`, `StoreService::new`, `StatusService::new`

### `background.rs` — Required plumbing for `StatusService::new` call
- Added `ml_inference_pool: Arc<RayonPool>` parameter to `spawn_background_tick`, `background_tick_loop`, `run_single_tick`
- Updated `StatusService::new` call in `run_single_tick` to pass `Arc::clone(ml_inference_pool)`
- Note: the embedding call site migrations in `background.rs` (~543, ~1162) are out of scope for this agent; only the constructor plumbing was touched

### `main.rs`
- Added `Arc::clone(&ml_inference_pool)` to both `spawn_background_tick` call sites (daemon and stdio paths)

### `briefing.rs`
- Updated test helper `make_briefing_service` to supply `RayonPool::new(1, "test_pool")` to `SearchService::new`

---

## Double `.map_err` Pattern Preservation

All 4 migrated sites preserve the exact double `.map_err` pattern:
```rust
self.rayon_pool
    .spawn_with_timeout(MCP_HANDLER_TIMEOUT, { ... })
    .await
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?  // outer: RayonError
    .map_err(|e| ServiceError::EmbeddingFailed(e.to_string()))?; // inner: CoreError
```

Exception: `status.rs` Site 4 uses `match ... { Ok(Ok(...)) => ..., _ => ... }` for graceful degradation (same as before).

---

## Compile Result

**Pass** — `cargo build --workspace` exits 0 with zero errors.

---

## Tests

**Pass** — `cargo test -p unimatrix-server --lib`: 1483 passed, 0 failed.

---

## Issues / Blockers

None. One non-obvious dependency discovered: changing `StatusService::new` and `SearchService::new` signatures requires updating test helpers inside `#[cfg(test)]` blocks in `status.rs` and `briefing.rs`. These are only caught by `cargo test --lib`, not `cargo check`.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` (spawn_blocking_with_timeout rayon migration, crt-022 architectural decisions) — found ADR-002 (timeout at bridge), ADR-001 (rayon in server only), Rayon-Tokio bridge pattern entries. All confirmed alignment with implementation approach.
- Stored: entry #2553 "Changing a service constructor signature forces changes in every instantiation site including background.rs test helpers" via `/uni-store-pattern` — documents the gotcha that `cargo check` passes but `cargo test --lib` catches test-helper constructor mismatches, and the pattern for background.rs threading.
