# Pseudocode: observation-source

## NEW File: crates/unimatrix-observe/src/source.rs

### ObservationSource trait definition

```rust
use crate::types::{ObservationRecord, ObservationStats};
use crate::error::Result;

/// Abstraction over observation data storage.
///
/// Defined in unimatrix-observe (ADR-002) to preserve crate independence.
/// Implemented by SqlObservationSource in unimatrix-server.
pub trait ObservationSource {
    /// Load observation records for a feature cycle.
    /// Returns records sorted by timestamp (ascending).
    fn load_feature_observations(&self, feature_cycle: &str) -> Result<Vec<ObservationRecord>>;

    /// Discover session IDs associated with a feature cycle.
    fn discover_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>>;

    /// Get aggregate observation statistics.
    fn observation_stats(&self) -> Result<ObservationStats>;
}
```

## File: crates/unimatrix-observe/src/lib.rs

### Change: Add source module and export

```
pub mod source;
// Add to re-exports:
pub use source::ObservationSource;
```

## File: crates/unimatrix-observe/src/types.rs

### Change: Revise ObservationStats

```rust
pub struct ObservationStats {
    /// Number of observation records (was: file_count).
    pub record_count: u64,
    /// Number of distinct sessions with observations.
    pub session_count: u64,
    /// Age of oldest observation record in days (was: oldest_file_age_days).
    pub oldest_record_age_days: u64,
    /// Session IDs with records approaching 60-day cleanup (45-59 days old).
    pub approaching_cleanup: Vec<String>,
}
```

Remove `total_size_bytes` field (not meaningful for table rows).

## Notes

- ADR-002: Trait in observe, impl in server. No unimatrix-store dependency in observe.
- FR-03.5: No dependency on unimatrix-store
- The trait uses observe's own error::Result, not store's
- ObservationStats field rename requires updating all consumers (status.rs, status response)
