use std::fmt;
use std::time::Duration;

/// Identifies which pool caused a `PoolTimeout` error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PoolKind {
    Read,
    Write,
}

impl fmt::Display for PoolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PoolKind::Read => write!(f, "read"),
            PoolKind::Write => write!(f, "write"),
        }
    }
}

/// All errors returned by the storage engine.
#[derive(Debug)]
pub enum StoreError {
    /// Entry with the given ID was not found.
    EntryNotFound(u64),

    /// sqlx database error.
    Database(Box<dyn std::error::Error + Send + Sync>),

    /// Database open failed (pool construction or connection error).
    Open(Box<dyn std::error::Error + Send + Sync>),

    /// Invalid pool configuration (e.g., write_max_connections > 2).
    InvalidPoolConfig { reason: String },

    /// Pool acquire timeout elapsed.
    PoolTimeout { pool: PoolKind, elapsed: Duration },

    /// migrate_if_needed() failed. Pool construction did not proceed.
    Migration {
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Drain task join handle resolved with a panic.
    DrainTaskPanic,

    /// Bincode serialization failed.
    Serialization(String),

    /// Bincode deserialization failed.
    Deserialization(String),

    /// Invalid status byte (not 0, 1, 2, or 3).
    InvalidStatus(u8),

    /// Invalid input for an operation (e.g., correcting a deprecated entry).
    InvalidInput { field: String, reason: String },
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::EntryNotFound(id) => write!(f, "entry not found: {id}"),
            StoreError::Database(e) => write!(f, "database error: {e}"),
            StoreError::Open(e) => write!(f, "database open failed: {e}"),
            StoreError::InvalidPoolConfig { reason } => {
                write!(f, "invalid pool config: {reason}")
            }
            StoreError::PoolTimeout { pool, elapsed } => {
                write!(
                    f,
                    "{pool} pool acquire timeout after {:.3}s",
                    elapsed.as_secs_f64()
                )
            }
            StoreError::Migration { source } => write!(f, "migration failed: {source}"),
            StoreError::DrainTaskPanic => write!(f, "analytics drain task panicked"),
            StoreError::Serialization(msg) => write!(f, "serialization error: {msg}"),
            StoreError::Deserialization(msg) => write!(f, "deserialization error: {msg}"),
            StoreError::InvalidStatus(byte) => write!(f, "invalid status byte: {byte}"),
            StoreError::InvalidInput { field, reason } => {
                write!(f, "invalid input for '{field}': {reason}")
            }
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            StoreError::Database(e) => Some(e.as_ref()),
            StoreError::Open(e) => Some(e.as_ref()),
            StoreError::Migration { source } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::PoolTimedOut => StoreError::PoolTimeout {
                pool: PoolKind::Write,
                elapsed: crate::pool_config::WRITE_POOL_ACQUIRE_TIMEOUT,
            },
            other => StoreError::Database(other.into()),
        }
    }
}

impl From<bincode::error::EncodeError> for StoreError {
    fn from(e: bincode::error::EncodeError) -> Self {
        StoreError::Serialization(e.to_string())
    }
}

impl From<bincode::error::DecodeError> for StoreError {
    fn from(e: bincode::error::DecodeError) -> Self {
        StoreError::Deserialization(e.to_string())
    }
}

/// Convenience type alias for results from the storage engine.
pub type Result<T> = std::result::Result<T, StoreError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_error_display_entry_not_found() {
        let err = StoreError::EntryNotFound(42);
        let msg = err.to_string();
        assert!(msg.contains("42"), "expected '42' in: {msg}");
        assert!(
            msg.contains("entry not found"),
            "expected 'entry not found' in: {msg}"
        );
    }

    #[test]
    fn test_error_display_invalid_status() {
        let err = StoreError::InvalidStatus(99);
        let msg = err.to_string();
        assert!(msg.contains("99"), "expected '99' in: {msg}");
        assert!(
            msg.contains("invalid status byte"),
            "expected 'invalid status byte' in: {msg}"
        );
    }

    #[test]
    fn test_error_display_serialization() {
        let err = StoreError::Serialization("bad data".into());
        let msg = err.to_string();
        assert!(msg.contains("bad data"), "expected 'bad data' in: {msg}");
    }

    #[test]
    fn test_error_display_deserialization() {
        let err = StoreError::Deserialization("corrupt".into());
        let msg = err.to_string();
        assert!(msg.contains("corrupt"), "expected 'corrupt' in: {msg}");
    }

    #[test]
    fn test_error_is_std_error() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<StoreError>();
    }

    #[test]
    fn test_error_source_returns_none_for_app_errors() {
        let err = StoreError::EntryNotFound(1);
        assert!(err.source().is_none());

        let err = StoreError::InvalidStatus(5);
        assert!(err.source().is_none());

        let err = StoreError::Serialization("test".into());
        assert!(err.source().is_none());
    }

    #[test]
    fn test_error_display_invalid_pool_config() {
        let err = StoreError::InvalidPoolConfig {
            reason: "write_max exceeds 2".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("invalid pool config"), "got: {msg}");
        assert!(msg.contains("write_max exceeds 2"), "got: {msg}");
    }

    #[test]
    fn test_error_display_pool_timeout() {
        let err = StoreError::PoolTimeout {
            pool: PoolKind::Write,
            elapsed: Duration::from_secs(5),
        };
        let msg = err.to_string();
        assert!(msg.contains("write"), "got: {msg}");
        assert!(msg.contains("timeout"), "got: {msg}");
    }

    #[test]
    fn test_pool_kind_display() {
        assert_eq!(PoolKind::Read.to_string(), "read");
        assert_eq!(PoolKind::Write.to_string(), "write");
    }

    #[test]
    fn test_error_display_migration() {
        let err = StoreError::Migration {
            source: "migration step failed".into(),
        };
        let msg = err.to_string();
        assert!(msg.contains("migration failed"), "got: {msg}");
    }

    #[test]
    fn test_error_display_drain_task_panic() {
        let err = StoreError::DrainTaskPanic;
        let msg = err.to_string();
        assert!(msg.contains("drain task panicked"), "got: {msg}");
    }
}
