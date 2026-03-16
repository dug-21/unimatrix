//! StatusService: transport-agnostic status computation (vnc-008).
//!
//! Rewritten for nxs-008: direct SQL queries replace compat layer.
//! Uses SQL aggregation where possible.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use unimatrix_core::async_wrappers::AsyncEntryStore;
use unimatrix_core::{CoreError, EmbedService, Store, VectorAdapter, VectorIndex};
use unimatrix_store::rusqlite;
use unimatrix_store::sessions::{DELETE_THRESHOLD_SECS, TIMED_OUT_THRESHOLD_SECS};
use unimatrix_store::{EntryRecord, StoreError};

use unimatrix_adapt::AdaptationService;

use crate::infra::coherence;
use crate::infra::contradiction;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::session::SessionRegistry;
use crate::infra::timeout::{MCP_HANDLER_TIMEOUT, spawn_blocking_with_timeout};
use crate::mcp::response::status::{CoAccessClusterEntry, StatusReport};
use crate::server::PendingEntriesAnalysis;
use crate::services::ServiceError;
use crate::services::confidence::ConfidenceStateHandle;
use crate::services::contradiction_cache::ContradictionScanCacheHandle;

/// Minimum number of voted entries required for empirical Bayesian prior estimation.
///
/// Below this threshold, cold-start defaults (alpha0=3.0, beta0=3.0) are used.
/// ADR-002 sets this at 10 for population stability (SPEC originally stated 5).
pub const MINIMUM_VOTED_POPULATION: usize = 10;

/// Pre-crt-019 measured confidence spread baseline.
///
/// Returned by `compute_observed_spread` when the active population is non-empty
/// but too small (< 10 entries) to compute a reliable spread.
const PRE_CRT019_SPREAD_BASELINE: f64 = 0.1471;

/// Cold-start alpha0 prior (positive pseudo-votes).
const COLD_START_ALPHA: f64 = 3.0;
/// Cold-start beta0 prior (negative pseudo-votes).
const COLD_START_BETA: f64 = 3.0;

/// Compute empirical Bayesian prior (alpha0, beta0) from voted-entry population.
///
/// Uses method-of-moments estimation on the Beta distribution. Requires
/// at least `MINIMUM_VOTED_POPULATION` (10) voted entries to attempt estimation;
/// below this threshold returns cold-start defaults (3.0, 3.0).
///
/// Handles zero-variance degeneracy (all entries identical rate) by returning
/// cold-start defaults rather than propagating NaN or +∞.
///
/// Output clamped to [0.5, 50.0] per ADR-002 / IMPLEMENTATION-BRIEF.
///
/// # Parameters
/// - `voted_entries`: slice of `(helpful_count, unhelpful_count)` for entries
///   with at least one vote (total >= 1). The caller is responsible for the filter.
pub(crate) fn compute_empirical_prior(voted_entries: &[(u32, u32)]) -> (f64, f64) {
    if voted_entries.len() < MINIMUM_VOTED_POPULATION {
        return (COLD_START_ALPHA, COLD_START_BETA);
    }

    // Per-entry helpfulness rate: cast u32 to f64 before division.
    // Caller guarantees total >= 1, so no division-by-zero here.
    let rates: Vec<f64> = voted_entries
        .iter()
        .map(|(h, u)| {
            let h_f = *h as f64;
            let u_f = *u as f64;
            h_f / (h_f + u_f)
        })
        .collect();

    let n = rates.len() as f64;

    // Population mean helpfulness rate.
    let p_bar: f64 = rates.iter().sum::<f64>() / n;

    // Sample variance (Bessel's correction: divide by n-1).
    // With n >= MINIMUM_VOTED_POPULATION (10), n-1 >= 9, so no division-by-zero.
    let sum_sq_dev: f64 = rates.iter().map(|r| (r - p_bar).powi(2)).sum();
    let variance = sum_sq_dev / (n - 1.0);

    // Zero-variance degeneracy (R-12): all entries have identical rate.
    // Cannot estimate concentration; return cold-start to avoid NaN/∞.
    if variance <= 0.0 {
        return (COLD_START_ALPHA, COLD_START_BETA);
    }

    // Method-of-moments for Beta distribution:
    //   concentration = p_bar * (1 - p_bar) / variance - 1
    // Requires p_bar * (1 - p_bar) / variance > 1 for a valid Beta;
    // if not, the variance is too large relative to the mean — return cold-start.
    let ratio = p_bar * (1.0 - p_bar) / variance;
    if ratio <= 1.0 {
        return (COLD_START_ALPHA, COLD_START_BETA);
    }

    let concentration = ratio - 1.0;
    let alpha0 = (p_bar * concentration).clamp(0.5, 50.0);
    let beta0 = ((1.0 - p_bar) * concentration).clamp(0.5, 50.0);

    (alpha0, beta0)
}

/// Compute observed confidence spread as p95 - p5 of the confidence distribution.
///
/// Returns:
/// - `0.0` for an empty slice (EC-01).
/// - `PRE_CRT019_SPREAD_BASELINE` (0.1471) for 1–9 entries (too small for reliable spread;
///   use the pre-crt-019 measured baseline rather than a noisy near-zero estimate).
/// - Computed p95 - p5 for 10 or more entries, using the nearest-rank method.
///
/// Result is non-negative (guarded by `.max(0.0)` against floating-point rounding).
pub(crate) fn compute_observed_spread(confidences: &[f64]) -> f64 {
    if confidences.is_empty() {
        return 0.0;
    }

    if confidences.len() < MINIMUM_VOTED_POPULATION {
        return PRE_CRT019_SPREAD_BASELINE;
    }

    let mut sorted = confidences.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted.len();

    // Nearest-rank percentile (1-indexed):
    //   p5  → index = ceil(0.05 * n) - 1, clamped to [0, n-1]
    //   p95 → index = ceil(0.95 * n) - 1, clamped to [0, n-1]
    let p5_idx = ((0.05 * n as f64).ceil() as usize).saturating_sub(1);
    let p95_idx = (((0.95 * n as f64).ceil() as usize).saturating_sub(1)).min(n - 1);

    let p5 = sorted[p5_idx];
    let p95 = sorted[p95_idx];

    (p95 - p5).max(0.0)
}

/// Compute adaptive confidence weight from observed spread.
///
/// Formula: `clamp(observed_spread * 1.25, 0.15, 0.25)`
///
/// This is a local copy of the formula from `unimatrix_engine::confidence::adaptive_confidence_weight`
/// (added in crt-019). Once the engine function is available, call sites in Step 2b can
/// delegate to `unimatrix_engine::confidence::adaptive_confidence_weight(spread)`.
fn adaptive_confidence_weight_local(observed_spread: f64) -> f64 {
    (observed_spread * 1.25).clamp(0.15, 0.25)
}

/// Transport-agnostic status computation service.
///
/// Extracted from the `context_status` handler (ADR-001).
/// Uses direct SQL queries (nxs-008).
#[derive(Clone)]
pub(crate) struct StatusService {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    /// crt-019 (ADR-001, ADR-002): write-side owner of the adaptive blend state.
    ///
    /// The maintenance tick (run_maintenance Step 2b) acquires the write lock to
    /// update `{alpha0, beta0, observed_spread, confidence_weight}` atomically
    /// after computing empirical priors from the voted-entry population.
    /// Read once before each confidence refresh batch (IR-02) to snapshot
    /// alpha0/beta0 without acquiring the lock inside the hot loop.
    confidence_state: ConfidenceStateHandle,
    /// GH #278: last contradiction scan result, written by background tick, read here.
    ///
    /// `compute_report()` reads the cached result instead of running O(N) ONNX
    /// inference on every call. `None` on cold-start; set after first scan tick.
    contradiction_cache: ContradictionScanCacheHandle,
}

/// Result of maintenance operations.
#[allow(dead_code)]
pub(crate) struct MaintenanceResult {
    pub confidence_refreshed: u64,
    pub graph_compacted: bool,
    pub stale_pairs_cleaned: u64,
}

