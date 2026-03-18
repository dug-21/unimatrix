use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteConnection};

use crate::error::StoreError;

// ---------------------------------------------------------------------------
// Public timeout constants (ADR-001)
// ---------------------------------------------------------------------------

/// Acquire timeout for the read connection pool (ADR-001).
/// After this duration without obtaining a connection, callers receive
/// `StoreError::PoolTimeout { pool: PoolKind::Read, .. }`.
pub const READ_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(2);

/// Acquire timeout for the write connection pool (ADR-001).
/// After this duration without obtaining a connection, callers receive
/// `StoreError::PoolTimeout { pool: PoolKind::Write, .. }`.
pub const WRITE_POOL_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

/// Bounded channel capacity for the analytics write queue.
/// Events beyond this capacity are shed and counted in `shed_events_total`.
pub const ANALYTICS_QUEUE_CAPACITY: usize = 1000;

// ---------------------------------------------------------------------------
// Drain task constants (pub(crate) — used by analytics.rs)
// ---------------------------------------------------------------------------

/// Maximum number of events committed in a single drain task transaction.
pub(crate) const DRAIN_BATCH_SIZE: usize = 50;

/// Maximum time the drain task waits for a partial batch to fill before committing.
pub(crate) const DRAIN_FLUSH_INTERVAL: Duration = Duration::from_millis(500);

/// Grace period for the drain task to commit remaining events during `Store::close()`.
pub(crate) const DRAIN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

// ---------------------------------------------------------------------------
// PoolConfig struct
// ---------------------------------------------------------------------------

/// Configuration for the SqlxStore dual-pool architecture.
///
/// Pass to `SqlxStore::open()`. Validated at open time; invalid configs return
/// `StoreError::InvalidPoolConfig` before any database connection is opened.
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Maximum concurrent read connections. Valid range: 1–8. Recommended: 6–8.
    pub read_max_connections: u32,

    /// Maximum concurrent write connections. Hard cap: must be ≤ 2 (AC-09, NF-01).
    /// Values > 2 are rejected at open time with `StoreError::InvalidPoolConfig`.
    /// Default is 1 to serialize all writes and avoid SQLITE_BUSY_SNAPSHOT under
    /// concurrent WAL transactions (fire-and-forget usage recording pattern).
    pub write_max_connections: u32,

    /// Timeout for acquiring a read pool connection (ADR-001).
    /// On timeout: `StoreError::PoolTimeout { pool: PoolKind::Read, elapsed }`.
    pub read_acquire_timeout: Duration,

    /// Timeout for acquiring a write pool connection (ADR-001).
    /// On timeout: `StoreError::PoolTimeout { pool: PoolKind::Write, elapsed }`.
    pub write_acquire_timeout: Duration,
}

impl Default for PoolConfig {
    /// Production-safe defaults per ADR-001.
    ///
    /// `read_max=8`, `write_max=1`, `read_timeout=2s`, `write_timeout=5s`.
    /// write_max=1 serializes all writes, preventing SQLITE_BUSY_SNAPSHOT conflicts
    /// from concurrent deferred WAL transactions (fire-and-forget usage recording).
    fn default() -> Self {
        Self {
            read_max_connections: 8,
            write_max_connections: 1,
            read_acquire_timeout: READ_POOL_ACQUIRE_TIMEOUT,
            write_acquire_timeout: WRITE_POOL_ACQUIRE_TIMEOUT,
        }
    }
}

impl PoolConfig {
    /// Reduced timeouts and connection counts for test contexts (ADR-001).
    ///
    /// Shorter timeouts prevent test suite slowdown when exercising saturation
    /// scenarios. `read_max=2`, `write_max=1`, `read_timeout=500ms`, `write_timeout=1s`.
    pub fn test_default() -> Self {
        Self {
            read_max_connections: 2,
            write_max_connections: 1,
            read_acquire_timeout: Duration::from_millis(500),
            write_acquire_timeout: Duration::from_secs(1),
        }
    }

