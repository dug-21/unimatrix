//! Error types for the observation pipeline.

use std::fmt;

/// All errors returned by the observation pipeline.
#[derive(Debug)]
pub enum ObserveError {
    /// I/O error reading observation files.
    Io(std::io::Error),
    /// JSON parsing error.
    Json(serde_json::Error),
    /// Bincode serialization/deserialization error.
    Serialization(String),
    /// Timestamp parsing error.
    TimestampParse(String),
    /// Database query error (col-012: ObservationSource implementations).
    Database(String),
}

impl fmt::Display for ObserveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObserveError::Io(e) => write!(f, "I/O error: {e}"),
            ObserveError::Json(e) => write!(f, "JSON error: {e}"),
            ObserveError::Serialization(msg) => write!(f, "serialization error: {msg}"),
            ObserveError::TimestampParse(msg) => write!(f, "timestamp parse error: {msg}"),
            ObserveError::Database(msg) => write!(f, "database error: {msg}"),
        }
    }
}

impl std::error::Error for ObserveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ObserveError::Io(e) => Some(e),
            ObserveError::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ObserveError {
    fn from(e: std::io::Error) -> Self {
        ObserveError::Io(e)
    }
}

impl From<serde_json::Error> for ObserveError {
    fn from(e: serde_json::Error) -> Self {
        ObserveError::Json(e)
    }
}

impl From<bincode::error::EncodeError> for ObserveError {
    fn from(e: bincode::error::EncodeError) -> Self {
        ObserveError::Serialization(e.to_string())
    }
}

impl From<bincode::error::DecodeError> for ObserveError {
    fn from(e: bincode::error::DecodeError) -> Self {
        ObserveError::Serialization(e.to_string())
    }
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, ObserveError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observe_error_display_io() {
        let err = ObserveError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "gone"));
        let msg = format!("{err}");
        assert!(msg.contains("I/O error"), "expected I/O error in: {msg}");
        assert!(!msg.contains("ObserveError"), "should not leak type: {msg}");
    }

    #[test]
    fn test_observe_error_display_serialization() {
        let err = ObserveError::Serialization("bad bytes".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("bad bytes"), "expected detail in: {msg}");
    }

    #[test]
    fn test_observe_error_display_timestamp() {
        let err = ObserveError::TimestampParse("invalid format".to_string());
        let msg = format!("{err}");
        assert!(
            msg.contains("timestamp"),
            "expected timestamp in: {msg}"
        );
    }

    #[test]
    fn test_observe_error_is_std_error() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<ObserveError>();
    }
}