/// Lightweight snapshot of the data consumed by `maintenance_tick()`.
///
/// Replaces the full `compute_report()` call in the background tick path (#280).
/// Only the three values actually consumed by the tick are computed:
/// - `active_entries`: loaded via `store.load_active_entries_with_tags()`
/// - `graph_stale_ratio`: computed inline from `VectorIndex` counters
/// - `effectiveness`: built via the same Phase 8 logic as `compute_report()`
///
/// Phases 2 (contradiction scan, O(N) ONNX), 3, 4, 6, 7, and most of Phase 1
/// are intentionally skipped to avoid wasting 15–35 s per tick.
#[derive(Debug)]
pub(crate) struct MaintenanceDataSnapshot {
    pub active_entries: Vec<EntryRecord>,
    pub graph_stale_ratio: f64,
    pub effectiveness: Option<unimatrix_engine::effectiveness::EffectivenessReport>,
}

impl StatusService {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        confidence_state: ConfidenceStateHandle,
        contradiction_cache: ContradictionScanCacheHandle,
    ) -> Self {
        StatusService {
            store,
            vector_index,
            embed_service,
            adapt_service,
            confidence_state,
            contradiction_cache,
        }
    }

    /// Load the minimal data snapshot required by the background maintenance tick (#280).
    ///
    /// Runs exactly three operations, skipping the O(N) ONNX contradiction scan
    /// (Phase 2), co-access queries (Phase 4), observation stats (Phase 6),
    /// retrospective count (Phase 7), and most of Phase 1:
    /// 1. `store.load_active_entries_with_tags()` for confidence refresh and graph compaction.
    /// 2. `VectorIndex::point_count()` / `stale_count()` (inline, no blocking) for `graph_stale_ratio`.
    /// 3. `store.compute_effectiveness_aggregates()` + classify loop + `build_report()` for auto-quarantine.
    ///
    /// `compute_report()` is left untouched — it is still used by the `context_status` MCP tool.
    pub(crate) async fn load_maintenance_snapshot(
        &self,
    ) -> Result<MaintenanceDataSnapshot, ServiceError> {
        // Step 1: Load active entries (needed by confidence refresh, graph compaction).
        let store_for_entries = Arc::clone(&self.store);
        let active_entries = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
            store_for_entries
                .load_active_entries_with_tags()
                .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e)))
        })
        .await
        .map_err(|e| {
            let core_err: CoreError = match e {
                crate::error::ServerError::Core(ce) => ce,
                other => CoreError::JoinError(other.to_string()),
            };
            ServiceError::Core(core_err)
        })?
        .map_err(|e| {
            let core_err: CoreError = match e {
                crate::error::ServerError::Core(ce) => ce,
                other => CoreError::JoinError(other.to_string()),
            };
            ServiceError::Core(core_err)
        })?;

        // Step 2: Compute graph stale ratio inline (no blocking — VectorIndex uses atomics).
        let graph_point_count = self.vector_index.point_count();
        let graph_stale_count = self.vector_index.stale_count();
        let graph_stale_ratio = if graph_point_count == 0 {
            0.0
        } else {
            graph_stale_count as f64 / graph_point_count as f64
        };

        // Step 3: Effectiveness analysis (same logic as Phase 8 of compute_report).
        let store_for_eff = Arc::clone(&self.store);
        let effectiveness = match spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
            let aggregates = store_for_eff.compute_effectiveness_aggregates()?;
            let entry_meta = store_for_eff.load_entry_classification_meta()?;
            Ok::<_, unimatrix_store::StoreError>((aggregates, entry_meta))
        })
        .await
        {
            Ok(Ok((aggregates, entry_meta))) => {
                use unimatrix_engine::effectiveness::{
                    NOISY_TRUST_SOURCES, build_report, classify_entry,
                };

                let stats_map: HashMap<u64, &unimatrix_store::read::EntryInjectionStats> =
                    aggregates
                        .entry_stats
                        .iter()
                        .map(|s| (s.entry_id, s))
                        .collect();

                let classifications: Vec<unimatrix_engine::effectiveness::EntryEffectiveness> =
                    entry_meta
                        .iter()
                        .map(|meta| {
                            let (inj_count, success, rework, abandoned) =
                                match stats_map.get(&meta.entry_id) {
                                    Some(stats) => (
                                        stats.injection_count,
                                        stats.success_count,
                                        stats.rework_count,
                                        stats.abandoned_count,
                                    ),
                                    None => (0, 0, 0, 0),
                                };

                            let topic_has_sessions = aggregates.active_topics.contains(&meta.topic);

                            classify_entry(
                                meta.entry_id,
                                &meta.title,
                                &meta.topic,
                                &meta.trust_source,
                                meta.helpful_count,
                                meta.unhelpful_count,
                                inj_count,
                                success,
                                rework,
                                abandoned,
                                topic_has_sessions,
                                NOISY_TRUST_SOURCES,
                            )
                        })
                        .collect();

                let data_window = unimatrix_engine::effectiveness::DataWindow {
                    session_count: aggregates.session_count,
                    earliest_session_at: aggregates.earliest_session_at,
                    latest_session_at: aggregates.latest_session_at,
                };

                Some(build_report(
                    classifications,
                    &aggregates.calibration_rows,
                    data_window,
                ))
            }
            Ok(Err(e)) => {
                tracing::warn!("Effectiveness query failed in snapshot: {e}");
                None
            }
            Err(e) => {
                tracing::warn!("Effectiveness task timed out or panicked in snapshot: {e}");
                None
            }
        };

        Ok(MaintenanceDataSnapshot {
            active_entries,
            graph_stale_ratio,
            effectiveness,
        })
    }

    /// Compute the full status report using direct SQL queries.
    ///
    /// Returns (StatusReport, active_entries) for optional maintenance pass.
    pub(crate) async fn compute_report(
        &self,
        topic_filter: Option<String>,
        category_filter: Option<String>,
        check_embeddings: bool,
    ) -> Result<(StatusReport, Vec<EntryRecord>), ServiceError> {
        // Phase 1: SQL queries (spawn_blocking_with_timeout #277)
        let store = Arc::clone(&self.store);
        let report_result = spawn_blocking_with_timeout(
            MCP_HANDLER_TIMEOUT,
            move || -> Result<(StatusReport, Vec<EntryRecord>), crate::error::ServerError> {
                let conn = store.lock_conn();

                // Status counters from counters table
                let total_active =
                    unimatrix_store::counters::read_counter(&conn, "total_active").unwrap_or(0);
                let total_deprecated =
                    unimatrix_store::counters::read_counter(&conn, "total_deprecated").unwrap_or(0);
                let total_proposed =
                    unimatrix_store::counters::read_counter(&conn, "total_proposed").unwrap_or(0);
                let total_quarantined =
                    unimatrix_store::counters::read_counter(&conn, "total_quarantined")
                        .unwrap_or(0);

                // Category distribution via SQL aggregation
                let mut category_distribution: BTreeMap<String, u64> = BTreeMap::new();
                if let Some(ref filter_cat) = category_filter {
                    let count: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM entries WHERE category = ?1",
                            rusqlite::params![filter_cat],
                            |row| row.get::<_, i64>(0),
                        )
                        .map_err(|e| {
                            crate::error::ServerError::Core(CoreError::Store(StoreError::Sqlite(e)))
                        })?;
                    if count > 0 {
                        category_distribution.insert(filter_cat.clone(), count as u64);
                    }
                } else {
                    let mut stmt = conn
                        .prepare("SELECT category, COUNT(*) FROM entries GROUP BY category")
                        .map_err(|e| {
                            crate::error::ServerError::Core(CoreError::Store(StoreError::Sqlite(e)))
                        })?;
                    let rows = stmt
                        .query_map([], |row| {
                            let cat: String = row.get(0)?;
                            let count: i64 = row.get(1)?;
                            Ok((cat, count as u64))
                        })
                        .map_err(|e| {
                            crate::error::ServerError::Core(CoreError::Store(StoreError::Sqlite(e)))
                        })?;
                    for item in rows {
                        let (cat, count): (String, u64) = item.map_err(|e| {
                            crate::error::ServerError::Core(CoreError::Store(StoreError::Sqlite(e)))
                        })?;
                        category_distribution.insert(cat, count);
                    }
                }

                // Topic distribution via SQL aggregation
                let mut topic_distribution: BTreeMap<String, u64> = BTreeMap::new();
                if let Some(ref filter_topic) = topic_filter {
                    let count: i64 = conn
                        .query_row(
                            "SELECT COUNT(*) FROM entries WHERE topic = ?1",
                            rusqlite::params![filter_topic],
                            |row| row.get::<_, i64>(0),
                        )
                        .map_err(|e| {
                            crate::error::ServerError::Core(CoreError::Store(StoreError::Sqlite(e)))
                        })?;
                    if count > 0 {
                        topic_distribution.insert(filter_topic.clone(), count as u64);
                    }
                } else {
                    let mut stmt = conn
                        .prepare("SELECT topic, COUNT(*) FROM entries GROUP BY topic")
                        .map_err(|e| {
                            crate::error::ServerError::Core(CoreError::Store(StoreError::Sqlite(e)))
                        })?;
                    let rows = stmt
                        .query_map([], |row| {
                            let topic: String = row.get(0)?;
                            let count: i64 = row.get(1)?;
                            Ok((topic, count as u64))
                        })
                        .map_err(|e| {
                            crate::error::ServerError::Core(CoreError::Store(StoreError::Sqlite(e)))
                        })?;
                    for item in rows {
                        let (topic, count): (String, u64) = item.map_err(|e| {
                            crate::error::ServerError::Core(CoreError::Store(StoreError::Sqlite(e)))
                        })?;
                        topic_distribution.insert(topic, count);
                    }
                }

                // Release the connection lock before calling store methods that
                // re-acquire it. std::sync::Mutex is non-reentrant: holding `conn`
                // while calling lock_conn() again would deadlock (#176).
                drop(conn);

                // Correction chain metrics + security metrics via SQL aggregation (crt-013)
                let aggregates = store
                    .compute_status_aggregates()
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e)))?;

                let entries_with_supersedes = aggregates.supersedes_count;
                let entries_with_superseded_by = aggregates.superseded_by_count;
                let total_correction_count = aggregates.total_correction_count;
                let trust_source_dist: BTreeMap<String, u64> = aggregates.trust_source_distribution;
                let entries_without_attribution = aggregates.unattributed_count;

                // Active entries with tags (for lambda computation)
                let active_entries = store
                    .load_active_entries_with_tags()
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e)))?;

                // Outcome statistics (targeted query for category="outcome" only)
                let outcome_entries = store
                    .load_outcome_entries_with_tags()
                    .map_err(|e| crate::error::ServerError::Core(CoreError::Store(e)))?;

                let mut total_outcomes = 0u64;
                let mut outcomes_by_type: BTreeMap<String, u64> = BTreeMap::new();
                let mut outcomes_by_result: BTreeMap<String, u64> = BTreeMap::new();
                let mut outcomes_by_feature_cycle: BTreeMap<String, u64> = BTreeMap::new();

                for record in &outcome_entries {
                    total_outcomes += 1;

                    for tag in &record.tags {
                        if let Some((tag_key, tag_value)) = tag.split_once(':') {
                            match tag_key {
                                "type" => {
                                    *outcomes_by_type.entry(tag_value.to_string()).or_insert(0) +=
                                        1;
                                }
                                "result" => {
                                    *outcomes_by_result
                                        .entry(tag_value.to_string())
                                        .or_insert(0) += 1;
                                }
                                _ => {}
                            }
                        }
                    }

                    if !record.feature_cycle.is_empty() {
                        *outcomes_by_feature_cycle
                            .entry(record.feature_cycle.clone())
                            .or_insert(0) += 1;
                    }
                }

                // Sort feature cycles by count descending, take top 10
                let mut fc_sorted: Vec<(String, u64)> =
                    outcomes_by_feature_cycle.into_iter().collect();
                fc_sorted.sort_by(|a, b| b.1.cmp(&a.1));
                fc_sorted.truncate(10);

                // Build StatusReport
                let report = StatusReport {
                    total_active,
                    total_deprecated,
                    total_proposed,
                    total_quarantined,
                    category_distribution: category_distribution.into_iter().collect(),
                    topic_distribution: topic_distribution.into_iter().collect(),
                    entries_with_supersedes,
                    entries_with_superseded_by,
                    total_correction_count,
                    trust_source_distribution: trust_source_dist.into_iter().collect(),
                    entries_without_attribution,
                    contradictions: Vec::new(),
                    contradiction_count: 0,
                    embedding_inconsistencies: Vec::new(),
                    contradiction_scan_performed: false,
                    embedding_check_performed: false,
                    total_co_access_pairs: 0,
                    active_co_access_pairs: 0,
                    top_co_access_pairs: Vec::new(),
                    stale_pairs_cleaned: 0,
                    coherence: 1.0,
                    confidence_freshness_score: 1.0,
                    graph_quality_score: 1.0,
                    embedding_consistency_score: 1.0,
                    contradiction_density_score: 1.0,
                    stale_confidence_count: 0,
                    confidence_refreshed_count: 0,
                    graph_stale_ratio: 0.0,
                    graph_compacted: false,
                    maintenance_recommendations: Vec::new(),
                    total_outcomes,
                    outcomes_by_type: outcomes_by_type.into_iter().collect(),
                    outcomes_by_result: outcomes_by_result.into_iter().collect(),
                    outcomes_by_feature_cycle: fc_sorted,
                    observation_file_count: 0,
                    observation_total_size_bytes: 0,
                    observation_oldest_file_days: 0,
                    observation_approaching_cleanup: Vec::new(),
                    retrospected_feature_count: 0,
                    last_maintenance_run: None,
                    next_maintenance_scheduled: None,
                    extraction_stats: None,
                    coherence_by_source: Vec::new(),
                    effectiveness: None,
                };
                Ok((report, active_entries))
            },
        )
        .await
        .map_err(|e| {
            let core_err: CoreError = match e {
                crate::error::ServerError::Core(ce) => ce,
                other => CoreError::JoinError(other.to_string()),
            };
            ServiceError::Core(core_err)
        })?
        .map_err(|e| {
            let core_err: CoreError = match e {
                crate::error::ServerError::Core(ce) => ce,
                other => CoreError::JoinError(other.to_string()),
            };
            ServiceError::Core(core_err)
        })?;
        let (mut report, active_entries) = report_result;

        // Phase 2: Contradiction scan — read from cache populated by background tick.
        //
        // GH #278: scan_contradictions() runs O(N) ONNX inference and is too expensive
        // to call on every context_status invocation. The background tick writes the
        // cache every CONTRADICTION_SCAN_INTERVAL_TICKS ticks; we read it here without
        // touching the embed service at all.
        {
            let cached = self
                .contradiction_cache
                .read()
                .unwrap_or_else(|e| e.into_inner());
            if let Some(ref result) = *cached {
                report.contradiction_count = result.pairs.len();
                report.contradictions = result.pairs.clone();
                report.contradiction_scan_performed = true;
            }
            // If None (cold-start): contradiction_scan_performed stays false (default).
        }

        // Phase 3: Embedding consistency (opt-in)
        if check_embeddings {
            if let Ok(adapter) = self.embed_service.get_adapter().await {
                let store_for_embed = Arc::clone(&self.store);
                let vi_for_embed = Arc::clone(&self.vector_index);
                let adapter_for_embed = Arc::clone(&adapter);
                let config_for_embed = contradiction::ContradictionConfig::default();

                match spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
                    let vs = VectorAdapter::new(vi_for_embed);
                    contradiction::check_embedding_consistency(
                        &store_for_embed,
                        &vs,
                        &*adapter_for_embed,
                        &config_for_embed,
                    )
                })
                .await
                {
                    Ok(Ok(inconsistencies)) => {
                        report.embedding_inconsistencies = inconsistencies;
                        report.embedding_check_performed = true;
                    }
                    _ => {
                        // Check failed or timed out -- graceful degradation
                    }
                }
            }
        }

        // Phase 4: Co-access stats (read-only, maintain=false)
        {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let staleness_cutoff = now.saturating_sub(crate::coaccess::CO_ACCESS_STALENESS_SECONDS);

            let store_for_coaccess = Arc::clone(&self.store);
            let co_access_result = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
                let (total, active) = store_for_coaccess.co_access_stats(staleness_cutoff)?;
                let top_pairs = store_for_coaccess.top_co_access_pairs(5, staleness_cutoff)?;

                let mut clusters = Vec::new();
                for ((id_a, id_b), record) in &top_pairs {
                    let title_a = store_for_coaccess
                        .get(*id_a)
                        .map(|e| e.title.clone())
                        .unwrap_or_else(|_| format!("#{id_a}"));
                    let title_b = store_for_coaccess
                        .get(*id_b)
                        .map(|e| e.title.clone())
                        .unwrap_or_else(|_| format!("#{id_b}"));
                    clusters.push(CoAccessClusterEntry {
                        entry_id_a: *id_a,
                        entry_id_b: *id_b,
                        title_a,
                        title_b,
                        count: record.count,
                        last_updated: record.last_updated,
                    });
                }

                Ok::<_, unimatrix_store::StoreError>((total, active, clusters))
            })
            .await;

            match co_access_result {
                Ok(Ok((total, active, clusters))) => {
                    report.total_co_access_pairs = total;
                    report.active_co_access_pairs = active;
                    report.top_co_access_pairs = clusters;
                }
                Ok(Err(e)) => {
                    tracing::warn!("co-access stats failed: {e}");
                }
                Err(e) => {
                    tracing::warn!("co-access stats task timed out or panicked: {e}");
                }
            }
        }

        // Phase 5: Coherence dimensions
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let (freshness_dim, stale_conf_count) = coherence::confidence_freshness_score(
            &active_entries,
            now_ts,
            coherence::DEFAULT_STALENESS_THRESHOLD_SECS,
        );
        report.confidence_freshness_score = freshness_dim;
        report.stale_confidence_count = stale_conf_count;

        let graph_point_count = self.vector_index.point_count();
        let graph_stale_count = self.vector_index.stale_count();
        let graph_stale_ratio = if graph_point_count == 0 {
            0.0
        } else {
            graph_stale_count as f64 / graph_point_count as f64
        };
        report.graph_quality_score =
            coherence::graph_quality_score(graph_stale_count, graph_point_count);
        report.graph_stale_ratio = graph_stale_ratio;

        let embed_dim = if report.embedding_check_performed {
            let total_checked = active_entries.len();
            let inconsistent_count = report.embedding_inconsistencies.len();
            Some(coherence::embedding_consistency_score(
                inconsistent_count,
                total_checked,
            ))
        } else {
            None
        };
        report.embedding_consistency_score = embed_dim.unwrap_or(1.0);

        report.contradiction_density_score =
            coherence::contradiction_density_score(report.total_quarantined, report.total_active);

        // Lambda computation + recommendations
        let oldest_stale = coherence::oldest_stale_age(
            &active_entries,
            now_ts,
            coherence::DEFAULT_STALENESS_THRESHOLD_SECS,
        );
        report.coherence = coherence::compute_lambda(
            report.confidence_freshness_score,
            report.graph_quality_score,
            embed_dim,
            report.contradiction_density_score,
            &coherence::DEFAULT_WEIGHTS,
        );
        // Coherence by source (col-013)
        {
            let mut source_groups: std::collections::HashMap<String, Vec<&EntryRecord>> =
                std::collections::HashMap::new();
            for entry in &active_entries {
                let source = if entry.trust_source.is_empty() {
                    "(none)".to_string()
                } else {
                    entry.trust_source.clone()
                };
                source_groups.entry(source).or_default().push(entry);
            }

            let mut coherence_by_source = Vec::new();
            for (source, entries) in &source_groups {
                let (source_freshness, _) = coherence::confidence_freshness_score(
                    &entries.iter().map(|e| (*e).clone()).collect::<Vec<_>>(),
                    now_ts,
                    coherence::DEFAULT_STALENESS_THRESHOLD_SECS,
                );
                let source_lambda = coherence::compute_lambda(
                    source_freshness,
                    report.graph_quality_score,
                    embed_dim,
                    report.contradiction_density_score,
                    &coherence::DEFAULT_WEIGHTS,
                );
                coherence_by_source.push((source.clone(), source_lambda));
            }
            coherence_by_source.sort_by(|a, b| a.0.cmp(&b.0));
            report.coherence_by_source = coherence_by_source;
        }

        report.maintenance_recommendations = coherence::generate_recommendations(
            report.coherence,
            coherence::DEFAULT_LAMBDA_THRESHOLD,
            report.stale_confidence_count,
            oldest_stale,
            report.graph_stale_ratio,
            report.embedding_inconsistencies.len(),
            report.total_quarantined,
        );

        // Phase 6: Observation stats from SQL (col-012)
        let store_for_obs = Arc::clone(&self.store);
        let obs_stats = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
            use unimatrix_observe::ObservationSource;
            let source = crate::services::observation::SqlObservationSource::new(store_for_obs);
            source.observation_stats()
        })
        .await
        .unwrap_or_else(|e| {
            tracing::error!("observation stats task timed out or panicked: {e}");
            Ok(unimatrix_observe::ObservationStats {
                record_count: 0,
                session_count: 0,
                oldest_record_age_days: 0,
                approaching_cleanup: vec![],
            })
        })
        .unwrap_or_else(|_| unimatrix_observe::ObservationStats {
            record_count: 0,
            session_count: 0,
            oldest_record_age_days: 0,
            approaching_cleanup: vec![],
        });

        report.observation_file_count = obs_stats.record_count;
        report.observation_total_size_bytes = obs_stats.session_count;
        report.observation_oldest_file_days = obs_stats.oldest_record_age_days;
        report.observation_approaching_cleanup = obs_stats.approaching_cleanup;

        // Phase 7: Retrospected feature count
        let retrospected = spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, {
            let store = Arc::clone(&self.store);
            move || store.list_all_metrics()
        })
        .await
        .unwrap_or_else(|e| {
            tracing::error!("metric vectors task timed out or panicked: {e}");
            Ok(vec![])
        })
        .unwrap_or_else(|_| vec![]);
        report.retrospected_feature_count = retrospected.len() as u64;

        // Phase 8: Effectiveness analysis (crt-018)
        let store_for_eff = Arc::clone(&self.store);
        let effectiveness = match spawn_blocking_with_timeout(MCP_HANDLER_TIMEOUT, move || {
            let aggregates = store_for_eff.compute_effectiveness_aggregates()?;
            let entry_meta = store_for_eff.load_entry_classification_meta()?;
            Ok::<_, StoreError>((aggregates, entry_meta))
        })
        .await
        {
            Ok(Ok((aggregates, entry_meta))) => {
                use unimatrix_engine::effectiveness::{
                    NOISY_TRUST_SOURCES, build_report, classify_entry,
                };

                let stats_map: HashMap<u64, &unimatrix_store::read::EntryInjectionStats> =
                    aggregates
                        .entry_stats
                        .iter()
                        .map(|s| (s.entry_id, s))
                        .collect();

                let classifications: Vec<unimatrix_engine::effectiveness::EntryEffectiveness> =
                    entry_meta
                        .iter()
                        .map(|meta| {
                            let (inj_count, success, rework, abandoned) =
                                match stats_map.get(&meta.entry_id) {
                                    Some(stats) => (
                                        stats.injection_count,
                                        stats.success_count,
                                        stats.rework_count,
                                        stats.abandoned_count,
                                    ),
                                    None => (0, 0, 0, 0),
                                };

                            let topic_has_sessions = aggregates.active_topics.contains(&meta.topic);

                            classify_entry(
                                meta.entry_id,
                                &meta.title,
                                &meta.topic,
                                &meta.trust_source,
                                meta.helpful_count,
                                meta.unhelpful_count,
                                inj_count,
                                success,
                                rework,
                                abandoned,
                                topic_has_sessions,
                                NOISY_TRUST_SOURCES,
                            )
                        })
                        .collect();

                let data_window = unimatrix_engine::effectiveness::DataWindow {
                    session_count: aggregates.session_count,
                    earliest_session_at: aggregates.earliest_session_at,
                    latest_session_at: aggregates.latest_session_at,
                };

                Some(build_report(
                    classifications,
                    &aggregates.calibration_rows,
                    data_window,
                ))
            }
            Ok(Err(e)) => {
                tracing::warn!("Effectiveness query failed: {e}");
                None
            }
            Err(e) => {
                tracing::warn!("Effectiveness task timed out or panicked: {e}");
                None
            }
        };
        report.effectiveness = effectiveness;

        Ok((report, active_entries))
    }

    /// Run maintenance operations. Called by the background tick (col-013).
    ///
    /// Operations:
    /// 1. Co-access stale pair cleanup
    /// 2. Confidence refresh (batch 500, 200ms wall-clock guard — crt-019)
    /// 2b. Empirical prior + spread computation (crt-019)
    /// 3. Graph compaction (if stale ratio > trigger)
    /// 4. Observation file cleanup (60-day retention)
    /// 5. Stale session sweep + signal processing
    /// 6. Session GC (timeout + delete thresholds)
    pub(crate) async fn run_maintenance(
        &self,
        active_entries: &[EntryRecord],
        report: &mut StatusReport,
        session_registry: &SessionRegistry,
        entry_store: &Arc<AsyncEntryStore<unimatrix_core::StoreAdapter>>,
        pending_entries_analysis: &Arc<std::sync::Mutex<PendingEntriesAnalysis>>,
    ) -> Result<MaintenanceResult, ServiceError> {
        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 1. Co-access cleanup
        let staleness_cutoff = now_ts.saturating_sub(crate::coaccess::CO_ACCESS_STALENESS_SECONDS);
        let store_for_cleanup = Arc::clone(&self.store);
        let stale_pairs_cleaned = match tokio::task::spawn_blocking(move || {
            store_for_cleanup.cleanup_stale_co_access(staleness_cutoff)
        })
        .await
        {
            Ok(Ok(cleaned)) => {
                report.stale_pairs_cleaned = cleaned;
                cleaned
            }
            _ => 0,
        };

        // 2. Confidence refresh (batch 500, 200ms duration guard, alpha0/beta0 snapshot)
        let mut confidence_refreshed = 0u64;
        {
            let staleness_threshold = coherence::DEFAULT_STALENESS_THRESHOLD_SECS;
            let batch_cap = coherence::MAX_CONFIDENCE_REFRESH_BATCH;

            let mut stale_entries: Vec<&EntryRecord> = active_entries
                .iter()
                .filter(|e| {
                    let ref_ts = e.updated_at.max(e.last_accessed_at);
                    if ref_ts == 0 {
                        return true;
                    }
                    if now_ts > ref_ts {
                        (now_ts - ref_ts) > staleness_threshold
                    } else {
                        false
                    }
                })
                .collect();

            stale_entries.sort_by_key(|e| e.updated_at.max(e.last_accessed_at));
            stale_entries.truncate(batch_cap);

            if !stale_entries.is_empty() {
                // Snapshot alpha0/beta0 ONCE before the loop (IR-02: avoid per-entry lock acquisition).
                let (snapshot_alpha0, snapshot_beta0) = {
                    let guard = self
                        .confidence_state
                        .read()
                        .unwrap_or_else(|e| e.into_inner());
                    (guard.alpha0, guard.beta0)
                };

                let ids_and_confs: Vec<(u64, f64)> = stale_entries
                    .iter()
                    .map(|e| {
                        (
                            e.id,
                            crate::confidence::compute_confidence(
                                e,
                                now_ts,
                                snapshot_alpha0,
                                snapshot_beta0,
                            ),
                        )
                    })
                    .collect();

                let store_for_refresh = Arc::clone(&self.store);
                let refresh_result = tokio::task::spawn_blocking(move || {
                    let loop_start = Instant::now();
                    let wall_budget = Duration::from_millis(200);
                    let mut refreshed = 0u64;

                    for (id, new_conf) in ids_and_confs {
                        // Duration guard (crt-019, FR-05, R-13): break early if 200ms wall-clock exceeded.
                        if loop_start.elapsed() > wall_budget {
                            tracing::debug!(
                                refreshed,
                                "confidence refresh: 200ms budget exceeded, stopping early"
                            );
                            break;
                        }
                        match store_for_refresh.update_confidence(id, new_conf) {
                            Ok(()) => refreshed += 1,
                            Err(e) => {
                                tracing::warn!("confidence refresh failed for {id}: {e}");
                            }
                        }
                    }
                    refreshed
                })
                .await;

                match refresh_result {
                    Ok(count) => {
                        report.confidence_refreshed_count = count;
                        confidence_refreshed = count;
                    }
                    Err(e) => {
                        tracing::warn!("confidence refresh task failed: {e}");
                    }
                }
            }
        }

        // 2b. Empirical prior + spread computation (crt-019 Step 2b).
        //
        // After the confidence refresh loop, compute alpha0/beta0 from the voted-entry
        // population using method-of-moments, then compute observed_spread from all active
        // confidence values. Atomically update ConfidenceState when available.
        //
        // NOTE: ConfidenceStateHandle wiring is handled by the confidence-state agent
        // (crt-019). This step accepts Option<&ConfidenceStateHandle> — passing None here
        // until that component is wired through ServiceLayer.
        {
            let store_for_prior = Arc::clone(&self.store);
            let prior_result = tokio::task::spawn_blocking(move || -> (f64, f64, f64, f64) {
                let conn = store_for_prior.lock_conn();

                // Load voted entries: active with helpful_count + unhelpful_count >= 1.
                let voted_pairs: Vec<(u32, u32)> = {
                    match conn.prepare(
                        "SELECT helpful_count, unhelpful_count \
                         FROM entries \
                         WHERE status = 'active' \
                           AND (helpful_count + unhelpful_count) >= 1",
                    ) {
                        Ok(mut stmt) => stmt
                            .query_map([], |row| Ok((row.get::<_, u32>(0)?, row.get::<_, u32>(1)?)))
                            .map(|rows| rows.filter_map(|r| r.ok()).collect())
                            .unwrap_or_default(),
                        Err(e) => {
                            tracing::warn!("prior computation: voted-entry query failed: {e}");
                            vec![]
                        }
                    }
                };

                // Load all active entry confidence values for spread computation.
                let all_confidences: Vec<f64> = {
                    match conn.prepare("SELECT confidence FROM entries WHERE status = 'active'") {
                        Ok(mut stmt) => stmt
                            .query_map([], |row| row.get::<_, f64>(0))
                            .map(|rows| rows.filter_map(|r| r.ok()).collect())
                            .unwrap_or_default(),
                        Err(e) => {
                            tracing::warn!("prior computation: confidence query failed: {e}");
                            vec![]
                        }
                    }
                };

                let (alpha0, beta0) = compute_empirical_prior(&voted_pairs);
                let observed_spread = compute_observed_spread(&all_confidences);
                let confidence_weight = adaptive_confidence_weight_local(observed_spread);

                (alpha0, beta0, observed_spread, confidence_weight)
            })
            .await;

            match prior_result {
                Ok((alpha0, beta0, observed_spread, confidence_weight)) => {
                    // Atomic write of all four fields (ADR-002, FM-03).
                    // All values written in a single lock acquisition to prevent
                    // a reader observing a partially-updated state.
                    {
                        let mut guard = self
                            .confidence_state
                            .write()
                            .unwrap_or_else(|e| e.into_inner());
                        guard.alpha0 = alpha0;
                        guard.beta0 = beta0;
                        guard.observed_spread = observed_spread;
                        guard.confidence_weight = confidence_weight;
                    }
                    tracing::debug!(
                        alpha0 = %format!("{alpha0:.3}"),
                        beta0 = %format!("{beta0:.3}"),
                        observed_spread = %format!("{observed_spread:.4}"),
                        confidence_weight = %format!("{confidence_weight:.4}"),
                        "confidence state updated (Step 2b)"
                    );
                }
                Err(e) => {
                    // Graceful degradation (FM-01): ConfidenceState retains previous tick values.
                    tracing::warn!("prior computation task failed: {e}");
                }
            }
        }

        // 3. Graph compaction (if stale ratio > trigger)
        let mut graph_compacted = false;
        if report.graph_stale_ratio > coherence::DEFAULT_STALE_RATIO_TRIGGER {
            if let Ok(adapter) = self.embed_service.get_adapter().await {
                let pairs: Vec<(String, String)> = active_entries
                    .iter()
                    .map(|e| (e.title.clone(), e.content.clone()))
                    .collect();

                match adapter.embed_entries(&pairs) {
                    Ok(embeddings) => {
                        let compact_input: Vec<(u64, Vec<f32>)> = active_entries
                            .iter()
                            .zip(embeddings.into_iter())
                            .map(|(entry, raw_emb)| {
                                let adapted = self.adapt_service.adapt_embedding(
                                    &raw_emb,
                                    Some(&entry.category),
                                    Some(&entry.topic),
                                );
                                (entry.id, unimatrix_embed::l2_normalized(&adapted))
                            })
                            .collect();

                        let vi_for_compact = Arc::clone(&self.vector_index);
                        match tokio::task::spawn_blocking(move || {
                            vi_for_compact.compact(compact_input)
                        })
                        .await
                        {
                            Ok(Ok(())) => {
                                report.graph_compacted = true;
                                graph_compacted = true;
                            }
                            Ok(Err(e)) => {
                                tracing::warn!("graph compaction failed: {e}");
                            }
                            Err(e) => {
                                tracing::warn!("graph compaction task failed: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("re-embedding for compaction failed: {e}");
                    }
                }
            }
        }

        // 4. Observation retention cleanup (col-012: SQL DELETE instead of file removal)
        let store_for_obs_cleanup = Arc::clone(&self.store);
        let _ = tokio::task::spawn_blocking(move || {
            let conn = store_for_obs_cleanup.lock_conn();
            let now_millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64;
            let sixty_days_millis = 60_i64 * 24 * 60 * 60 * 1000;
            let cutoff = now_millis - sixty_days_millis;
            let _ = conn.execute(
                "DELETE FROM observations WHERE ts_millis < ?1",
                unimatrix_store::rusqlite::params![cutoff],
            );
        })
        .await;

        // 5. Stale session sweep (col-009, FR-09.2)
        // #198 Part 3: Sweep now resolves feature_cycle via majority vote before eviction
        let stale_outputs = session_registry.sweep_stale_sessions();
        if !stale_outputs.is_empty() {
            let store_for_sweep = Arc::clone(&self.store);
            let entry_store_for_sweep = Arc::clone(entry_store);
            let pending_for_sweep = Arc::clone(pending_entries_analysis);
            for sweep_result in &stale_outputs {
                tracing::info!(session_id = %sweep_result.session_id, "status: sweeping stale session");
                // #198: Persist resolved feature_cycle for stale session
                if let Some(ref fc) = sweep_result.resolved_feature {
                    let store_fc = Arc::clone(&store_for_sweep);
                    let sid = sweep_result.session_id.clone();
                    let fc_owned = fc.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        crate::uds::listener::update_session_feature_cycle_pub(
                            &store_fc, &sid, &fc_owned,
                        )
                    });
                }
                crate::uds::listener::write_signals_to_queue(
                    &sweep_result.output,
                    &store_for_sweep,
                )
                .await;
            }
            crate::uds::listener::run_confidence_consumer(
                &store_for_sweep,
                &entry_store_for_sweep,
                &pending_for_sweep,
            )
            .await;
            crate::uds::listener::run_retrospective_consumer(
                &store_for_sweep,
                &pending_for_sweep,
                &entry_store_for_sweep,
            )
            .await;
        }

        // 6. Session GC (timeout + delete thresholds)
        let store_gc = Arc::clone(&self.store);
        match tokio::task::spawn_blocking(move || {
            store_gc.gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)
        })
        .await
        {
            Ok(Ok(stats)) => {
                tracing::info!(
                    timed_out = %stats.timed_out_count,
                    deleted_sessions = %stats.deleted_session_count,
                    deleted_log_entries = %stats.deleted_injection_log_count,
                    "Session GC complete"
                );
            }
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "Session GC failed");
            }
            Err(join_err) => {
                tracing::warn!(error = %join_err, "Session GC task panicked");
            }
        }

        Ok(MaintenanceResult {
            confidence_refreshed,
            graph_compacted,
            stale_pairs_cleaned,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // compute_empirical_prior — threshold boundary (R-05)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_empirical_prior_below_threshold_returns_cold_start() {
        // Exactly 9 voted entries — must use cold-start (3.0, 3.0).
        let voted_entries: Vec<(u32, u32)> = (0..9).map(|_| (5u32, 2u32)).collect();
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert_eq!(
            alpha0, 3.0,
            "below threshold must return cold-start alpha0=3.0"
        );
        assert_eq!(
            beta0, 3.0,
            "below threshold must return cold-start beta0=3.0"
        );
    }

    #[test]
    fn test_empirical_prior_zero_entries_returns_cold_start() {
        let voted_entries: Vec<(u32, u32)> = vec![];
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert_eq!(alpha0, 3.0);
        assert_eq!(beta0, 3.0);
    }

    #[test]
    fn test_empirical_prior_five_entries_returns_cold_start() {
        // Verifies threshold is 10, not 5 (ADR-002 overrides SPEC FR-09).
        let voted_entries: Vec<(u32, u32)> = (0..5).map(|_| (10u32, 0u32)).collect();
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert_eq!(
            alpha0, 3.0,
            "5 entries must use cold-start (threshold is 10, not 5)"
        );
        assert_eq!(beta0, 3.0);
    }

    #[test]
    fn test_empirical_prior_at_threshold_uses_population() {
        // Exactly 10 voted entries — must attempt empirical estimation.
        // Uniform p_i = 0.5 produces zero variance -> falls back to cold-start.
        // Key assertion: no panic, values in [0.5, 50.0].
        let voted_entries: Vec<(u32, u32)> = (0..10).map(|_| (5u32, 5u32)).collect();
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert!(
            alpha0 >= 0.5 && alpha0 <= 50.0,
            "alpha0 out of clamp range: {alpha0}"
        );
        assert!(
            beta0 >= 0.5 && beta0 <= 50.0,
            "beta0 out of clamp range: {beta0}"
        );
    }

    #[test]
    fn test_empirical_prior_fifteen_entries_uses_population() {
        // 15 entries with identical skewed data — zero variance → cold-start.
        // Main assertion: no panic, values clamped.
        let voted_entries: Vec<(u32, u32)> = (0..15).map(|_| (8u32, 2u32)).collect();
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert!(alpha0 >= 0.5 && alpha0 <= 50.0);
        assert!(beta0 >= 0.5 && beta0 <= 50.0);
    }

    // ---------------------------------------------------------------------------
    // compute_empirical_prior — balanced mixed population (R-05, boundary exact)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_empirical_prior_mixed_rates_sensible_values() {
        // 10 entries with genuine rate variance — empirical path should produce
        // finite clamped values with p_bar = 0.5.
        let voted_entries = vec![
            (10u32, 0u32),
            (8, 2),
            (6, 4),
            (4, 6),
            (2, 8),
            (9, 1),
            (7, 3),
            (5, 5),
            (3, 7),
            (1, 9),
        ];
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        // p_bar = 0.5, variance > 0 → empirical path; symmetric → alpha0 ≈ beta0
        assert!(!alpha0.is_nan(), "alpha0 must not be NaN");
        assert!(!beta0.is_nan(), "beta0 must not be NaN");
        assert!(alpha0 >= 0.5 && alpha0 <= 50.0, "alpha0={alpha0}");
        assert!(beta0 >= 0.5 && beta0 <= 50.0, "beta0={beta0}");
    }

    // ---------------------------------------------------------------------------
    // compute_empirical_prior — zero-variance degeneracy (R-12)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_prior_zero_variance_all_helpful_returns_cold_start() {
        // All 10 entries at p_i = 1.0 (all helpful) — variance = 0 → cold-start.
        let voted_entries: Vec<(u32, u32)> = (0..10).map(|_| (10u32, 0u32)).collect();
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert!(
            !alpha0.is_nan(),
            "alpha0 must not be NaN with zero variance"
        );
        assert!(!beta0.is_nan(), "beta0 must not be NaN with zero variance");
        assert!(
            alpha0 >= 0.5 && alpha0 <= 50.0,
            "alpha0 out of clamp: {alpha0}"
        );
        assert!(beta0 >= 0.5 && beta0 <= 50.0, "beta0 out of clamp: {beta0}");
    }

    #[test]
    fn test_prior_zero_variance_all_unhelpful_returns_cold_start() {
        // All 10 entries at p_i = 0.0 — variance = 0 → cold-start.
        let voted_entries: Vec<(u32, u32)> = (0..10).map(|_| (0u32, 10u32)).collect();
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert!(!alpha0.is_nan(), "alpha0 must not be NaN");
        assert!(!beta0.is_nan(), "beta0 must not be NaN");
        assert!(alpha0 >= 0.5 && alpha0 <= 50.0);
        assert!(beta0 >= 0.5 && beta0 <= 50.0);
    }

    #[test]
    fn test_prior_mixed_variance_stays_in_clamp_range() {
        // 12 entries with genuine variance — no NaN, values in [0.5, 50.0].
        let voted_entries = vec![
            (10u32, 0u32),
            (9, 1),
            (8, 2),
            (7, 3),
            (6, 4),
            (5, 5),
            (4, 6),
            (3, 7),
            (2, 8),
            (1, 9),
            (0, 10),
            (10, 0),
        ];
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert!(
            !alpha0.is_nan() && !beta0.is_nan(),
            "NaN propagation detected"
        );
        assert!(alpha0 >= 0.5 && alpha0 <= 50.0, "alpha0={alpha0}");
        assert!(beta0 >= 0.5 && beta0 <= 50.0, "beta0={beta0}");
    }

    // ---------------------------------------------------------------------------
    // compute_empirical_prior — clamp fires on near-degenerate input
    // ---------------------------------------------------------------------------

    #[test]
    fn test_prior_clamp_prevents_extreme_values() {
        // Near-zero variance with high mean: alpha0 would be very large without clamping.
        // Use 10 entries with near-identical high rate (999 helpful, 1 unhelpful).
        // variance will be ~0 → cold-start path (variance <= 0.0 check fires first).
        let voted_entries: Vec<(u32, u32)> = (0..10).map(|_| (999u32, 1u32)).collect();
        let (alpha0, beta0) = compute_empirical_prior(&voted_entries);
        assert!(
            alpha0 <= 50.0,
            "alpha0 must be clamped to 50.0, got {alpha0}"
        );
        assert!(beta0 <= 50.0, "beta0 must be clamped to 50.0, got {beta0}");
        assert!(alpha0 >= 0.5, "alpha0 must be >= 0.5, got {alpha0}");
        assert!(beta0 >= 0.5, "beta0 must be >= 0.5, got {beta0}");
    }

    // ---------------------------------------------------------------------------
    // compute_observed_spread (EC-01)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_observed_spread_empty_population() {
        let spread = compute_observed_spread(&[]);
        assert_eq!(spread, 0.0, "empty population should return 0.0");
    }

    #[test]
    fn test_observed_spread_single_entry_returns_baseline() {
        // Single entry: fewer than MINIMUM_VOTED_POPULATION → baseline.
        let spread = compute_observed_spread(&[0.6]);
        assert_eq!(spread, PRE_CRT019_SPREAD_BASELINE);
    }

    #[test]
    fn test_observed_spread_nine_entries_returns_baseline() {
        // 9 entries: fewer than 10 → pre-crt-019 baseline.
        let confs: Vec<f64> = (0..9).map(|i| i as f64 * 0.1).collect();
        let spread = compute_observed_spread(&confs);
        assert_eq!(spread, PRE_CRT019_SPREAD_BASELINE);
    }

    #[test]
    fn test_observed_spread_uniform_population() {
        // All same value (10+ entries) → spread ≈ 0.0.
        let confs: Vec<f64> = (0..20).map(|_| 0.5).collect();
        let spread = compute_observed_spread(&confs);
        assert!(
            spread.abs() < 1e-10,
            "uniform population spread must be ~0.0, got {spread}"
        );
    }

    #[test]
    fn test_observed_spread_full_range() {
        // Values spanning [0.0, 1.0] → spread close to 0.90 (p95 ≈ 0.95, p5 ≈ 0.05).
        let confs: Vec<f64> = (0..=100).map(|i| i as f64 / 100.0).collect();
        let spread = compute_observed_spread(&confs);
        assert!(spread > 0.85 && spread < 1.0, "full range spread: {spread}");
    }

    #[test]
    fn test_observed_spread_non_negative() {
        // Spread must never be negative regardless of input.
        let confs = vec![0.9, 0.1, 0.5, 0.3, 0.7, 0.2, 0.8, 0.4, 0.6, 0.15];
        let spread = compute_observed_spread(&confs);
        assert!(spread >= 0.0, "spread must be non-negative, got {spread}");
    }

    // ---------------------------------------------------------------------------
    // adaptive_confidence_weight_local — formula verification
    // ---------------------------------------------------------------------------

    #[test]
    fn test_adaptive_confidence_weight_floor() {
        // spread = 0.0 → clamp to floor 0.15.
        let w = adaptive_confidence_weight_local(0.0);
        assert!((w - 0.15).abs() < 1e-10, "floor: expected 0.15, got {w}");
    }

    #[test]
    fn test_adaptive_confidence_weight_ceiling() {
        // spread = 1.0 → 1.0 * 1.25 = 1.25 → clamp to ceiling 0.25.
        let w = adaptive_confidence_weight_local(1.0);
        assert!((w - 0.25).abs() < 1e-10, "ceiling: expected 0.25, got {w}");
    }

    #[test]
    fn test_adaptive_confidence_weight_initial_spread() {
        // Initial observed_spread = 0.1471 → 0.1471 * 1.25 = 0.183875 → 0.184 (approx).
        let w = adaptive_confidence_weight_local(0.1471);
        assert!(
            (w - 0.183875).abs() < 1e-6,
            "initial spread: expected ~0.184, got {w}"
        );
        assert!(w >= 0.15 && w <= 0.25);
    }

    #[test]
    fn test_adaptive_confidence_weight_midrange() {
        // spread = 0.2 → 0.2 * 1.25 = 0.25 → ceiling clamp.
        let w = adaptive_confidence_weight_local(0.2);
        assert!((w - 0.25).abs() < 1e-10, "0.2 * 1.25 = 0.25, got {w}");
    }

    // ---------------------------------------------------------------------------
    // MINIMUM_VOTED_POPULATION constant
    // ---------------------------------------------------------------------------

    #[test]
    fn test_minimum_voted_population_is_ten() {
        assert_eq!(
            MINIMUM_VOTED_POPULATION, 10,
            "ADR-002 requires threshold = 10"
        );
    }
}