    /// Validates this config before any database connection is opened.
    ///
    /// Returns `Err` if any constraint is violated:
    /// - `write_max_connections > 2` (SQLite WAL writer cap, AC-09)
    /// - `write_max_connections == 0` (pool would be unusable)
    /// - `read_max_connections == 0` (pool would be unusable)
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
        // Zero-duration timeouts are technically valid (immediate fail on any
        // saturation). Allowed — tests may use them for controlled failure scenarios.
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// PRAGMA helpers (pub(crate) — used by db.rs and migration.rs)
// ---------------------------------------------------------------------------

/// Constructs `SqliteConnectOptions` with all 6 required PRAGMAs applied per connection.
///
/// Used for both pool construction and the non-pooled migration connection (ADR-003).
/// Every connection in the pool — including lazily-created ones — receives these PRAGMAs
/// because they are applied via `SqliteConnectOptions::pragma()` at connection-open time.
pub(crate) fn build_connect_options(path: &Path) -> SqliteConnectOptions {
    // `.busy_timeout()` calls `sqlite3_busy_timeout()` at the C-API level at
    // connection-open time — before any PRAGMA executes. The PRAGMA below is
    // belt-and-suspenders for any connection path that doesn't go through here.
    SqliteConnectOptions::new()
        .filename(path)
        .busy_timeout(Duration::from_secs(10))
        .pragma("journal_mode", "WAL")
        .pragma("synchronous", "NORMAL")
        .pragma("wal_autocheckpoint", "1000")
        .pragma("foreign_keys", "ON")
        .pragma("busy_timeout", "10000") // milliseconds — belt-and-suspenders
        .pragma("cache_size", "-16384") // negative = kibibytes
        .create_if_missing(true)
}

/// Applies the same 6 PRAGMAs to an already-open `SqliteConnection`.
///
/// Used for the non-pooled migration connection (ADR-003). `SqliteConnectOptions::pragma()`
/// only applies at connection-open; for an existing connection, explicit PRAGMA queries
/// are required.
pub(crate) async fn apply_pragmas_to_connection(
    conn: &mut SqliteConnection,
) -> Result<(), sqlx::Error> {
    use sqlx::Executor;
    conn.execute("PRAGMA journal_mode = WAL").await?;
    conn.execute("PRAGMA synchronous = NORMAL").await?;
    conn.execute("PRAGMA wal_autocheckpoint = 1000").await?;
    conn.execute("PRAGMA foreign_keys = ON").await?;
    conn.execute("PRAGMA busy_timeout = 10000").await?;
    conn.execute("PRAGMA cache_size = -16384").await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pool_config_default_values() {
        let cfg = PoolConfig::default();
        assert_eq!(cfg.read_max_connections, 8);
        assert_eq!(cfg.write_max_connections, 1);
        assert_eq!(cfg.read_acquire_timeout, Duration::from_secs(2));
        assert_eq!(cfg.write_acquire_timeout, Duration::from_secs(5));
    }

    #[test]
    fn test_pool_config_test_default_values() {
        let cfg = PoolConfig::test_default();
        assert!(cfg.read_max_connections <= 8);
        assert!(cfg.write_max_connections <= 2);
        assert!(cfg.read_acquire_timeout <= Duration::from_millis(500));
        assert!(cfg.write_acquire_timeout <= Duration::from_secs(1));
    }

    #[test]
    fn test_pool_config_validate_write_max_3_rejected() {
        let cfg = PoolConfig {
            read_max_connections: 4,
            write_max_connections: 3,
            read_acquire_timeout: Duration::from_secs(1),
            write_acquire_timeout: Duration::from_secs(2),
        };
        let result = cfg.validate();
        assert!(result.is_err(), "expected Err for write_max=3");
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("2"), "expected '2' cap in error: {msg}");
    }

    #[test]
    fn test_pool_config_validate_write_max_2_accepted() {
        let cfg = PoolConfig {
            read_max_connections: 4,
            write_max_connections: 2,
            read_acquire_timeout: Duration::from_secs(1),
            write_acquire_timeout: Duration::from_secs(2),
        };
        assert!(cfg.validate().is_ok(), "expected Ok for write_max=2");
    }

    #[test]
    fn test_pool_config_validate_write_max_1_accepted() {
        let cfg = PoolConfig {
            read_max_connections: 4,
            write_max_connections: 1,
            read_acquire_timeout: Duration::from_secs(1),
            write_acquire_timeout: Duration::from_secs(2),
        };
        assert!(cfg.validate().is_ok(), "expected Ok for write_max=1");
    }

    #[test]
    fn test_pool_config_validate_zero_write_rejected() {
        let cfg = PoolConfig {
            read_max_connections: 4,
            write_max_connections: 0,
            read_acquire_timeout: Duration::from_secs(1),
            write_acquire_timeout: Duration::from_secs(2),
        };
        let result = cfg.validate();
        assert!(result.is_err(), "expected Err for write_max=0");
    }

    #[test]
    fn test_pool_config_validate_zero_read_rejected() {
        let cfg = PoolConfig {
            read_max_connections: 0,
            write_max_connections: 1,
            read_acquire_timeout: Duration::from_secs(1),
            write_acquire_timeout: Duration::from_secs(2),
        };
        let result = cfg.validate();
        assert!(result.is_err(), "expected Err for read_max=0");
    }

    #[test]
    fn test_read_pool_acquire_timeout_constant() {
        assert_eq!(READ_POOL_ACQUIRE_TIMEOUT, Duration::from_secs(2));
    }

    #[test]
    fn test_write_pool_acquire_timeout_constant() {
        assert_eq!(WRITE_POOL_ACQUIRE_TIMEOUT, Duration::from_secs(5));
    }

    #[test]
    fn test_analytics_queue_capacity_constant() {
        assert_eq!(ANALYTICS_QUEUE_CAPACITY, 1000);
    }

    #[test]
    fn test_drain_batch_size_constant() {
        assert_eq!(DRAIN_BATCH_SIZE, 50);
    }

    #[test]
    fn test_drain_flush_interval_constant() {
        assert_eq!(DRAIN_FLUSH_INTERVAL, Duration::from_millis(500));
    }

    #[test]
    fn test_drain_shutdown_timeout_constant() {
        assert_eq!(DRAIN_SHUTDOWN_TIMEOUT, Duration::from_secs(5));
    }

    /// Compile-time reachability check — asserts all public constants are accessible
    /// via the module path.
    #[test]
    fn test_constants_exported() {
        let _r = crate::pool_config::READ_POOL_ACQUIRE_TIMEOUT;
        let _w = crate::pool_config::WRITE_POOL_ACQUIRE_TIMEOUT;
        let _c = crate::pool_config::ANALYTICS_QUEUE_CAPACITY;
    }
}
