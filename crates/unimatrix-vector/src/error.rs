use std::fmt;

/// All errors returned by the vector index.
#[derive(Debug)]
pub enum VectorError {
    /// Embedding or query dimension does not match the configured dimension.
    DimensionMismatch { expected: usize, got: usize },

    /// Error propagated from the underlying storage engine.
    Store(unimatrix_store::StoreError),

    /// Error during index persistence (dump/load).
    Persistence(String),

    /// Index is empty (reserved for operations requiring a non-empty index).
    EmptyIndex,

    /// Entry ID has no vector mapping in the index.
    EntryNotInIndex(u64),

    /// hnsw_rs internal error.
    Index(String),

    /// Embedding or query contains invalid float values (NaN or infinity).
    InvalidEmbedding(String),
}

impl fmt::Display for VectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VectorError::DimensionMismatch { expected, got } => {
                write!(f, "dimension mismatch: expected {expected}, got {got}")
            }
            VectorError::Store(e) => write!(f, "store error: {e}"),
            VectorError::Persistence(msg) => write!(f, "persistence error: {msg}"),
            VectorError::EmptyIndex => write!(f, "index is empty"),
            VectorError::EntryNotInIndex(id) => {
                write!(f, "entry {id} not found in index")
            }
            VectorError::Index(msg) => write!(f, "index error: {msg}"),
            VectorError::InvalidEmbedding(msg) => {
                write!(f, "invalid embedding: {msg}")
            }
        }
    }
}

impl std::error::Error for VectorError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            VectorError::Store(e) => Some(e),
            _ => None,
        }
    }
}

impl From<unimatrix_store::StoreError> for VectorError {
    fn from(e: unimatrix_store::StoreError) -> Self {
        VectorError::Store(e)
    }
}

/// Convenience type alias for results from the vector index.
pub type Result<T> = std::result::Result<T, VectorError>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_dimension_mismatch_display() {
        let err = VectorError::DimensionMismatch {
            expected: 384,
            got: 128,
        };
        let msg = err.to_string();
        assert!(msg.contains("384"), "expected '384' in: {msg}");
        assert!(msg.contains("128"), "expected '128' in: {msg}");
        assert!(
            msg.contains("dimension mismatch"),
            "expected 'dimension mismatch' in: {msg}"
        );
    }

    #[test]
    fn test_store_error_display() {
        let store_err = unimatrix_store::StoreError::EntryNotFound(42);
        let err = VectorError::Store(store_err);
        let msg = err.to_string();
        assert!(
            msg.contains("store error"),
            "expected 'store error' in: {msg}"
        );
    }

    #[test]
    fn test_persistence_error_display() {
        let err = VectorError::Persistence("file not found: /tmp/test".into());
        let msg = err.to_string();
        assert!(
            msg.contains("persistence error"),
            "expected 'persistence error' in: {msg}"
        );
        assert!(
            msg.contains("file not found"),
            "expected 'file not found' in: {msg}"
        );
    }

    #[test]
    fn test_empty_index_display() {
        let err = VectorError::EmptyIndex;
        let msg = err.to_string();
        assert!(msg.contains("empty"), "expected 'empty' in: {msg}");
    }

    #[test]
    fn test_entry_not_in_index_display() {
        let err = VectorError::EntryNotInIndex(42);
        let msg = err.to_string();
        assert!(msg.contains("42"), "expected '42' in: {msg}");
    }

    #[test]
    fn test_index_error_display() {
        let err = VectorError::Index("hnsw internal error".into());
        let msg = err.to_string();
        assert!(
            msg.contains("index error"),
            "expected 'index error' in: {msg}"
        );
    }

    #[test]
    fn test_invalid_embedding_display() {
        let err = VectorError::InvalidEmbedding("NaN at index 5".into());
        let msg = err.to_string();
        assert!(
            msg.contains("invalid embedding"),
            "expected 'invalid embedding' in: {msg}"
        );
        assert!(msg.contains("NaN"), "expected 'NaN' in: {msg}");
    }

    #[test]
    fn test_from_store_error() {
        let store_err = unimatrix_store::StoreError::EntryNotFound(1);
        let err: VectorError = store_err.into();
        assert!(matches!(err, VectorError::Store(_)));
    }

    #[test]
    fn test_is_std_error() {
        fn assert_error<T: std::error::Error>() {}
        assert_error::<VectorError>();
    }

    #[test]
    fn test_error_source_store_variant() {
        let store_err = unimatrix_store::StoreError::EntryNotFound(1);
        let err = VectorError::Store(store_err);
        assert!(err.source().is_some());
    }

    #[test]
    fn test_error_source_other_variants() {
        assert!(VectorError::EmptyIndex.source().is_none());
        assert!(VectorError::Persistence("x".into()).source().is_none());
        assert!(
            VectorError::DimensionMismatch {
                expected: 1,
                got: 2
            }
            .source()
            .is_none()
        );
        assert!(VectorError::InvalidEmbedding("x".into()).source().is_none());
        assert!(VectorError::Index("x".into()).source().is_none());
        assert!(VectorError::EntryNotInIndex(1).source().is_none());
    }
}
