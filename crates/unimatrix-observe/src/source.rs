//! ObservationSource trait: abstraction over observation data storage.
//!
//! Defined in unimatrix-observe to preserve crate independence (ADR-002).
//! Implemented by SqlObservationSource in unimatrix-server.

use crate::error::Result;
use crate::types::{ObservationRecord, ObservationStats, ParsedSession};

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

    /// Load sessions with NULL feature_cycle and their observations.
    ///
    /// Returns `ParsedSession` structs (grouped by session_id, sorted by timestamp)
    /// for sessions where `feature_cycle IS NULL`. Used as a fallback for
    /// content-based attribution when the direct feature_cycle query returns empty.
    fn load_unattributed_sessions(&self) -> Result<Vec<ParsedSession>>;

    /// Get aggregate observation statistics.
    ///
    /// Returns record count, distinct session count, oldest record age,
    /// and sessions approaching the 60-day cleanup threshold.
    fn observation_stats(&self) -> Result<ObservationStats>;

    /// Load observation records attributed to a named feature cycle via cycle_events timestamps.
    ///
    /// This is the primary attribution path introduced in col-024. It uses the
    /// cycle_events table (which records cycle_start / cycle_stop events synchronously)
    /// to derive time windows, then discovers sessions by matching the topic_signal column
    /// against the cycle_id within those windows.
    ///
    /// Returns Ok(vec![]) in two cases:
    ///   1. No cycle_events rows exist for cycle_id (pre-col-024 features).
    ///   2. cycle_events rows exist but no observations match topic_signal within windows.
    /// The caller must not treat Ok(vec![]) as an error -- the legacy fallback activates
    /// on this return value (FM-01).
    ///
    /// Returns Err(ObserveError) only on a genuine SQL or database failure.
    ///
    /// Sync contract: implementations must not use async fn. All async work must be
    /// bridged via block_sync inside the implementation body (NFR-01, ADR-001).
    fn load_cycle_observations(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>;
}
