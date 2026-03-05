//! ObservationSource trait: abstraction over observation data storage.
//!
//! Defined in unimatrix-observe to preserve crate independence (ADR-002).
//! Implemented by SqlObservationSource in unimatrix-server.

use crate::error::Result;
use crate::types::{ObservationRecord, ObservationStats};

/// Abstraction over observation data storage.
///
/// Implementations provide access to persisted observation records and
/// aggregate statistics. The trait is defined here (unimatrix-observe)
/// so that detection rules and pipeline code remain decoupled from
/// storage backends.
pub trait ObservationSource {
    /// Load observation records for a given feature cycle.
    ///
    /// Returns records sorted by timestamp (ascending), containing all
    /// observations from sessions associated with the given feature.
    /// Sessions with NULL feature_cycle are excluded.
    fn load_feature_observations(&self, feature_cycle: &str) -> Result<Vec<ObservationRecord>>;

    /// Discover session IDs associated with a feature cycle.
    ///
    /// Returns session IDs from the sessions table where feature_cycle matches.
    fn discover_sessions_for_feature(&self, feature_cycle: &str) -> Result<Vec<String>>;

    /// Get aggregate observation statistics.
    ///
    /// Returns record count, distinct session count, oldest record age,
    /// and sessions approaching the 60-day cleanup threshold.
    fn observation_stats(&self) -> Result<ObservationStats>;
}
