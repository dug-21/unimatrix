use std::fmt;

/// All errors returned by the storage engine.
#[derive(Debug)]
pub enum StoreError {
    /// Entry with the given ID was not found.
    EntryNotFound(u64),

    /// Underlying redb database error.
    #[cfg(not(feature = "backend-sqlite"))]
    Database(redb::DatabaseError),

    /// redb transaction error.
    #[cfg(not(feature = "backend-sqlite"))]
    Transaction(redb::TransactionError),

    /// redb table error.
    #[cfg(not(feature = "backend-sqlite"))]
    Table(redb::TableError),

    /// redb storage error (I/O, corruption).
    #[cfg(not(feature = "backend-sqlite"))]
    Storage(redb::StorageError),

    /// redb commit error.
    #[cfg(not(feature = "backend-sqlite"))]
    Commit(redb::CommitError),

    /// redb compaction error.
    #[cfg(not(feature = "backend-sqlite"))]
    Compaction(redb::CompactionError),

    /// rusqlite error (SQLite backend).
    #[cfg(feature = "backend-sqlite")]
    Sqlite(rusqlite::Error),

    /// Bincode serialization failed.
    Serialization(String),

    /// Bincode deserialization failed.
    Deserialization(String),

    /// Invalid status byte (not 0, 1, or 2).
    InvalidStatus(u8),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StoreError::EntryNotFound(id) => write!(f, "entry not found: {id}"),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Database(e) => write!(f, "database error: {e}"),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Transaction(e) => write!(f, "transaction error: {e}"),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Table(e) => write!(f, "table error: {e}"),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Storage(e) => write!(f, "storage error: {e}"),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Commit(e) => write!(f, "commit error: {e}"),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Compaction(e) => write!(f, "compaction error: {e}"),
            #[cfg(feature = "backend-sqlite")]
            StoreError::Sqlite(e) => write!(f, "sqlite error: {e}"),
            StoreError::Serialization(msg) => write!(f, "serialization error: {msg}"),
            StoreError::Deserialization(msg) => write!(f, "deserialization error: {msg}"),
            StoreError::InvalidStatus(byte) => write!(f, "invalid status byte: {byte}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Database(e) => Some(e),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Transaction(e) => Some(e),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Table(e) => Some(e),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Storage(e) => Some(e),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Commit(e) => Some(e),
            #[cfg(not(feature = "backend-sqlite"))]
            StoreError::Compaction(e) => Some(e),
            #[cfg(feature = "backend-sqlite")]
            StoreError::Sqlite(e) => Some(e),
            _ => None,
        }
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::DatabaseError> for StoreError {
    fn from(e: redb::DatabaseError) -> Self {
        StoreError::Database(e)
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::TransactionError> for StoreError {
    fn from(e: redb::TransactionError) -> Self {
        StoreError::Transaction(e)
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::TableError> for StoreError {
    fn from(e: redb::TableError) -> Self {
        StoreError::Table(e)
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::StorageError> for StoreError {
    fn from(e: redb::StorageError) -> Self {
        StoreError::Storage(e)
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::CommitError> for StoreError {
    fn from(e: redb::CommitError) -> Self {
        StoreError::Commit(e)
    }
}

#[cfg(not(feature = "backend-sqlite"))]
impl From<redb::CompactionError> for StoreError {
    fn from(e: redb::CompactionError) -> Self {
        StoreError::Compaction(e)
    }
}

#[cfg(feature = "backend-sqlite")]
impl From<rusqlite::Error> for StoreError {
    fn from(e: rusqlite::Error) -> Self {
        StoreError::Sqlite(e)
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
        assert!(msg.contains("entry not found"), "expected 'entry not found' in: {msg}");
    }

    #[test]
    fn test_error_display_invalid_status() {
        let err = StoreError::InvalidStatus(99);
        let msg = err.to_string();
        assert!(msg.contains("99"), "expected '99' in: {msg}");
        assert!(msg.contains("invalid status byte"), "expected 'invalid status byte' in: {msg}");
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
}
