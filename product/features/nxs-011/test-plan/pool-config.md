# Test Plan: PoolConfig (pool_config.rs)

**Component**: `crates/unimatrix-store/src/pool_config.rs`
**Risks**: R-01 (pool starvation), R-11 (PRAGMA per-connection), R-12 (read_only WAL checkpoint)
**ACs**: AC-02, AC-09, AC-10

---

## Unit Tests (`#[tokio::test]` in `unimatrix-store/src/pool_config.rs` or `tests/pool_config_tests.rs`)

### PC-U-01: `test_pool_config_default_values`
- **Inputs**: `PoolConfig::default()`
- **Assert**: `read_max_connections == 8`, `write_max_connections == 2`, `read_acquire_timeout == Duration::from_secs(2)`, `write_acquire_timeout == Duration::from_secs(5)`
- **Risk**: R-01 (correct defaults prevent starvation under default config)

### PC-U-02: `test_pool_config_test_default_values`
- **Inputs**: `PoolConfig::test_default()`
- **Assert**: `read_max_connections <= 8`, `write_max_connections <= 2`, `read_acquire_timeout <= Duration::from_millis(500)`, `write_acquire_timeout <= Duration::from_secs(1)`
- **Risk**: R-02 (shorter timeouts prevent test hangs)

### PC-U-03: `test_pool_config_write_max_boundary_valid`
- **Inputs**: `PoolConfig { write_max_connections: 1, .. }`, `PoolConfig { write_max_connections: 2, .. }`
- **Assert**: `Store::open()` returns `Ok(..)` for both; `Store::close().await` completes without panic
- **Risk**: R-01 (boundary: write_max=1 is valid; write_max=2 is valid)

### PC-U-04: `test_pool_config_write_max_above_cap_rejected` — (AC-09)
- **Inputs**: `PoolConfig { write_max_connections: 3, .. }`
- **Assert**: `Store::open()` returns `Err(StoreError::InvalidPoolConfig { .. })`; error `reason` string mentions "2" and "cap"
- **Assert**: No database connections are opened (verify by checking no DB file is written beyond the path check)
- **Risk**: R-01 (hard cap enforcement)

### PC-U-05: `test_read_pool_acquire_timeout_constant`
- **Inputs**: `READ_POOL_ACQUIRE_TIMEOUT`
- **Assert**: value equals `Duration::from_secs(2)` (per ADR-001)
- **Risk**: R-01

### PC-U-06: `test_write_pool_acquire_timeout_constant`
- **Inputs**: `WRITE_POOL_ACQUIRE_TIMEOUT`
- **Assert**: value equals `Duration::from_secs(5)` (per ADR-001)
- **Risk**: R-01

---

## Integration Tests (`#[tokio::test]` in `unimatrix-store/tests/`)

### PC-I-01: `test_pragma_journal_mode_wal_read_pool` — (AC-02)
- **Arrange**: `SqlxStore::open(temp_db, PoolConfig::test_default()).await`
- **Act**: Execute `PRAGMA journal_mode` via read_pool connection
- **Assert**: Result is `"wal"` (case-insensitive)
- **Teardown**: `store.close().await`
- **Risk**: R-11

### PC-I-02: `test_pragma_journal_mode_wal_write_pool` — (AC-02)
- **Arrange**: `SqlxStore::open(temp_db, PoolConfig::test_default()).await`
- **Act**: Execute `PRAGMA journal_mode` via write_pool connection (use a raw write to exercise that pool)
- **Assert**: Result is `"wal"`
- **Teardown**: `store.close().await`
- **Risk**: R-11

### PC-I-03: `test_pragma_foreign_keys_on` — (AC-02)
- **Arrange**: `SqlxStore::open(temp_db, PoolConfig::test_default()).await`
- **Act**: Execute `PRAGMA foreign_keys` via read_pool
- **Assert**: Result is `1` (ON)
- **Teardown**: `store.close().await`
- **Risk**: R-11

### PC-I-04: `test_pragma_synchronous_normal` — (AC-02)
- **Arrange**: `SqlxStore::open(temp_db, PoolConfig::test_default()).await`
- **Act**: Execute `PRAGMA synchronous` via read_pool
- **Assert**: Result is `1` (NORMAL)
- **Teardown**: `store.close().await`
- **Risk**: R-11

### PC-I-05: `test_write_pool_acquire_timeout_returns_pool_timeout` — (AC-10)
- **Arrange**: Open store with `PoolConfig { write_max_connections: 1, write_acquire_timeout: Duration::from_millis(200), .. }`
- **Act**: Hold the single write connection by beginning a transaction; concurrently call an integrity write method
- **Assert**: Second caller returns `Err(StoreError::PoolTimeout { pool: PoolKind::Write, .. })` within 300ms
- **Assert**: No panic; no indefinite block
- **Teardown**: `store.close().await`
- **Risk**: R-01

### PC-I-06: `test_read_pool_acquire_timeout_returns_pool_timeout` — (AC-10)
- **Arrange**: Open store with `PoolConfig { read_max_connections: 1, read_acquire_timeout: Duration::from_millis(200), .. }`
- **Act**: Hold the single read connection; concurrently call a read method
- **Assert**: Second caller returns `Err(StoreError::PoolTimeout { pool: PoolKind::Read, .. })` within 300ms
- **Teardown**: `store.close().await`
- **Risk**: R-01

### PC-I-07: `test_wal_checkpoint_not_blocked_by_read_pool` — (AC-10, R-12)
- **Arrange**: Open store with `PoolConfig::test_default()`
- **Act**: Insert 1001 entries via write_pool (above `wal_autocheckpoint=1000` threshold)
- **Assert**: WAL file does not grow unboundedly; checkpoint completes (verify WAL size does not exceed 2× the pre-checkpoint size after 1001 writes)
- **Teardown**: `store.close().await`
- **Risk**: R-12

---

## Notes

- PC-I-05 and PC-I-06 are the formal AC-10 tests. They must produce structured `StoreError::PoolTimeout`, not a panic or timeout error from the tokio runtime.
- PC-I-07 is Low priority (R-12) but must be included because the `read_only(true)` defense-in-depth mechanism is being evaluated and may need to be removed.
- All integration tests use `PoolConfig::test_default()` (shorter timeouts) except for tests that specifically test timeout behavior.
- Store::close().await is MANDATORY in every test — TC-02.
