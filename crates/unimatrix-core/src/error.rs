use std::fmt;

/// Unified error type for all core trait methods.
///
/// Wraps errors from each foundation crate and async task failures.
#[derive(Debug)]
pub enum CoreError {
    /// Storage engine error (from unimatrix-store).
    Store(unimatrix_store::StoreError),

    /// Vector index error (from unimatrix-vector).
    Vector(unimatrix_vector::VectorError),

    /// Embedding error (from unimatrix-embed).
    Embed(unimatrix_embed::EmbedError),

    /// Async task join failure (from tokio::task::spawn_blocking).
    JoinError(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Store(e) => write!(f, "store error: {e}"),
            CoreError::Vector(e) => write!(f, "vector error: {e}"),
            CoreError::Embed(e) => write!(f, "embed error: {e}"),
            CoreError::JoinError(msg) => write!(f, "async task error: {msg}"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CoreError::Store(e) => Some(e),
            CoreError::Vector(e) => Some(e),
            CoreError::Embed(e) => Some(e),
            CoreError::JoinError(_) => None,
        }
    }
}

impl From<unimatrix_store::StoreError> for CoreError {
    fn from(e: unimatrix_store::StoreError) -> Self {
        CoreError::Store(e)
    }
}

impl From<unimatrix_vector::VectorError> for CoreError {
    fn from(e: unimatrix_vector::VectorError) -> Self {
        CoreError::Vector(e)
    }
}

impl From<unimatrix_embed::EmbedError> for CoreError {
    fn from(e: unimatrix_embed::EmbedError) -> Self {
        CoreError::Embed(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_store_error() {
        let store_err = unimatrix_store::StoreError::EntryNotFound(42);
        let core_err: CoreError = store_err.into();
        assert!(matches!(core_err, CoreError::Store(_)));
    }

    #[test]
    fn test_display_store_error() {
        let err = CoreError::Store(unimatrix_store::StoreError::EntryNotFound(42));
        let msg = format!("{err}");
        assert!(msg.contains("store error"));
        assert!(msg.contains("42"));
    }

    #[test]
    fn test_display_join_error() {
        let err = CoreError::JoinError("task panicked".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("async task error"));
        assert!(msg.contains("task panicked"));
    }

    #[test]
    fn test_error_source_store() {
        let err = CoreError::Store(unimatrix_store::StoreError::EntryNotFound(1));
        assert!(std::error::Error::source(&err).is_some());
    }

    #[test]
    fn test_error_source_join() {
        let err = CoreError::JoinError("msg".to_string());
        assert!(std::error::Error::source(&err).is_none());
    }

    #[test]
    fn test_core_error_is_send_sync() {
        fn _check<T: Send + Sync>() {}
        _check::<CoreError>();
    }
}
