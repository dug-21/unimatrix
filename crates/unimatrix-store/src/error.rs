use std::fmt;

/// All errors returned by the storage engine.
#[derive(Debug)]
pub enum StoreError {
    /// Entry with the given ID was not found.
    EntryNotFound(u64),

    /// rusqlite error (SQLite backend).
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
            StoreError::Sqlite(e) => Some(e),
            _ => None,
        }
    }
}

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
