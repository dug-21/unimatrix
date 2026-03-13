//! Background tick loop for automated maintenance and knowledge extraction (col-013).
//!
//! Spawns a tokio task that runs every 15 minutes (configurable):
//! 1. Maintenance tick: co-access cleanup, confidence refresh, graph compaction,
//!    observation retention, session GC.
//! 2. Extraction tick: runs extraction rules on new observations, quality-gates
//!    proposals, stores accepted entries.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use unimatrix_core::async_wrappers::AsyncEntryStore;
use unimatrix_core::{
    CoreError, EmbedService, NewEntry, Store, StoreAdapter, VectorAdapter, VectorIndex, VectorStore,
};
use unimatrix_learn::TrainingService;
use unimatrix_learn::models::{ConventionScorer, SignalClassifier};
use unimatrix_observe::extraction::neural::{EnhancerMode, NeuralEnhancer};
use unimatrix_observe::extraction::shadow::{ShadowEvaluator, ShadowLogEntry};
use unimatrix_observe::extraction::{
    ExtractionContext, ExtractionStats, ProposedEntry, QualityGateResult, default_extraction_rules,
    quality_gate, run_extraction_rules,
};
use unimatrix_observe::types::{HookType, ObservationRecord};
use unimatrix_store::rusqlite;

use unimatrix_adapt::AdaptationService;

use crate::infra::contradiction::{self, ContradictionConfig};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::session::SessionRegistry;
use crate::server::PendingEntriesAnalysis;
use crate::services::ServiceError;
use crate::services::status::StatusService;

/// Default tick interval: 15 minutes.
const TICK_INTERVAL_SECS: u64 = 900;

/// Shared tick metadata, read by context_status and written by the tick loop.
#[derive(Default)]
pub struct TickMetadata {
    /// Unix timestamp of last completed maintenance tick.
    pub last_maintenance_run: Option<u64>,
    /// Unix timestamp of last completed extraction tick.
    pub last_extraction_run: Option<u64>,
    /// Unix timestamp of next scheduled tick.
    pub next_scheduled: Option<u64>,
    /// Cumulative extraction statistics.
    pub extraction_stats: ExtractionStats,
}

impl TickMetadata {
    /// Create new metadata with no history.
    pub fn new() -> Self {
        Self::default()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Initialize the neural enhancer with baseline models.
///
/// Always starts in Shadow mode. Returns `None` only if initialization
/// fails catastrophically (which should not happen with baseline models).
pub fn init_neural_enhancer() -> Option<(NeuralEnhancer, ShadowEvaluator)> {
    let classifier = SignalClassifier::new_with_baseline();
    let scorer = ConventionScorer::new_with_baseline();
    let enhancer = NeuralEnhancer::new(classifier, scorer, EnhancerMode::Shadow);
    let evaluator = ShadowEvaluator::new(20, 0.05, 50);
    Some((enhancer, evaluator))
}

/// Persist shadow evaluation logs to the shadow_evaluations table.
fn persist_shadow_evaluations(store: &Store, logs: &[ShadowLogEntry]) {
    let conn = store.lock_conn();
    let mut stmt = match conn.prepare_cached(
        "INSERT INTO shadow_evaluations
         (timestamp, rule_name, rule_category, neural_category,
          neural_confidence, convention_score, rule_accepted, digest)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    ) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("failed to prepare shadow_evaluations insert: {e}");
            return;
        }
    };
    for log in logs {
        if let Err(e) = stmt.execute(rusqlite::params![
            log.timestamp as i64,
            log.rule_name,
            log.rule_category,
            log.neural_category,
            log.neural_confidence as f64,
            log.convention_score as f64,
            log.rule_accepted as i32,
            log.digest_bytes,
        ]) {
            tracing::warn!("failed to insert shadow evaluation: {e}");
        }
    }
}

/// Spawn the background tick loop. Call once at server startup.
///
/// Returns a JoinHandle that runs indefinitely (until server shutdown).
#[allow(clippy::too_many_arguments)]
pub fn spawn_background_tick(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: Arc<Mutex<TickMetadata>>,
    training_service: Option<Arc<TrainingService>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(background_tick_loop(
        store,
        vector_index,
        embed_service,
        adapt_service,
        session_registry,
        entry_store,
        pending_entries,
        tick_metadata,
        training_service,
    ))
}

