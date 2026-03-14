//! StatusService: transport-agnostic status computation (vnc-008).
//!
//! Rewritten for nxs-008: direct SQL queries replace compat layer.
//! Uses SQL aggregation where possible.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

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
use crate::mcp::response::status::{CoAccessClusterEntry, StatusReport};
use crate::server::PendingEntriesAnalysis;
use crate::services::ServiceError;
use crate::services::confidence::ConfidenceStateHandle;

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
    #[allow(dead_code)]
    confidence_state: ConfidenceStateHandle,
}

/// Result of maintenance operations.
#[allow(dead_code)]
pub(crate) struct MaintenanceResult {
    pub confidence_refreshed: u64,
    pub graph_compacted: bool,
    pub stale_pairs_cleaned: u64,
}

impl StatusService {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        embed_service: Arc<EmbedServiceHandle>,
        adapt_service: Arc<AdaptationService>,
        confidence_state: ConfidenceStateHandle,
    ) -> Self {
        StatusService {
            store,
            vector_index,
            embed_service,
            adapt_service,
            confidence_state,
        }
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
        // Phase 1: SQL queries (spawn_blocking)
        let store = Arc::clone(&self.store);
        let report_result = tokio::task::spawn_blocking(
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
        .map_err(|e| ServiceError::Core(CoreError::JoinError(e.to_string())))?
        .map_err(|e| {
            let core_err: CoreError = match e {
                crate::error::ServerError::Core(ce) => ce,
                other => CoreError::JoinError(other.to_string()),
            };
            ServiceError::Core(core_err)
        })?;
        let (mut report, active_entries) = report_result;

        // Phase 2: Contradiction scanning (outside read txn)
        if let Ok(adapter) = self.embed_service.get_adapter().await {
            let scan_config = contradiction::ContradictionConfig::default();
            let store_for_scan = Arc::clone(&self.store);
            let vi_for_scan = Arc::clone(&self.vector_index);
            let adapter_for_scan = Arc::clone(&adapter);
            let config_for_scan = scan_config.clone();

            match tokio::task::spawn_blocking(move || {
                let vs = VectorAdapter::new(vi_for_scan);
                contradiction::scan_contradictions(
                    &store_for_scan,
                    &vs,
                    &*adapter_for_scan,
                    &config_for_scan,
                )
            })
            .await
            {
                Ok(Ok(contradictions)) => {
                    report.contradiction_count = contradictions.len();
                    report.contradictions = contradictions;
                    report.contradiction_scan_performed = true;
                }
                _ => {
                    // Scan failed -- graceful degradation
                }
            }

            // Phase 3: Embedding consistency (opt-in)
            if check_embeddings {
                let store_for_embed = Arc::clone(&self.store);
                let vi_for_embed = Arc::clone(&self.vector_index);
                let adapter_for_embed = Arc::clone(&adapter);
                let config_for_embed = contradiction::ContradictionConfig::default();

                match tokio::task::spawn_blocking(move || {
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
                        // Check failed -- graceful degradation
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
            let co_access_result = tokio::task::spawn_blocking(move || {
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
                    tracing::warn!("co-access stats task failed: {e}");
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
        let obs_stats = tokio::task::spawn_blocking(move || {
            use unimatrix_observe::ObservationSource;
            let source = crate::services::observation::SqlObservationSource::new(store_for_obs);
            source.observation_stats()
        })
        .await
        .unwrap()
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
        let retrospected = tokio::task::spawn_blocking({
            let store = Arc::clone(&self.store);
            move || store.list_all_metrics()
        })
        .await
        .unwrap()
        .unwrap_or_else(|_| vec![]);
        report.retrospected_feature_count = retrospected.len() as u64;

        // Phase 8: Effectiveness analysis (crt-018)
        let store_for_eff = Arc::clone(&self.store);
        let effectiveness = match tokio::task::spawn_blocking(move || {
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
            Err(join_err) => {
                tracing::warn!("Effectiveness task panicked: {join_err}");
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
    /// 2. Confidence refresh (batch 100)
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

        // 2. Confidence refresh (batch 100)
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
                let ids_and_confs: Vec<(u64, f64)> = stale_entries
                    .iter()
                    .map(|e| (e.id, crate::confidence::compute_confidence(e, now_ts)))
                    .collect();

                let store_for_refresh = Arc::clone(&self.store);
                let refresh_result = tokio::task::spawn_blocking(move || {
                    let mut refreshed = 0u64;
                    for (id, new_conf) in ids_and_confs {
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