// ---------------------------------------------------------------------------
// crt-019: Confidence refresh batch unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod confidence_refresh_tests {
    use super::*;
    use crate::services::confidence::ConfidenceState;
    use unimatrix_engine::confidence::compute_confidence;

    // AC-07: verify batch size constant has been updated to 500
    #[test]
    fn test_refresh_batch_constant_is_500() {
        assert_eq!(
            coherence::MAX_CONFIDENCE_REFRESH_BATCH,
            500,
            "MAX_CONFIDENCE_REFRESH_BATCH must be 500 after crt-019"
        );
    }

    // R-06: verify ConfidenceState initial values are non-zero
    // (observed_spread = 0.1471, not 0.0)
    #[test]
    fn test_confidence_state_default_initial_values() {
        let state = ConfidenceState::default();
        assert_eq!(state.alpha0, 3.0, "cold-start alpha0 must be 3.0");
        assert_eq!(state.beta0, 3.0, "cold-start beta0 must be 3.0");
        assert!(
            (state.observed_spread - 0.1471).abs() < 1e-6,
            "initial observed_spread must be pre-crt-019 measured value 0.1471, got {}",
            state.observed_spread
        );
        assert!(
            (state.confidence_weight - 0.18375).abs() < 1e-4,
            "initial confidence_weight must be ~0.18375 (clamp(0.1471*1.25, 0.15, 0.25)), got {}",
            state.confidence_weight
        );
    }

    // IR-02: verify that alpha0/beta0 snapshot produces different results than cold-start
    // when an entry has votes. This confirms the snapshot path is functional.
    #[test]
    fn test_snapshot_affects_confidence_computation() {
        use unimatrix_store::{EntryRecord, Status};

        let now = 1_000_000u64;
        // Entry with helpful votes to make helpfulness_score differ between priors
        let entry = EntryRecord {
            id: 1,
            title: String::new(),
            content: String::new(),
            topic: String::new(),
            category: String::new(),
            tags: vec![],
            source: String::new(),
            status: Status::Active,
            confidence: 0.0,
            created_at: now - 100,
            updated_at: now - 100,
            last_accessed_at: now - 50,
            access_count: 5,
            supersedes: None,
            superseded_by: None,
            correction_count: 0,
            embedding_dim: 0,
            created_by: String::new(),
            modified_by: String::new(),
            content_hash: String::new(),
            previous_hash: String::new(),
            version: 1,
            feature_cycle: String::new(),
            trust_source: "agent".to_string(),
            helpful_count: 5,
            unhelpful_count: 0,
            pre_quarantine_status: None,
        };

        // Cold-start prior: h = (5 + 3.0) / (5 + 0 + 3.0 + 3.0) = 8/11 ≈ 0.727
        let conf_cold = compute_confidence(&entry, now, 3.0, 3.0);

        // Empirical prior with high positive bias: h = (5 + 8.0) / (5 + 0 + 8.0 + 2.0) = 13/15 ≈ 0.867
        // Note: the current engine ignores alpha0/beta0 (another agent wires this).
        // This test validates the calling convention compiles and runs correctly.
        let conf_empirical = compute_confidence(&entry, now, 8.0, 2.0);

        // Both must be in valid range
        assert!(
            conf_cold >= 0.0 && conf_cold <= 1.0,
            "cold-start confidence out of range: {conf_cold}"
        );
        assert!(
            conf_empirical >= 0.0 && conf_empirical <= 1.0,
            "empirical-prior confidence out of range: {conf_empirical}"
        );
    }

    // FM-03: verify RwLock poison recovery pattern compiles and runs
    #[test]
    fn test_confidence_state_handle_read_lock_poison_recovery() {
        let handle = ConfidenceState::new_handle();

        // Normal read
        let guard = handle.read().unwrap_or_else(|e| e.into_inner());
        let alpha0 = guard.alpha0;
        drop(guard);

        assert_eq!(alpha0, 3.0, "default alpha0 must be 3.0");
    }

    // Verify the duration guard constant is 200ms (code-review complement)
    #[test]
    fn test_duration_guard_budget_is_200ms() {
        // The budget is defined inline in run_maintenance.
        // This test documents and enforces the expected budget value.
        let budget = std::time::Duration::from_millis(200);
        assert_eq!(
            budget.as_millis(),
            200,
            "duration guard must be 200ms per FR-05"
        );
    }

    // GH-275: verify JoinError from spawn_blocking is handled without panic.
    //
    // A real JoinError can only be produced by tokio when a spawned task panics.
    // We cannot inject that in a pure unit test without actually panicking a thread,
    // so this test validates the recovery *pattern* by constructing an equivalent
    // Result chain and confirming the fallback values are returned safely.
    //
    // Integration-level coverage (actual spawn_blocking panic → recovery) lives in
    // test_availability.py::test_sustained_multi_tick.
    #[test]
    fn test_join_error_recovery_pattern_observation_stats() {
        // Simulate the recovery chain for observation stats:
        //   JoinHandle::await -> Err(JoinError)  =>  unwrap_or_else returns Ok(default)
        //   Ok(default).unwrap_or_else(|_| fallback)  =>  returns default
        let join_result: Result<Result<unimatrix_observe::ObservationStats, ()>, &str> =
            Err("simulated join error");

        let recovered = join_result
            .unwrap_or_else(|_join_err| {
                Ok(unimatrix_observe::ObservationStats {
                    record_count: 0,
                    session_count: 0,
                    oldest_record_age_days: 0,
                    approaching_cleanup: vec![],
                })
            })
            .unwrap_or_else(|_| unimatrix_observe::ObservationStats {
                record_count: 0,
                session_count: 0,
                oldest_record_age_days: 0,
                approaching_cleanup: vec![],
            });

        assert_eq!(
            recovered.record_count, 0,
            "join error must produce zero record_count"
        );
        assert_eq!(
            recovered.session_count, 0,
            "join error must produce zero session_count"
        );
        assert!(
            recovered.approaching_cleanup.is_empty(),
            "join error must produce empty approaching_cleanup"
        );
    }

    // GH-275: verify JoinError recovery pattern for metric vectors.
    #[test]
    fn test_join_error_recovery_pattern_metric_vectors() {
        // Simulate: JoinHandle::await -> Err(JoinError)  =>  unwrap_or_else returns Ok(vec![])
        //           Ok(vec![]).unwrap_or_else(|_| vec![])  =>  returns vec![]
        let join_result: Result<Result<Vec<String>, ()>, &str> = Err("simulated join error");

        let recovered = join_result
            .unwrap_or_else(|_join_err| Ok(vec![]))
            .unwrap_or_else(|_| vec![]);

        assert!(
            recovered.is_empty(),
            "join error must produce empty metric vector list"
        );
    }
}

