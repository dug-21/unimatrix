# Test Plan: SqlxStore (db.rs)

**Component**: `crates/unimatrix-store/src/db.rs`
**Risks**: R-01 (pool starvation), R-02 (drain task teardown), R-06 (write path isolation), R-10 (concurrent read correctness)
**ACs**: AC-02, AC-08, AC-10, AC-19

---

## Unit Tests

### SS-U-01: `test_sqlx_store_open_succeeds_with_valid_config`
- **Arrange**: temp DB path, `PoolConfig::test_default()`
- **Act**: `SqlxStore::open(path, config).await`
- **Assert**: Returns `Ok(SqlxStore)`; no panic
- **Teardown**: `store.close().await`
- **Risk**: Baseline open path

### SS-U-02: `test_sqlx_store_open_rejects_write_max_3` — (AC-09)
- **Act**: `SqlxStore::open(path, PoolConfig { write_max_connections: 3, .. }).await`
- **Assert**: Returns `Err(StoreError::InvalidPoolConfig { reason })`; `reason` mentions write cap
- **Risk**: R-01

### SS-U-03: `test_sqlx_store_shed_events_total_is_zero_at_open`
- **Arrange**: Open fresh store
- **Assert**: `store.shed_events_total() == 0`
- **Teardown**: `store.close().await`
- **Risk**: R-04

### SS-U-04: `test_sqlx_store_close_returns_without_panic_on_empty_queue`
- **Arrange**: Open store; enqueue no events
- **Act**: `store.close().await`
- **Assert**: Returns without panic; completes quickly (under 1s with test_default config)
- **Risk**: R-02 (edge case 6: shutdown with empty queue)

### SS-U-05: `test_sqlx_store_open_write_max_1_valid`
- **Arrange**: `PoolConfig { write_max_connections: 1, .. }`
- **Assert**: `open()` returns `Ok`
- **Teardown**: `store.close().await`
- **Risk**: R-01 (boundary: write_max=1 is valid)

---

## Integration Tests (`#[tokio::test]` in `unimatrix-store/tests/`)

### SS-I-01: `test_store_write_and_read_entry_roundtrip`
- **Arrange**: Open store, create a `NewEntry`
- **Act**: `store.insert(entry).await`; `store.get(id).await`
- **Assert**: Returned entry matches inserted entry; `Ok` result on both
- **Teardown**: `store.close().await`
- **Risk**: Fundamental async write/read path

### SS-I-02: `test_concurrent_reads_do_not_block_each_other` — (R-10)
- **Arrange**: Open store with `read_max_connections: 4`; insert one entry
- **Act**: Spawn 4 concurrent `store.get(id).await` calls
- **Assert**: All 4 complete without `StoreError::PoolTimeout`; all return same entry
- **Teardown**: `store.close().await`
- **Risk**: R-10 (WAL concurrent read capability)

### SS-I-03: `test_no_dirty_read_uncommitted_write` — (R-10)
- **Arrange**: Open store; begin a write transaction on write_pool (do NOT commit)
- **Act**: Concurrently call `store.get(row_id).await` via read_pool
- **Assert**: Read returns the pre-transaction state (reader does not see uncommitted write)
- **Teardown**: Roll back transaction; `store.close().await`
- **Risk**: R-10 (WAL MVCC dirty read isolation)

### SS-I-04: `test_write_pool_saturated_integrity_write_times_out` — (AC-10)
- **Arrange**: `PoolConfig { write_max_connections: 1, write_acquire_timeout: Duration::from_millis(200), .. }`; hold the single write connection in a long transaction (via raw pool access)
- **Act**: Call `store.insert(new_entry).await` from a concurrent task
- **Assert**: Returns `Err(StoreError::PoolTimeout { pool: PoolKind::Write, .. })` within 300ms
- **Assert**: No panic
- **Teardown**: `store.close().await`
- **Risk**: R-01, AC-10

### SS-I-05: `test_integrity_write_not_blocked_by_analytics_queue_full` — (AC-08)
- **Arrange**: Open store; pause drain task; enqueue 1000 events (fill queue)
- **Act**: `store.insert(entry).await` (integrity write — bypasses queue)
- **Assert**: Returns `Ok(entry_id)` without any pool-timeout error
- **Assert**: `store.get(entry_id).await` returns the inserted entry
- **Teardown**: `store.close().await`
- **Risk**: R-06 (AC-08)

### SS-I-06: `test_store_close_pool_connections_return_to_zero` — (AC-19)
- **Arrange**: Open store; perform several reads and writes
- **Act**: `store.close().await`
- **Assert**: All pool connections returned (pools are dropped; SQLite file lock released)
- **Assert**: Subsequent `SqlxStore::open()` on the same file succeeds (would fail if connections were leaked)
- **Risk**: R-02 (AC-19)

### SS-I-07: `test_multiple_stores_same_file_sequential`
- **Arrange**: Open store A, insert entry, close A; open store B on same file
- **Act**: `store_b.get(entry_id).await`
- **Assert**: Returns the entry inserted by store A (persistence)
- **Teardown**: `store_b.close().await`
- **Risk**: R-02 (connection lifecycle correct across open/close cycles)

### SS-I-08: `test_store_open_nonexistent_path_creates_file`
- **Arrange**: Provide a temp path that does not yet exist
- **Act**: `SqlxStore::open(nonexistent_path, PoolConfig::test_default()).await`
- **Assert**: Returns `Ok`; file now exists on disk
- **Teardown**: `store.close().await`
- **Risk**: FR-01 (create_if_missing semantics)

### SS-I-09: `test_pool_starvation_10_concurrent_write_callers` — (R-01)
- **Arrange**: `PoolConfig { write_max_connections: 2, write_acquire_timeout: Duration::from_secs(1), .. }`
- **Act**: Spawn 10 concurrent `store.insert(entry).await` calls, each holding the write connection briefly
- **Assert**: All 10 eventually succeed or receive `StoreError::PoolTimeout`; no panic; no indefinite block
- **Assert**: Callers that time out receive `StoreError::PoolTimeout { pool: PoolKind::Write, .. }`, not a generic error
- **Teardown**: `store.close().await`
- **Risk**: R-01 (pool starvation scenario)

---

## Notes

- SS-I-03 (dirty read test) requires low-level pool access to hold a write transaction open while a concurrent read executes. Use `store.write_pool.begin().await` directly in the test body (allow read access to the inner field in tests via a test accessor or `pub(crate)` on `write_pool`).
- SS-I-05 requires pausing the drain task. A test-only mechanism (e.g., a `#[cfg(test)]` channel that blocks the drain loop) may be needed; the delivery agent must design this.
- TC-02: `store.close().await` is mandatory in every test. Unclosed stores in tests will cause "task panicked after runtime shutdown" in subsequent tests.
- TC-03: No shared store across tests. Every `#[tokio::test]` opens its own store instance.