/// Main tick loop: runs maintenance + extraction every TICK_INTERVAL_SECS.
#[allow(clippy::too_many_arguments)]
async fn background_tick_loop(
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    adapt_service: Arc<AdaptationService>,
    session_registry: Arc<SessionRegistry>,
    entry_store: Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: Arc<Mutex<TickMetadata>>,
    _training_service: Option<Arc<TrainingService>>,
) {
    let mut interval = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
    let mut extraction_ctx = ExtractionContext::new();

    // Initialize neural enhancer (crt-007: shadow mode)
    let (neural_enhancer, mut shadow_evaluator) = match init_neural_enhancer() {
        Some((e, s)) => {
            tracing::info!("neural enhancer initialized in shadow mode");
            (Some(e), Some(s))
        }
        None => {
            tracing::warn!("neural enhancer initialization failed, running rule-only");
            (None, None)
        }
    };

    // Skip the immediate first tick (fires at t=0).
    interval.tick().await;

    loop {
        interval.tick().await;

        // Wrap the entire tick body in a spawned task (vnc-010).
        // If any spawn_blocking panics, the JoinError is caught here
        // instead of killing the background tick loop silently.
        let tick_result = run_single_tick(
            &store,
            &vector_index,
            &embed_service,
            &adapt_service,
            &session_registry,
            &entry_store,
            &pending_entries,
            &tick_metadata,
            &mut extraction_ctx,
            neural_enhancer.as_ref(),
            shadow_evaluator.as_mut(),
        )
        .await;

        if let Err(e) = tick_result {
            tracing::error!("background tick failed: {e}; continuing to next tick");
        }
    }
}

/// Maximum duration for a single maintenance or extraction tick (#236).
///
/// If a tick exceeds this timeout, it is aborted and will retry next cycle.
/// The work is idempotent so no data is lost.
const TICK_TIMEOUT: Duration = Duration::from_secs(120);

/// Execute a single tick iteration with error recovery (vnc-010).
///
/// Catches panics from spawn_blocking tasks via JoinError and logs them
/// instead of propagating. Returns Err only for fatal issues.
///
/// Both maintenance and extraction ticks are wrapped in a 2-minute timeout (#236)
/// to prevent long-running DB operations from blocking MCP requests indefinitely.
#[allow(clippy::too_many_arguments)]
async fn run_single_tick(
    store: &Arc<Store>,
    vector_index: &Arc<VectorIndex>,
    embed_service: &Arc<EmbedServiceHandle>,
    adapt_service: &Arc<AdaptationService>,
    session_registry: &SessionRegistry,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: &Arc<Mutex<PendingEntriesAnalysis>>,
    tick_metadata: &Arc<Mutex<TickMetadata>>,
    extraction_ctx: &mut ExtractionContext,
    neural_enhancer: Option<&NeuralEnhancer>,
    shadow_evaluator: Option<&mut ShadowEvaluator>,
) -> Result<(), String> {
    let tick_start = now_secs();
    tracing::info!("background tick starting");

    // 1. Maintenance tick (with timeout, #236)
    let status_svc = StatusService::new(
        Arc::clone(store),
        Arc::clone(vector_index),
        Arc::clone(embed_service),
        Arc::clone(adapt_service),
    );
    match tokio::time::timeout(
        TICK_TIMEOUT,
        maintenance_tick(&status_svc, session_registry, entry_store, pending_entries),
    )
    .await
    {
        Ok(Ok(())) => {
            if let Ok(mut meta) = tick_metadata.lock() {
                meta.last_maintenance_run = Some(tick_start);
            }
            tracing::info!("maintenance tick complete");
        }
        Ok(Err(e)) => {
            tracing::warn!("maintenance tick failed: {}", e);
        }
        Err(_) => {
            tracing::warn!(
                timeout_secs = TICK_TIMEOUT.as_secs(),
                "maintenance tick timed out; will retry next cycle"
            );
        }
    }

    // 2. Extraction tick (with timeout, #236)
    match tokio::time::timeout(
        TICK_TIMEOUT,
        extraction_tick(
            store,
            vector_index,
            embed_service,
            extraction_ctx,
            neural_enhancer,
            shadow_evaluator,
        ),
    )
    .await
    {
        Ok(Ok(stats)) => {
            if let Ok(mut meta) = tick_metadata.lock() {
                meta.last_extraction_run = Some(now_secs());
                meta.extraction_stats = stats;
            }
            tracing::info!("extraction tick complete");
        }
        Ok(Err(e)) => {
            tracing::warn!("extraction tick failed: {}", e);
        }
        Err(_) => {
            tracing::warn!(
                timeout_secs = TICK_TIMEOUT.as_secs(),
                "extraction tick timed out; will retry next cycle"
            );
        }
    }

    // Update next scheduled time
    if let Ok(mut meta) = tick_metadata.lock() {
        meta.next_scheduled = Some(now_secs() + TICK_INTERVAL_SECS);
    }

    let duration = now_secs() - tick_start;
    tracing::info!(duration_secs = duration, "background tick complete");
    Ok(())
}

