# Component: PoolConfig
## File: `crates/unimatrix-store/src/pool_config.rs` (new file)

---

## Purpose

Defines the `PoolConfig` struct that callers pass to `SqlxStore::open()`. Encodes all pool
sizing and timeout parameters. Provides named public constants for timeout values (ADR-001).
Provides `PoolConfig::default()` for production and `PoolConfig::test_default()` for tests.

This is a pure data struct — no I/O, no async. It is the sole source of truth for pool
parameters across the codebase.

---

## Constants

```rust
// Public constants — referenced in tests and documentation.
pub const READ_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(2);
pub const WRITE_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

// Drain task constants — pub(crate), defined here for proximity to PoolConfig.
// (Alternatively, can be defined in analytics.rs; whichever file is chosen must be the
//  single authoritative location — do not define in both.)
pub const ANALYTICS_QUEUE_CAPACITY: usize = 1000;
pub(crate) const DRAIN_BATCH_SIZE: usize = 50;
pub(crate) const DRAIN_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const DRAIN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
```

Note: The implementation agent may choose to move `ANALYTICS_QUEUE_CAPACITY`,
`DRAIN_BATCH_SIZE`, `DRAIN_FLUSH_INTERVAL`, and `DRAIN_SHUTDOWN_TIMEOUT` into `analytics.rs`
for cohesion. If so, `pool_config.rs` imports them from there. The important constraint is
each constant appears in exactly one place.

---

## Data Structures

```rust
/// Configuration for the SqlxStore dual-pool architecture.
///
/// Pass to `SqlxStore::open()`. Validated at open time; invalid configs return
/// `StoreError::InvalidPoolConfig` before any database connection is opened.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum concurrent read connections. Valid range: 1–8. Recommended: 6–8.
    pub read_max_connections: u32,

    /// Maximum concurrent write connections. Hard cap: must be ≤ 2 (AC-09, NF-01).
    /// Values > 2 are rejected at open time with StoreError::InvalidPoolConfig.
    pub write_max_connections: u32,

    /// Timeout for acquiring a read pool connection (ADR-001).
    /// On timeout: StoreError::PoolTimeout { pool: PoolKind::Read, elapsed }.
    pub read_acquire_timeout: Duration,

    /// Timeout for acquiring a write pool connection (ADR-001).
    /// On timeout: StoreError::PoolTimeout { pool: PoolKind::Write, elapsed }.
    pub write_acquire_timeout: Duration,
}
```

---

## Functions

### `PoolConfig::default`

```rust
impl PoolConfig {
    /// Production-safe defaults per ADR-001.
    /// read_max=8, write_max=2, read_timeout=2s, write_timeout=5s
    pub fn default() -> Self {
        Self {
            read_max_connections: 8,
            write_max_connections: 2,
            read_acquire_timeout: READ_POOL_ACQUIRE_TIMEOUT,   // 2s
            write_acquire_timeout: WRITE_POOL_ACQUIRE_TIMEOUT, // 5s
        }
    }
}
```

### `PoolConfig::test_default`

```rust
    /// Reduced timeouts for test contexts (ADR-001).
    /// Shorter timeouts prevent test suite slowdown when exercising saturation scenarios.
    /// read_max=2, write_max=1, read_timeout=500ms, write_timeout=1s
    pub fn test_default() -> Self {
        Self {
            read_max_connections: 2,
            write_max_connections: 1,
            read_acquire_timeout: Duration::from_millis(500),
            write_acquire_timeout: Duration::from_secs(1),
        }
    }
```

### `PoolConfig::validate`

```rust
    /// Called by SqlxStore::open() before touching any database.
    /// Returns Err(StoreError::InvalidPoolConfig) if any constraint is violated.
    pub(crate) fn validate(&self) -> Result<(), StoreError> {
        if self.write_max_connections > 2 {
            return Err(StoreError::InvalidPoolConfig {
                reason: format!(
                    "write_pool max_connections {} exceeds hard cap of 2 (SQLite WAL writer limit)",
                    self.write_max_connections
                ),
            });
        }
        if self.write_max_connections == 0 {
            return Err(StoreError::InvalidPoolConfig {
                reason: "write_pool max_connections must be at least 1".to_string(),
            });
        }
        if self.read_max_connections == 0 {
            return Err(StoreError::InvalidPoolConfig {
                reason: "read_pool max_connections must be at least 1".to_string(),
            });
        }
        // read_acquire_timeout and write_acquire_timeout: zero-duration is technically
        // valid (immediate fail on any saturation) but unusual. Allow it; tests may use it.
        Ok(())
    }
```