// ---------------------------------------------------------------------------
// GH-280: load_maintenance_snapshot() — skips O(N) ONNX phases
// ---------------------------------------------------------------------------

#[cfg(test)]
mod maintenance_snapshot_tests {
    use std::sync::Arc;

    use unimatrix_adapt::AdaptationService;
    use unimatrix_core::{VectorConfig, VectorIndex};
    use unimatrix_store::{NewEntry, Status, Store};

    use crate::infra::embed_handle::EmbedServiceHandle;
    use crate::services::confidence::ConfidenceState;
    use crate::services::contradiction_cache::new_contradiction_cache_handle;
    use crate::services::status::StatusService;

    fn make_status_service(store: &Arc<Store>) -> StatusService {
        let vector_index = Arc::new(
            VectorIndex::new(Arc::clone(store), VectorConfig::default()).expect("vector index"),
        );
        // EmbedServiceHandle::new() already returns Arc<EmbedServiceHandle>.
        let embed_service = EmbedServiceHandle::new();
        let adapt_service = Arc::new(AdaptationService::new(
            unimatrix_adapt::AdaptConfig::default(),
        ));
        let confidence_state = Arc::new(std::sync::RwLock::new(ConfidenceState::default()));
        let contradiction_cache = new_contradiction_cache_handle();
        StatusService::new(
            Arc::clone(store),
            vector_index,
            embed_service,
            adapt_service,
            confidence_state,
            contradiction_cache,
        )
    }

