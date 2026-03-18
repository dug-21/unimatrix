# Agent Report: nxs-011-agent-4-wave2-store

**Feature:** nxs-011 — rusqlite → sqlx 0.8 dual-pool migration (Wave 2)
**Issue:** #298
**Branch:** feature/nxs-011
**Commit:** b93d91a

## Scope

Wave 2 implementation: complete migration of all crates from rusqlite 0.34 (sync `Mutex<Connection>`) to sqlx 0.8 `SqlxStore` with dual write/read pool architecture. Fix all test failures introduced by Wave 1 and prior migration work.

## Files Created/Modified

### unimatrix-store (core migration)
- `crates/unimatrix-store/src/db.rs` — SqlxStore dual-pool impl
- `crates/unimatrix-store/src/analytics.rs` — async analytics drain
- `crates/unimatrix-store/src/read.rs` — all read ops async via read_pool
- `crates/unimatrix-store/src/write.rs` — all write ops async via write_pool
- `crates/unimatrix-store/src/write_ext.rs` — record_feature_entries direct write (not drain)
- `crates/unimatrix-store/src/sessions.rs` — insert_session direct write (not drain)
- `crates/unimatrix-store/src/signal.rs` — insert_signal direct write (not drain); removed unused AnalyticsWrite import
- `crates/unimatrix-store/src/migration.rs` — async migration engine
- `crates/unimatrix-store/src/error.rs` — StoreError with sqlx variants
- `crates/unimatrix-store/src/pool_config.rs` — PoolConfig
- `crates/unimatrix-store/src/audit.rs` — new (audit ops migrated from server infra)
- `crates/unimatrix-store/src/observations.rs` — new (observation ops migrated)
- `crates/unimatrix-store/src/registry.rs` — new (registry ops migrated)
- `crates/unimatrix-store/src/txn.rs` — deleted (rusqlite txn wrapper, no longer needed)
- `crates/unimatrix-store/src/test_helpers.rs` — async test helpers
- `crates/unimatrix-store/tests/sqlite_parity.rs` — all calls updated to async
- `crates/unimatrix-store/tests/sqlite_parity_specialized.rs` — insert_signal/.await.unwrap() fixes
- `crates/unimatrix-store/tests/migration_v10_to_v11.rs` — async migration tests
- `crates/unimatrix-store/tests/migration_v11_to_v12.rs` — insert_session/.await.unwrap() fixes

### unimatrix-server
- `crates/unimatrix-server/src/export.rs` — block_export_sync helper using block_in_place
- `crates/unimatrix-server/src/uds/listener.rs` — insert_signal production caller (async direct); test callers (.await.unwrap()); content_based_attribution_fallback uses block_in_place; attribution tests use multi_thread
- `crates/unimatrix-server/src/uds/mcp_listener.rs` — build_test_server uses block_in_place; tests use multi_thread
- `crates/unimatrix-server/src/services/observation.rs` — load_feature_observations uses block_sync helper
- `crates/unimatrix-server/src/services/usage.rs` — record_feature_entries async call sites
- `crates/unimatrix-server/src/server.rs` — record_feature_entries .await
- `crates/unimatrix-server/src/infra/` — audit, registry, contradiction, shutdown migrated
- `crates/unimatrix-server/src/background.rs` — async background tasks
- `crates/unimatrix-server/tests/import_integration.rs` — store.close().await.unwrap() before run_import
- `crates/unimatrix-server/tests/export_integration.rs` — async test helpers
- All remaining server crates migrated from rusqlite to sqlx

### Other crates
- `crates/unimatrix-core/` — async adapter wrappers
- `crates/unimatrix-engine/` — coaccess async
- `crates/unimatrix-observe/` — extraction rules async
- `crates/unimatrix-vector/` — sqlx pool config

## Test Results

```
cargo test --workspace
All test result lines: ok (zero failures)
1365 lib tests (unimatrix-server) + 16 import integration + 16 export integration
Total: ~2200+ tests passing, 0 failing
```

## Key Bugs Fixed

### 1. Analytics drain misuse for immediate-read methods
`insert_session`, `insert_signal`, and `record_feature_entries` were incorrectly routed through `enqueue_analytics` (500ms flush interval) in Wave 1. All three restored to direct async `write_pool` writes.

### 2. `Cannot start a runtime from within a runtime` (4 locations)
- `export.rs`: `Builder::new_current_thread().block_on()` inside async test → `block_export_sync` using `block_in_place` pattern
- `mcp_listener.rs`: same pattern → `block_in_place` + tests use `multi_thread`
- `observation.rs`: `Handle::current().block_on()` → existing `block_sync` helper
- `listener.rs` `content_based_attribution_fallback`: `Handle::current().block_on()` → `block_in_place`; tests use `multi_thread`

### 3. SQLITE_BUSY on test store reuse
`drop(store)` doesn't await async pool close. Fixed by `store.close().await.unwrap()` before `run_import` opens same database.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for unimatrix-store — found patterns for drain task lifecycle (#2057) and batched writes (#731), neither covered analytics drain immediate-read issue
- Stored: entry #2125 "Analytics drain unsuitable for writes that callers read back immediately" via `/uni-store-pattern`
- Stored: entry #2126 "Use block_in_place (not Handle::current().block_on) when bridging async into sync from within an existing tokio runtime" via `/uni-store-pattern`