/// Run maintenance operations via StatusService.
async fn maintenance_tick(
    status_svc: &StatusService,
    session_registry: &SessionRegistry,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: &Arc<Mutex<PendingEntriesAnalysis>>,
) -> Result<(), ServiceError> {
    // Compute lightweight report to get active entries
    let (mut report, active_entries) = status_svc.compute_report(None, None, false).await?;

    // Run existing maintenance logic
    status_svc
        .run_maintenance(
            &active_entries,
            &mut report,
            session_registry,
            entry_store,
            pending_entries,
        )
        .await?;

    Ok(())
}

/// Parse a hook type string into a HookType enum.
fn parse_hook_type(s: &str) -> HookType {
    match s {
        "PreToolUse" => HookType::PreToolUse,
        "PostToolUse" => HookType::PostToolUse,
        "SubagentStart" => HookType::SubagentStart,
        "SubagentStop" => HookType::SubagentStop,
        _ => HookType::PreToolUse, // fallback
    }
}

/// Run extraction pipeline on new observations since last watermark.
#[allow(clippy::too_many_arguments)]
async fn extraction_tick(
    store: &Arc<Store>,
    vector_index: &Arc<VectorIndex>,
    embed_service: &Arc<EmbedServiceHandle>,
    ctx: &mut ExtractionContext,
    neural_enhancer: Option<&NeuralEnhancer>,
    shadow_evaluator: Option<&mut ShadowEvaluator>,
) -> Result<ExtractionStats, ServiceError> {
    let store_clone = Arc::clone(store);
    let watermark = ctx.last_watermark;

    // 1. Query new observations since watermark (spawn_blocking)
    let (observations, new_watermark) = tokio::task::spawn_blocking(move || {
        let conn = store_clone.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, ts_millis, hook, session_id, tool, input, response_size, response_snippet
             FROM observations WHERE id > ?1 ORDER BY id ASC LIMIT 10000",
        )
        .map_err(|e| ServiceError::Core(CoreError::Store(
            unimatrix_store::StoreError::Sqlite(e),
        )))?;

        let mut records = Vec::new();
        let mut max_id = watermark;
        let rows = stmt
            .query_map(rusqlite::params![watermark as i64], |row| {
                let id: i64 = row.get(0)?;
                let ts: i64 = row.get(1)?;
                let hook_str: String = row.get(2)?;
                let session_id: String = row.get(3)?;
                let tool: Option<String> = row.get(4)?;
                let input_str: Option<String> = row.get(5)?;
                let response_size: Option<i64> = row.get(6)?;
                let snippet: Option<String> = row.get(7)?;
                Ok((
                    id,
                    ts,
                    hook_str,
                    session_id,
                    tool,
                    input_str,
                    response_size,
                    snippet,
                ))
            })
            .map_err(|e| {
                ServiceError::Core(CoreError::Store(unimatrix_store::StoreError::Sqlite(e)))
            })?;

        for row in rows {
            let (id, ts, hook_str, session_id, tool, input_str, response_size, snippet) = row
                .map_err(|e| {
                    ServiceError::Core(CoreError::Store(unimatrix_store::StoreError::Sqlite(e)))
                })?;
            if id as u64 > max_id {
                max_id = id as u64;
            }
            let hook = parse_hook_type(&hook_str);
            let input = match (&hook, input_str) {
                (HookType::SubagentStart, Some(s)) => Some(serde_json::Value::String(s)),
                (_, Some(s)) => serde_json::from_str(&s).ok(),
                (_, None) => None,
            };
            records.push(ObservationRecord {
                ts: ts as u64,
                hook,
                session_id,
                tool,
                input,
                response_size: response_size.map(|s| s as u64),
                response_snippet: snippet,
            });
        }
        Ok::<(Vec<ObservationRecord>, u64), ServiceError>((records, max_id))
    })
    .await
    .map_err(|e| ServiceError::Core(CoreError::JoinError(e.to_string())))??;

    if observations.is_empty() {
        return Ok(ctx.stats.clone());
    }

    // 2. Run extraction rules
    let store_for_rules = Arc::clone(store);
    let obs_for_rules = observations;

    let proposals = tokio::task::spawn_blocking(move || {
        let rules = default_extraction_rules();
        run_extraction_rules(&obs_for_rules, &store_for_rules, &rules)
    })
    .await
    .map_err(|e| ServiceError::Core(CoreError::JoinError(e.to_string())))?;

    // 3. Quality gate (checks 1-4: cheap, in-memory)
    let mut accepted: Vec<ProposedEntry> = Vec::new();
    for proposal in proposals {
        match quality_gate(&proposal, ctx) {
            QualityGateResult::Accept => accepted.push(proposal),
            QualityGateResult::Reject { reason, check_name } => {
                tracing::debug!(
                    rule = %proposal.source_rule,
                    check = %check_name,
                    reason = %reason,
                    "extraction rejected"
                );
                ctx.stats.entries_rejected_total += 1;
            }
        }
    }

    // 3.5 Neural enhancement (crt-007: between quality gate checks 1-4 and 5-6)
    let accepted = if let (Some(enhancer), Some(evaluator)) = (neural_enhancer, shadow_evaluator) {
        let mut neural_accepted = Vec::new();
        for entry in accepted {
            let prediction = enhancer.enhance(&entry);

            match enhancer.mode() {
                EnhancerMode::Shadow => {
                    // Log prediction, pass entry unchanged
                    evaluator.log_prediction(&entry, &prediction, true);
                    neural_accepted.push(entry);
                }
                EnhancerMode::Active => {
                    // Suppress if classified as Noise with high confidence
                    if prediction.classification.category
                        == unimatrix_learn::models::SignalCategory::Noise
                        && prediction.classification.confidence > 0.8
                    {
                        evaluator.log_prediction(&entry, &prediction, false);
                        ctx.stats.entries_rejected_total += 1;
                        continue;
                    }
                    evaluator.log_prediction(&entry, &prediction, true);
                    neural_accepted.push(entry);
                }
            }
        }

        // Persist shadow evaluations (batch INSERT)
        let logs = evaluator.drain_evaluations();
        if !logs.is_empty() {
            let store_for_shadow = Arc::clone(store);
            let _ = tokio::task::spawn_blocking(move || {
                persist_shadow_evaluations(&store_for_shadow, &logs);
            })
            .await;
        }

        neural_accepted
    } else {
        accepted
    };

    // 4. Quality gate checks 5-6: near-duplicate + contradiction (need embedding)
    #[allow(clippy::collapsible_if)]
    if !accepted.is_empty() {
        if let Ok(adapter) = embed_service.get_adapter().await {
            let store_for_gate = Arc::clone(store);
            let vi_for_gate = Arc::clone(vector_index);

            let final_accepted = tokio::task::spawn_blocking(move || {
                let vs = VectorAdapter::new(vi_for_gate);
                let config = ContradictionConfig::default();
                let mut passed = Vec::new();

                for entry in accepted {
                    // Check 5: Near-duplicate via embedding similarity
                    let embedding = match adapter.embed_entry(&entry.title, &entry.content) {
                        Ok(v) => v,
                        Err(_) => {
                            passed.push(entry);
                            continue;
                        }
                    };
                    let neighbors = match vs.search(&embedding, 1, 32) {
                        Ok(n) => n,
                        Err(_) => {
                            passed.push(entry);
                            continue;
                        }
                    };
                    if neighbors.first().is_some_and(|top| top.similarity >= 0.92) {
                        // Near-duplicate, skip
                        continue;
                    }

                    // Check 6: Contradiction check
                    if let Ok(Some(_)) = contradiction::check_entry_contradiction(
                        &entry.content,
                        &entry.title,
                        &store_for_gate,
                        &vs,
                        &*adapter,
                        &config,
                    ) {
                        continue; // contradiction detected, skip
                    }

                    passed.push(entry);
                }
                passed
            })
            .await
            .map_err(|e| ServiceError::Core(CoreError::JoinError(e.to_string())))?;

            // 5. Store accepted entries
            let store_for_insert = Arc::clone(store);
            for entry in final_accepted {
                let rule_name = entry.source_rule.clone();
                let source_features_str = entry.source_features.join(",");

                // trust_source: "neural" when Active mode, else "auto"
                let trust_source = match neural_enhancer {
                    Some(e) if e.mode() == EnhancerMode::Active => "neural",
                    _ => "auto",
                };
                let new_entry = NewEntry {
                    title: entry.title,
                    content: entry.content,
                    topic: entry.topic,
                    category: entry.category,
                    tags: vec![
                        "auto-extracted".to_string(),
                        format!("rule:{}", rule_name),
                        format!("source-features:{}", source_features_str),
                    ],
                    source: "auto".to_string(),
                    status: unimatrix_core::Status::Active,
                    created_by: "background-tick".to_string(),
                    feature_cycle: String::new(),
                    trust_source: trust_source.to_string(),
                };

                let store_for_entry = Arc::clone(&store_for_insert);
                match tokio::task::spawn_blocking(move || store_for_entry.insert(new_entry)).await {
                    Ok(Ok(id)) => {
                        tracing::info!(entry_id = id, rule = %rule_name, "auto-extracted entry stored");
                        ctx.stats.entries_extracted_total += 1;
                        *ctx.stats.rules_fired.entry(rule_name).or_insert(0) += 1;
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(rule = %rule_name, error = %e, "failed to store extracted entry");
                    }
                    Err(e) => {
                        tracing::warn!(rule = %rule_name, error = %e, "store task panicked");
                    }
                }
            }
        }
    }

    // 6. Update watermark
    ctx.last_watermark = new_watermark;
    ctx.stats.last_extraction_run = Some(now_secs());

    Ok(ctx.stats.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tick_metadata_new_defaults() {
        let meta = TickMetadata::new();
        assert!(meta.last_maintenance_run.is_none());
        assert!(meta.last_extraction_run.is_none());
        assert!(meta.next_scheduled.is_none());
        assert_eq!(meta.extraction_stats.entries_extracted_total, 0);
        assert_eq!(meta.extraction_stats.entries_rejected_total, 0);
        assert!(meta.extraction_stats.last_extraction_run.is_none());
        assert!(meta.extraction_stats.rules_fired.is_empty());
    }

    #[test]
    fn parse_hook_type_variants() {
        assert!(matches!(
            parse_hook_type("PreToolUse"),
            HookType::PreToolUse
        ));
        assert!(matches!(
            parse_hook_type("PostToolUse"),
            HookType::PostToolUse
        ));
        assert!(matches!(
            parse_hook_type("SubagentStart"),
            HookType::SubagentStart
        ));
        assert!(matches!(
            parse_hook_type("SubagentStop"),
            HookType::SubagentStop
        ));
        assert!(matches!(parse_hook_type("Unknown"), HookType::PreToolUse));
    }

    #[test]
    fn now_secs_returns_reasonable_value() {
        let ts = now_secs();
        // Should be after 2024-01-01 (1704067200)
        assert!(ts > 1_704_067_200);
    }

    #[test]
    fn init_neural_enhancer_returns_some() {
        let result = init_neural_enhancer();
        assert!(result.is_some());
        let (enhancer, _evaluator) = result.unwrap();
        assert_eq!(enhancer.mode(), EnhancerMode::Shadow);
    }
}
