#![forbid(unsafe_code)]

mod adapters;
mod error;
pub mod observation;
mod traits;

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
    DatabaseConfig, EntryRecord, NewEntry, QueryFilter, SqlxStore, SqlxStore as Store, Status,
    StoreError, TimeRange,
};

// Metric types from unimatrix-store (nxs-009 ADR-001)
pub use unimatrix_store::{MetricVector, PhaseMetrics, UniversalMetrics};

// Domain types from unimatrix-vector
pub use unimatrix_vector::{SearchResult, VectorConfig, VectorError, VectorIndex};

// Domain types from unimatrix-embed
pub use unimatrix_embed::{EmbedConfig, EmbedError, EmbeddingProvider, OnnxProvider};