### `build_connect_options` (module-level function, pub(crate))

```rust
/// Constructs SqliteConnectOptions with all 6 required PRAGMAs applied per connection.
/// Used by both pool construction and the migration connection (ADR-003).
/// Every connection in the pool — including lazily created ones — receives these PRAGMAs.
pub(crate) fn build_connect_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL")
        .pragma("wal_autocheckpoint", "1000")
        .pragma("foreign_keys", "ON")
        .pragma("busy_timeout", "5000")    // milliseconds
        .pragma("cache_size", "-16384")   // negative = kibibytes
        .create_if_missing(true)
}

/// Applies the same 6 PRAGMAs to a non-pooled SqliteConnection (migration connection).
/// Uses sqlx::query() because SqliteConnectOptions::pragma() only applies at connection-open.
pub(crate) async fn apply_pragmas_to_connection(
    conn: &mut sqlx::SqliteConnection,
) -> Result<(), sqlx::Error> {
    sqlx::query("PRAGMA journal_mode = WAL").execute(&mut *conn).await?;
    sqlx::query("PRAGMA synchronous = NORMAL").execute(&mut *conn).await?;
    sqlx::query("PRAGMA wal_autocheckpoint = 1000").execute(&mut *conn).await?;
    sqlx::query("PRAGMA foreign_keys = ON").execute(&mut *conn).await?;
    sqlx::query("PRAGMA busy_timeout = 5000").execute(&mut *conn).await?;
    sqlx::query("PRAGMA cache_size = -16384").execute(&mut *conn).await?;
    Ok(())
}
```

Note on `apply_pragmas_to_connection`: When PRAGMAs are applied via
`SqliteConnectOptions::pragma()`, they run as part of connection initialization for every
new connection in the pool. For the non-pooled migration connection, we apply them via
explicit PRAGMA queries after connecting. Both paths must set the same 6 PRAGMAs.

---

## Error Handling

`PoolConfig::validate()` returns `Err(StoreError::InvalidPoolConfig { reason })` only.
No panics. The error propagates up through `SqlxStore::open()` which returns it to the
server startup caller.

---

## Key Test Scenarios

1. **`test_pool_config_default_values`**: Construct `PoolConfig::default()`; assert
   `read_max_connections == 8`, `write_max_connections == 2`,
   `read_acquire_timeout == Duration::from_secs(2)`,
   `write_acquire_timeout == Duration::from_secs(5)`.

2. **`test_pool_config_test_default_values`**: Construct `PoolConfig::test_default()`;
   assert shorter timeouts and reduced connection counts.

3. **`test_pool_config_validate_write_max_3_rejected`**: Construct config with
   `write_max_connections: 3`; assert `validate()` returns
   `Err(StoreError::InvalidPoolConfig)`. (AC-09)

4. **`test_pool_config_validate_write_max_2_accepted`**: `write_max_connections: 2`;
   assert `validate()` returns `Ok(())`.

5. **`test_pool_config_validate_write_max_1_accepted`**: `write_max_connections: 1`;
   assert `validate()` returns `Ok(())`.

6. **`test_pool_config_validate_zero_write_rejected`**: `write_max_connections: 0`;
   assert `validate()` returns `Err(StoreError::InvalidPoolConfig)`.

7. **`test_constants_exported`**: Compile-time check that `READ_POOL_ACQUIRE_TIMEOUT`,
   `WRITE_POOL_ACQUIRE_TIMEOUT`, and `ANALYTICS_QUEUE_CAPACITY` are reachable as
   `unimatrix_store::pool_config::READ_POOL_ACQUIRE_TIMEOUT` (or re-exported from `lib.rs`).

---

## OQ-DURING Items Affecting This Component

None directly. OQ-DURING-02 (drain shutdown timeout configurability) would add a
`drain_shutdown_timeout: Duration` field to this struct if chosen. Current stance:
constant only (`DRAIN_SHUTDOWN_TIMEOUT`).