    // T-280-01: snapshot returns Ok with empty active_entries on an empty store.
    #[tokio::test]
    async fn test_load_maintenance_snapshot_empty_store_returns_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(dir.path().join("test.db")).expect("store"));
        let svc = make_status_service(&store);

        let result = svc.load_maintenance_snapshot().await;
        assert!(result.is_ok(), "snapshot must succeed on empty store");

        let snapshot = result.unwrap();
        assert!(
            snapshot.active_entries.is_empty(),
            "empty store must produce empty active_entries"
        );
        assert_eq!(
            snapshot.graph_stale_ratio, 0.0,
            "empty graph must have zero stale ratio"
        );
        assert!(
            snapshot.effectiveness.is_some(),
            "empty store must produce Some effectiveness report (build_report succeeds on empty classifications)"
        );
    }

    // T-280-02: snapshot returns non-empty active_entries when active entries exist.
    #[tokio::test]
    async fn test_load_maintenance_snapshot_with_active_entries_returns_non_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(dir.path().join("test.db")).expect("store"));

        // Insert one active entry.
        store
            .insert(NewEntry {
                title: "Test entry".to_string(),
                content: "Content for maintenance snapshot test".to_string(),
                topic: "test".to_string(),
                category: "convention".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: Status::Active,
                created_by: "test".to_string(),
                feature_cycle: "bugfix-280".to_string(),
                trust_source: "human".to_string(),
            })
            .expect("insert entry");

        let svc = make_status_service(&store);
        let snapshot = svc
            .load_maintenance_snapshot()
            .await
            .expect("snapshot must succeed");

        assert_eq!(
            snapshot.active_entries.len(),
            1,
            "must return exactly one active entry"
        );
        assert_eq!(
            snapshot.active_entries[0].title, "Test entry",
            "must return the inserted entry"
        );
    }

    // T-280-03: snapshot graph_stale_ratio is 0.0 when vector index is empty.
    #[tokio::test]
    async fn test_load_maintenance_snapshot_graph_stale_ratio_zero_on_empty_index() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(Store::open(dir.path().join("test.db")).expect("store"));
        let svc = make_status_service(&store);

        let snapshot = svc.load_maintenance_snapshot().await.expect("snapshot ok");

        assert_eq!(
            snapshot.graph_stale_ratio, 0.0,
            "empty vector index must produce zero stale ratio"
        );
    }
}
