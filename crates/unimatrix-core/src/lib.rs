#![forbid(unsafe_code)]

mod error;
mod traits;
mod adapters;
pub mod observation;

#[cfg(feature = "async")]
pub mod async_wrappers;

// Error
pub use error::CoreError;

// Traits
pub use traits::{EmbedService, EntryStore, VectorStore};

// Adapters
pub use adapters::{EmbedAdapter, StoreAdapter, VectorAdapter};

// Observation types (col-013 ADR-002: moved from unimatrix-observe)
pub use observation::{HookType, ObservationRecord, ObservationStats, ParsedSession};

// Domain types from unimatrix-store
pub use unimatrix_store::{
    DatabaseConfig, EntryRecord, NewEntry, QueryFilter, Status, Store, StoreError, TimeRange,
};

// Domain types from unimatrix-vector
pub use unimatrix_vector::{SearchResult, VectorConfig, VectorError, VectorIndex};

// Domain types from unimatrix-embed
pub use unimatrix_embed::{EmbedConfig, EmbedError, EmbeddingProvider, OnnxProvider};
