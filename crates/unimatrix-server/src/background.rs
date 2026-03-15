//! Background tick loop for automated maintenance and knowledge extraction (col-013).
//!
//! Spawns a tokio task that runs every 15 minutes (configurable):
//! 1. Maintenance tick: co-access cleanup, confidence refresh, graph compaction,
//!    observation retention, session GC.
//! 2. Extraction tick: runs extraction rules on new observations, quality-gates
//!    proposals, stores accepted entries.

use std::collections::HashSet;
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
use unimatrix_store::Status;
use unimatrix_store::rusqlite;

use unimatrix_adapt::AdaptationService;

use crate::infra::audit::{AuditEvent, AuditLog, Outcome};
use crate::infra::contradiction::{self, ContradictionConfig};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::session::SessionRegistry;
use crate::server::PendingEntriesAnalysis;
use crate::services::ServiceError;
use crate::services::confidence::ConfidenceStateHandle;
use crate::services::effectiveness::EffectivenessStateHandle;
use crate::services::status::StatusService;
use crate::services::supersession::{SupersessionState, SupersessionStateHandle};
use unimatrix_engine::effectiveness::EffectivenessCategory;

/// Hardcoded system agent identity for background-generated audit events.
/// Never sourced from external input (Security Risk 2 from RISK-TEST-STRATEGY).
const SYSTEM_AGENT_ID: &str = "system";
const OP_AUTO_QUARANTINE: &str = "auto_quarantine";
const OP_TICK_SKIPPED: &str = "tick_skipped";

/// Upper bound on `UNIMATRIX_AUTO_QUARANTINE_CYCLES`.
/// Values above this are implausibly large and rejected at startup (Constraint 14).
const AUTO_QUARANTINE_CYCLES_MAX: u32 = 1000;

/// Default tick interval: 15 minutes.
const DEFAULT_TICK_INTERVAL_SECS: u64 = 900;

/// Parse a tick interval string as a `u64`, returning the default on any error.
///
/// Extracted for testability — avoids unsafe env var manipulation in tests.
fn parse_tick_interval_str(s: &str) -> u64 {
    match s.trim().parse::<u64>() {
        Ok(secs) => secs,
        Err(_) => DEFAULT_TICK_INTERVAL_SECS,
    }
}

/// Read the tick interval from `UNIMATRIX_TICK_INTERVAL_SECS` env var.
///
/// Falls back to `DEFAULT_TICK_INTERVAL_SECS` (900s) if the variable is unset
/// or contains a value that cannot be parsed as a `u64`.
fn read_tick_interval() -> u64 {
    match std::env::var("UNIMATRIX_TICK_INTERVAL_SECS") {
        Ok(val) => {
            let secs = parse_tick_interval_str(&val);
            if secs == DEFAULT_TICK_INTERVAL_SECS && val.trim() != "900" {
                tracing::warn!(
                    value = %val,
                    default = DEFAULT_TICK_INTERVAL_SECS,
                    "UNIMATRIX_TICK_INTERVAL_SECS is not a valid u64; using default"
                );
            } else {
                tracing::info!(
                    secs,
                    "tick interval configured via UNIMATRIX_TICK_INTERVAL_SECS"
                );
            }
            secs
        }
        Err(_) => DEFAULT_TICK_INTERVAL_SECS,
    }
}

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

/// Parse and validate `UNIMATRIX_AUTO_QUARANTINE_CYCLES` from the environment.
///
/// - Default: 3 (auto-quarantine after 3 consecutive bad ticks, ~45 min minimum).
/// - Value 0: auto-quarantine disabled (valid).
/// - Value > 1000: startup error (implausibly large, DoS mitigation, Constraint 14).
/// - Non-integer: startup error.
pub fn parse_auto_quarantine_cycles() -> Result<u32, String> {
    let raw = std::env::var("UNIMATRIX_AUTO_QUARANTINE_CYCLES").unwrap_or_else(|_| "3".to_string());
    parse_auto_quarantine_cycles_str(&raw)
}

/// Inner parse logic for `parse_auto_quarantine_cycles`, separated for unit-testability.
///
/// Takes the raw env var string value (or the default "3") and validates it.
/// This function avoids the need for `unsafe` env var manipulation in tests.
fn parse_auto_quarantine_cycles_str(raw: &str) -> Result<u32, String> {
    let value: u32 = raw.parse().map_err(|_| {
        format!(
            "UNIMATRIX_AUTO_QUARANTINE_CYCLES: must be a non-negative integer, got {:?}",
            raw
        )
    })?;

    if value > AUTO_QUARANTINE_CYCLES_MAX {
        return Err(format!(
            "UNIMATRIX_AUTO_QUARANTINE_CYCLES: value {} > {} is implausibly large; \
             set to 0 to disable or use a value in [1, {}]",
            value, AUTO_QUARANTINE_CYCLES_MAX, AUTO_QUARANTINE_CYCLES_MAX
        ));
    }

    Ok(value)
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
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle, // crt-018b: shared with search/briefing paths
    supersession_state: SupersessionStateHandle,   // GH #264: shared with SearchService
    audit_log: Arc<AuditLog>,
    auto_quarantine_cycles: u32,
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
        confidence_state,
        effectiveness_state,
        supersession_state,
        audit_log,
        auto_quarantine_cycles,
    ))
}

/// Main tick loop: runs maintenance + extraction at the configured tick interval.
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
    confidence_state: ConfidenceStateHandle,
    effectiveness_state: EffectivenessStateHandle, // crt-018b: threaded to run_single_tick
    supersession_state: SupersessionStateHandle,   // GH #264: threaded to run_single_tick
    audit_log: Arc<AuditLog>,
    auto_quarantine_cycles: u32,
) {
    let tick_interval_secs = read_tick_interval();
    let mut interval = tokio::time::interval(Duration::from_secs(tick_interval_secs));
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
            &confidence_state,
            &effectiveness_state,
            &supersession_state,
            &audit_log,
            auto_quarantine_cycles,
            tick_interval_secs,
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
    confidence_state: &ConfidenceStateHandle,
    effectiveness_state: &EffectivenessStateHandle,
    supersession_state: &SupersessionStateHandle, // GH #264: rebuild each tick
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    tick_interval_secs: u64, // nan-006: configurable via UNIMATRIX_TICK_INTERVAL_SECS
) -> Result<(), String> {
    let tick_start = now_secs();
    tracing::info!("background tick starting");

    // 1. Maintenance tick (with timeout, #236)
    let status_svc = StatusService::new(
        Arc::clone(store),
        Arc::clone(vector_index),
        Arc::clone(embed_service),
        Arc::clone(adapt_service),
        Arc::clone(confidence_state),
    );
    match tokio::time::timeout(
        TICK_TIMEOUT,
        maintenance_tick(
            &status_svc,
            session_registry,
            entry_store,
            pending_entries,
            effectiveness_state,
            audit_log,
            auto_quarantine_cycles,
            store,
        ),
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

    // GH #264 fix: Rebuild supersession graph cache after maintenance tick completes.
    // Runs inside spawn_blocking because Store uses Mutex<Connection> (sync).
    // Wrapped in TICK_TIMEOUT (GH #266) so an abandoned blocking thread cannot
    // hold the mutex indefinitely and block MCP handler spawn_blocking calls.
    // On timeout the existing cached state is retained (guard is not updated).
    {
        let store_clone = Arc::clone(store);
        match tokio::time::timeout(
            TICK_TIMEOUT,
            tokio::task::spawn_blocking(move || SupersessionState::rebuild(&store_clone)),
        )
        .await
        {
            Ok(Ok(Ok(new_state))) => {
                let mut guard = supersession_state
                    .write()
                    .unwrap_or_else(|e| e.into_inner());
                *guard = new_state;
                tracing::debug!(
                    "supersession state rebuilt ({} entries)",
                    guard.all_entries.len()
                );
            }
            Ok(Ok(Err(e))) => {
                tracing::error!("supersession state rebuild failed: {e}");
            }
            Ok(Err(e)) => {
                tracing::error!("supersession state rebuild task panicked: {e}");
            }
            Err(_) => {
                tracing::warn!(
                    timeout_secs = TICK_TIMEOUT.as_secs(),
                    "supersession state rebuild timed out; retaining existing cache"
                );
            }
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
        meta.next_scheduled = Some(now_secs() + tick_interval_secs);
    }

    let duration = now_secs() - tick_start;
    tracing::info!(duration_secs = duration, "background tick complete");
    Ok(())
}

/// Run maintenance operations via StatusService.
///
/// On `compute_report()` success: writes classification data to `EffectivenessState`,
/// scans for auto-quarantine threshold, and runs existing maintenance operations.
///
/// On `compute_report()` failure: emits a `tick_skipped` audit event and returns `Err`
/// without touching `EffectivenessState` (ADR-002 hold semantics, R-08).
#[allow(clippy::too_many_arguments)]
async fn maintenance_tick(
    status_svc: &StatusService,
    session_registry: &SessionRegistry,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: &Arc<Mutex<PendingEntriesAnalysis>>,
    effectiveness_state: &EffectivenessStateHandle,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
    store: &Arc<Store>,
) -> Result<(), ServiceError> {
    // Step 1: Attempt to compute the status report.
    let result = status_svc.compute_report(None, None, false).await;

    let (mut report, active_entries) = match result {
        Err(error) => {
            // ADR-002: hold semantics — do NOT modify EffectivenessState on error.
            // Emit tick_skipped audit event so operators can observe paused auto-quarantine.
            emit_tick_skipped_audit(audit_log, error.to_string());
            return Err(error);
        }
        Ok(pair) => pair,
    };

    // Step 2: Extract EffectivenessReport and update EffectivenessState if present.
    // `to_quarantine` is collected inside the write lock; SQL is called after lock release
    // (NFR-02, R-13).
    if report.effectiveness.is_some() {
        // SAFETY: checked is_some() above; unwrap is safe.
        let effectiveness_report = report.effectiveness.as_ref().unwrap();

        // Steps 3–8: Acquire write lock, update state, collect quarantine candidates,
        // then DROP the write lock (R-13, NFR-02).
        // The write guard goes out of scope at the closing `}` below — no store call inside.
        let to_quarantine: Vec<(u64, u32, EffectivenessCategory)> = {
            let mut state = effectiveness_state
                .write()
                .unwrap_or_else(|e| e.into_inner());

            // Step 4: Build new categories map from all_entries (full per-entry slice).
            let new_categories: std::collections::HashMap<u64, EffectivenessCategory> =
                effectiveness_report
                    .all_entries
                    .iter()
                    .map(|ee| (ee.entry_id, ee.category))
                    .collect();

            // Step 5: Replace categories map.
            state.categories = new_categories;

            // Step 6: Remove counters for entries no longer in the active classification
            // set (quarantined, deprecated, or deleted since last tick).
            let active_ids: HashSet<u64> = state.categories.keys().copied().collect();
            state
                .consecutive_bad_cycles
                .retain(|id, _| active_ids.contains(id));

            // Step 6b: Increment or reset per-entry consecutive_bad_cycles.
            // Collect updates first to satisfy the borrow checker — iterating over
            // &state.categories and mutating &mut state.consecutive_bad_cycles in the
            // same loop body triggers E0502 (simultaneous immutable + mutable borrow).
            let mut to_increment: Vec<u64> = Vec::new();
            let mut to_reset: Vec<u64> = Vec::new();
            for (&entry_id, category) in &state.categories {
                match category {
                    EffectivenessCategory::Ineffective | EffectivenessCategory::Noisy => {
                        to_increment.push(entry_id);
                    }
                    EffectivenessCategory::Effective
                    | EffectivenessCategory::Settled
                    | EffectivenessCategory::Unmatched => {
                        to_reset.push(entry_id);
                    }
                }
            }
            // Apply increments (FR-09).
            for entry_id in to_increment {
                let counter = state.consecutive_bad_cycles.entry(entry_id).or_insert(0);
                *counter += 1;
            }
            // Reset counters on recovery; remove to keep map sparse (FR-09).
            for entry_id in to_reset {
                state.consecutive_bad_cycles.remove(&entry_id);
            }

            // Step 7: Collect entries that cross the auto-quarantine threshold.
            // Scan happens INSIDE write lock (counters already updated).
            // SQL writes happen AFTER lock is released (NFR-02, R-13).
            let mut candidates = Vec::new();
            if auto_quarantine_cycles > 0 {
                for (&entry_id, &count) in &state.consecutive_bad_cycles {
                    if count >= auto_quarantine_cycles {
                        let category = state
                            .categories
                            .get(&entry_id)
                            .copied()
                            .unwrap_or(EffectivenessCategory::Ineffective);
                        // Defensive check: only Ineffective or Noisy qualify (AC-14, R-11).
                        match category {
                            EffectivenessCategory::Ineffective | EffectivenessCategory::Noisy => {
                                candidates.push((entry_id, count, category));
                            }
                            _ => {
                                // Counter was reset in Step 6b for non-bad categories.
                                // This branch is unreachable in practice (defensive guard).
                            }
                        }
                    }
                }
            }

            // Step 8: Increment generation counter.
            state.generation += 1;

            // Write lock drops here (end of block scope).
            // CRITICAL: No store calls may be made inside this block (NFR-02, R-13).
            candidates
        };
        // EffectivenessState write guard is now out of scope — lock is released.

        // Step 9: Auto-quarantine SQL writes (write lock is NOT held).
        let quarantined_ids = process_auto_quarantine(
            to_quarantine,
            effectiveness_state,
            effectiveness_report,
            store,
            audit_log,
            auto_quarantine_cycles,
        )
        .await;

        // Populate auto_quarantined_this_cycle on the report (FR-14).
        if let Some(ref mut eff_report) = report.effectiveness {
            eff_report.auto_quarantined_this_cycle = quarantined_ids;
        }
    }

    // Step 10: Run existing maintenance logic (unchanged).
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

/// Process the auto-quarantine candidates collected during the tick write.
///
/// Called after the `EffectivenessState` write lock has been released (NFR-02, R-13).
/// Each candidate is quarantined independently — failure of one does not abort
/// the remaining candidates (R-03).
///
/// Returns the list of entry IDs successfully quarantined this cycle (FR-14).
async fn process_auto_quarantine(
    to_quarantine: Vec<(u64, u32, EffectivenessCategory)>,
    effectiveness_state: &EffectivenessStateHandle,
    effectiveness_report: &unimatrix_engine::effectiveness::EffectivenessReport,
    store: &Arc<Store>,
    audit_log: &Arc<AuditLog>,
    auto_quarantine_cycles: u32,
) -> Vec<u64> {
    if to_quarantine.is_empty() || auto_quarantine_cycles == 0 {
        return Vec::new();
    }

    let mut quarantined: Vec<u64> = Vec::new();

    for (entry_id, cycle_count, category) in to_quarantine {
        // Defense in depth: verify the category is still Ineffective or Noisy (AC-14, R-11).
        // Background tick is the sole writer; no state change can have occurred since we
        // released the write lock above (still in the same tick invocation).
        match category {
            EffectivenessCategory::Ineffective | EffectivenessCategory::Noisy => { /* proceed */ }
            _ => continue, // stale entry — defensive skip
        }

        // Fetch entry metadata for audit event (title, topic).
        let (title, topic, entry_category) =
            find_entry_metadata_in_report(effectiveness_report, entry_id).unwrap_or_else(|| {
                (
                    format!("(id={})", entry_id),
                    "(unknown)".to_string(),
                    "(unknown)".to_string(),
                )
            });

        // Quarantine via synchronous store path inside spawn_blocking (NFR-05).
        // `Store::update_status` is the synchronous quarantine primitive.
        let store_clone = Arc::clone(store);
        let quarantine_result = tokio::task::spawn_blocking(move || {
            store_clone
                .update_status(entry_id, Status::Quarantined)
                .map_err(|e| e.to_string())
        })
        .await;

        match quarantine_result {
            Ok(Ok(())) => {
                // Quarantine succeeded — reset consecutive_bad_cycles counter (idempotent).
                // Re-acquire write lock for counter reset only.
                // `generation` is NOT incremented — search/briefing paths do not need to
                // re-clone categories for a counter-only change.
                {
                    let mut state = effectiveness_state
                        .write()
                        .unwrap_or_else(|e| e.into_inner());
                    state.consecutive_bad_cycles.remove(&entry_id);
                }
                // write lock drops here.

                // Emit auto_quarantine audit event (Component 6, FR-11).
                emit_auto_quarantine_audit(
                    audit_log,
                    entry_id,
                    &title,
                    &topic,
                    &entry_category,
                    category,
                    cycle_count,
                    auto_quarantine_cycles,
                );

                quarantined.push(entry_id);
            }
            Ok(Err(store_error)) => {
                // Quarantine SQL failed (e.g., entry already quarantined or deleted).
                // Do NOT reset counter — entry may still qualify next tick.
                // Do NOT abort loop — continue to next candidate (R-03).
                tracing::warn!(
                    entry_id = entry_id,
                    error = %store_error,
                    "auto-quarantine: update_status failed, skipping entry"
                );
            }
            Err(join_error) => {
                // spawn_blocking panicked.
                tracing::warn!(
                    entry_id = entry_id,
                    error = %join_error,
                    "auto-quarantine: spawn_blocking join error, skipping entry"
                );
            }
        }
    }

    quarantined
}

/// Look up entry title, topic, and category string from the `EffectivenessReport`.
///
/// Searches `top_ineffective` and `noisy_entries` lists. Entries that crossed the
/// quarantine threshold must be `Ineffective` or `Noisy`, so these lists are
/// sufficient. Returns `None` if the entry is not found in either list.
fn find_entry_metadata_in_report(
    report: &unimatrix_engine::effectiveness::EffectivenessReport,
    entry_id: u64,
) -> Option<(String, String, String)> {
    report
        .top_ineffective
        .iter()
        .chain(report.noisy_entries.iter())
        .find(|ee| ee.entry_id == entry_id)
        .map(|ee| {
            (
                ee.title.clone(),
                ee.topic.clone(),
                // `trust_source` is the closest available field for category context
                // (knowledge category is not stored on EntryEffectiveness).
                ee.trust_source.clone(),
            )
        })
}

/// Emit a `tick_skipped` audit event when `compute_report()` returns an error.
///
/// The error reason flows through to the audit log so operators can understand
/// why auto-quarantine logic was paused (ADR-002, FR-13, SR-07).
fn emit_tick_skipped_audit(audit_log: &Arc<AuditLog>, error_reason: String) {
    let event = AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: SYSTEM_AGENT_ID.to_string(),
        operation: OP_TICK_SKIPPED.to_string(),
        target_ids: vec![],
        outcome: Outcome::Error,
        detail: format!("background tick compute_report failed: {}", error_reason),
    };

    if let Err(e) = audit_log.log_event(event) {
        tracing::warn!(error = %e, "failed to emit tick_skipped audit event");
    }
}

/// Emit an `auto_quarantine` audit event for a successfully quarantined entry.
///
/// All 9 FR-11 fields are encoded: operation, agent_id, target_ids, entry_title,
/// entry_category, classification, consecutive_cycles, threshold, reason.
#[allow(clippy::too_many_arguments)]
fn emit_auto_quarantine_audit(
    audit_log: &Arc<AuditLog>,
    entry_id: u64,
    title: &str,
    topic: &str,
    entry_category: &str,
    classification: EffectivenessCategory,
    consecutive_cycles: u32,
    threshold: u32,
) {
    let reason = format!(
        "auto-quarantine: entry '{}' (id={}, category={:?}, \
         consecutive_bad_cycles={}, topic={}) quarantined after {} consecutive \
         background maintenance ticks classified as {:?}",
        title,
        entry_id,
        classification,
        consecutive_cycles,
        topic,
        consecutive_cycles,
        classification
    );

    let detail = format!(
        "entry_title={:?} entry_category={:?} classification={:?} \
         consecutive_cycles={} threshold={} reason={}",
        title, entry_category, classification, consecutive_cycles, threshold, reason
    );

    let event = AuditEvent {
        event_id: 0,
        timestamp: 0,
        session_id: String::new(),
        agent_id: SYSTEM_AGENT_ID.to_string(),
        operation: OP_AUTO_QUARANTINE.to_string(),
        target_ids: vec![entry_id],
        outcome: Outcome::Success,
        detail,
    };

    if let Err(e) = audit_log.log_event(event) {
        tracing::warn!(
            entry_id = entry_id,
            error = %e,
            "auto-quarantine: failed to write audit event"
        );
        // Do not escalate — quarantine succeeded even if audit write fails.
    }
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
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};
    use unimatrix_engine::effectiveness::EffectivenessCategory;

    use crate::services::effectiveness::EffectivenessState;

    // ---------------------------------------------------------------------------
    // Legacy tests (preserved)
    // ---------------------------------------------------------------------------

    // ---------------------------------------------------------------------------
    // UNIMATRIX_TICK_INTERVAL_SECS parsing tests (nan-006)
    // Tests target parse_tick_interval_str() to avoid env var mutation
    // (forbidden by #![forbid(unsafe_code)] on std::env::set_var in Rust 1.81+).
    // ---------------------------------------------------------------------------

    #[test]
    fn parse_tick_interval_str_default_value() {
        assert_eq!(
            parse_tick_interval_str("900"),
            900,
            "default value string must parse correctly"
        );
    }

    #[test]
    fn parse_tick_interval_str_custom_value() {
        assert_eq!(parse_tick_interval_str("30"), 30, "should parse '30' as 30");
    }

    #[test]
    fn parse_tick_interval_str_invalid_falls_back() {
        assert_eq!(
            parse_tick_interval_str("not-a-number"),
            DEFAULT_TICK_INTERVAL_SECS,
            "invalid string must fall back to 900"
        );
    }

    #[test]
    fn parse_tick_interval_str_empty_falls_back() {
        assert_eq!(
            parse_tick_interval_str(""),
            DEFAULT_TICK_INTERVAL_SECS,
            "empty string must fall back to 900"
        );
    }

    #[test]
    fn parse_tick_interval_str_whitespace_value() {
        assert_eq!(
            parse_tick_interval_str("  60  "),
            60,
            "whitespace-padded value must be trimmed and parsed"
        );
    }

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

    // ---------------------------------------------------------------------------
    // Helper: simulate the tick write logic on an EffectivenessStateHandle.
    //
    // Mirrors the logic inside maintenance_tick without requiring a real store or
    // StatusService. Returns the set of quarantine candidates collected.
    // ---------------------------------------------------------------------------

    /// Apply one tick write to `effectiveness_state` using the given category map.
    ///
    /// Returns `(to_quarantine, generation_after)`.
    fn apply_tick_write(
        effectiveness_state: &EffectivenessStateHandle,
        categories_from_report: HashMap<u64, EffectivenessCategory>,
        auto_quarantine_cycles: u32,
    ) -> (Vec<(u64, u32, EffectivenessCategory)>, u64) {
        let to_quarantine: Vec<(u64, u32, EffectivenessCategory)> = {
            let mut state = effectiveness_state
                .write()
                .unwrap_or_else(|e| e.into_inner());

            // Replace categories map
            state.categories = categories_from_report;

            // Remove counters for entries absent from the new classification set
            let active_ids: HashSet<u64> = state.categories.keys().copied().collect();
            state
                .consecutive_bad_cycles
                .retain(|id, _| active_ids.contains(id));

            // Two-pass increment/reset (borrow-checker safe)
            let mut to_increment: Vec<u64> = Vec::new();
            let mut to_reset: Vec<u64> = Vec::new();
            for (&entry_id, category) in &state.categories {
                match category {
                    EffectivenessCategory::Ineffective | EffectivenessCategory::Noisy => {
                        to_increment.push(entry_id);
                    }
                    EffectivenessCategory::Effective
                    | EffectivenessCategory::Settled
                    | EffectivenessCategory::Unmatched => {
                        to_reset.push(entry_id);
                    }
                }
            }
            for entry_id in to_increment {
                let counter = state.consecutive_bad_cycles.entry(entry_id).or_insert(0);
                *counter += 1;
            }
            for entry_id in to_reset {
                state.consecutive_bad_cycles.remove(&entry_id);
            }

            // Collect quarantine candidates
            let mut candidates = Vec::new();
            if auto_quarantine_cycles > 0 {
                for (&entry_id, &count) in &state.consecutive_bad_cycles {
                    if count >= auto_quarantine_cycles {
                        let category = state
                            .categories
                            .get(&entry_id)
                            .copied()
                            .unwrap_or(EffectivenessCategory::Ineffective);
                        match category {
                            EffectivenessCategory::Ineffective | EffectivenessCategory::Noisy => {
                                candidates.push((entry_id, count, category));
                            }
                            _ => {}
                        }
                    }
                }
            }

            state.generation += 1;
            candidates
        };

        let generation_after = {
            let state = effectiveness_state
                .read()
                .unwrap_or_else(|e| e.into_inner());
            state.generation
        };
        (to_quarantine, generation_after)
    }

    // ---------------------------------------------------------------------------
    // FR-03 / AC-01 — Categories written correctly from report
    // ---------------------------------------------------------------------------

    #[test]
    fn test_tick_write_updates_categories_from_report() {
        let handle = EffectivenessState::new_handle();
        let mut cats = HashMap::new();
        cats.insert(1u64, EffectivenessCategory::Effective);
        cats.insert(2u64, EffectivenessCategory::Ineffective);
        cats.insert(3u64, EffectivenessCategory::Settled);

        apply_tick_write(&handle, cats, 5);

        let state = handle.read().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            state.categories.get(&1),
            Some(&EffectivenessCategory::Effective)
        );
        assert_eq!(
            state.categories.get(&2),
            Some(&EffectivenessCategory::Ineffective)
        );
        assert_eq!(
            state.categories.get(&3),
            Some(&EffectivenessCategory::Settled)
        );
    }

    // ---------------------------------------------------------------------------
    // FR-03 — Generation increments on each tick write
    // ---------------------------------------------------------------------------

    #[test]
    fn test_tick_write_increments_generation() {
        let handle = EffectivenessState::new_handle();

        {
            let (_, generation_after) = apply_tick_write(&handle, HashMap::new(), 5);
            assert_eq!(generation_after, 1, "generation must be 1 after first tick");
        }
        {
            let (_, generation_after) = apply_tick_write(&handle, HashMap::new(), 5);
            assert_eq!(
                generation_after, 2,
                "generation must be 2 after second tick"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // FR-09 / AC-09 — consecutive_bad_cycles increment for Ineffective
    // ---------------------------------------------------------------------------

    #[test]
    fn test_consecutive_bad_cycles_increment_for_ineffective() {
        let handle = EffectivenessState::new_handle();

        let mut cats = HashMap::new();
        cats.insert(10u64, EffectivenessCategory::Ineffective);
        apply_tick_write(&handle, cats.clone(), 99);
        {
            let state = handle.read().unwrap_or_else(|e| e.into_inner());
            assert_eq!(
                state.consecutive_bad_cycles.get(&10),
                Some(&1),
                "counter must be 1 after tick 1"
            );
        }

        apply_tick_write(&handle, cats, 99);
        {
            let state = handle.read().unwrap_or_else(|e| e.into_inner());
            assert_eq!(
                state.consecutive_bad_cycles.get(&10),
                Some(&2),
                "counter must be 2 after tick 2"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // FR-09 / AC-09 — consecutive_bad_cycles increment for Noisy
    // ---------------------------------------------------------------------------

    #[test]
    fn test_consecutive_bad_cycles_increment_for_noisy() {
        let handle = EffectivenessState::new_handle();

        let mut cats = HashMap::new();
        cats.insert(20u64, EffectivenessCategory::Noisy);
        apply_tick_write(&handle, cats, 99);

        let state = handle.read().unwrap_or_else(|e| e.into_inner());
        assert_eq!(
            state.consecutive_bad_cycles.get(&20),
            Some(&1),
            "Noisy must increment counter by 1"
        );
    }

    // ---------------------------------------------------------------------------
    // FR-09 / AC-09 — counter resets when entry becomes Effective
    // ---------------------------------------------------------------------------

    #[test]
    fn test_consecutive_bad_cycles_reset_on_recovery() {
        let handle = EffectivenessState::new_handle();

        // Pre-seed: entry 5 was Ineffective for 2 ticks
        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.consecutive_bad_cycles.insert(5, 2);
        }

        let mut cats = HashMap::new();
        cats.insert(5u64, EffectivenessCategory::Effective);
        apply_tick_write(&handle, cats, 99);

        let state = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            !state.consecutive_bad_cycles.contains_key(&5),
            "counter must be removed on Effective recovery"
        );
    }

    // ---------------------------------------------------------------------------
    // FR-09 — counter reset on Settled
    // ---------------------------------------------------------------------------

    #[test]
    fn test_consecutive_bad_cycles_reset_on_settled() {
        let handle = EffectivenessState::new_handle();

        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.consecutive_bad_cycles.insert(7, 3);
        }

        let mut cats = HashMap::new();
        cats.insert(7u64, EffectivenessCategory::Settled);
        apply_tick_write(&handle, cats, 99);

        let state = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            !state.consecutive_bad_cycles.contains_key(&7),
            "counter must be removed on Settled"
        );
    }

    // ---------------------------------------------------------------------------
    // FR-09 — counter reset on Unmatched
    // ---------------------------------------------------------------------------

    #[test]
    fn test_consecutive_bad_cycles_reset_on_unmatched() {
        let handle = EffectivenessState::new_handle();

        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.consecutive_bad_cycles.insert(8, 1);
        }

        let mut cats = HashMap::new();
        cats.insert(8u64, EffectivenessCategory::Unmatched);
        apply_tick_write(&handle, cats, 99);

        let state = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            !state.consecutive_bad_cycles.contains_key(&8),
            "counter must be removed on Unmatched"
        );
    }

    // ---------------------------------------------------------------------------
    // FR-09 — counter removed for entry absent from tick report (quarantined externally)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_consecutive_bad_cycles_remove_absent_entry() {
        let handle = EffectivenessState::new_handle();

        // Entry 99 has a counter but no longer appears in the report
        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.consecutive_bad_cycles.insert(99, 2);
        }

        // Report with only entry 1, not entry 99
        let mut cats = HashMap::new();
        cats.insert(1u64, EffectivenessCategory::Effective);
        apply_tick_write(&handle, cats, 99);

        let state = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            !state.consecutive_bad_cycles.contains_key(&99),
            "absent entry counter must be removed by retain()"
        );
    }

    // ---------------------------------------------------------------------------
    // FR-09 — three-tick sequence with recovery: no false quarantine at threshold=2
    // ---------------------------------------------------------------------------

    #[test]
    fn test_consecutive_bad_cycles_three_tick_sequence_no_quarantine() {
        // Tick 1: Ineffective → counter=1
        // Tick 2: Effective   → counter=0 (reset)
        // Tick 3: Ineffective → counter=1 (not 3)
        let handle = EffectivenessState::new_handle();
        let threshold = 2u32;

        let mut ineffective = HashMap::new();
        ineffective.insert(42u64, EffectivenessCategory::Ineffective);
        let mut effective = HashMap::new();
        effective.insert(42u64, EffectivenessCategory::Effective);

        apply_tick_write(&handle, ineffective.clone(), threshold);
        {
            let state = handle.read().unwrap_or_else(|e| e.into_inner());
            assert_eq!(state.consecutive_bad_cycles.get(&42), Some(&1));
        }

        apply_tick_write(&handle, effective, threshold);
        {
            let state = handle.read().unwrap_or_else(|e| e.into_inner());
            assert!(!state.consecutive_bad_cycles.contains_key(&42));
        }

        let (candidates, _) = apply_tick_write(&handle, ineffective, threshold);
        {
            let state = handle.read().unwrap_or_else(|e| e.into_inner());
            assert_eq!(state.consecutive_bad_cycles.get(&42), Some(&1));
        }
        assert!(
            candidates.is_empty(),
            "counter=1 < threshold=2, so no quarantine candidate"
        );
    }

    // ---------------------------------------------------------------------------
    // AC-14 / R-11 — Category restriction: only Ineffective and Noisy qualify for quarantine
    // ---------------------------------------------------------------------------

    #[test]
    fn test_auto_quarantine_does_not_fire_for_settled() {
        let handle = EffectivenessState::new_handle();
        // Pre-seed high counter
        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.consecutive_bad_cycles.insert(50, 10);
        }
        let mut cats = HashMap::new();
        cats.insert(50u64, EffectivenessCategory::Settled);
        let (candidates, _) = apply_tick_write(&handle, cats, 3);
        assert!(
            candidates.is_empty(),
            "Settled must never produce quarantine candidates"
        );
    }

    #[test]
    fn test_auto_quarantine_does_not_fire_for_unmatched() {
        let handle = EffectivenessState::new_handle();
        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.consecutive_bad_cycles.insert(51, 10);
        }
        let mut cats = HashMap::new();
        cats.insert(51u64, EffectivenessCategory::Unmatched);
        let (candidates, _) = apply_tick_write(&handle, cats, 3);
        assert!(
            candidates.is_empty(),
            "Unmatched must never produce quarantine candidates"
        );
    }

    #[test]
    fn test_auto_quarantine_does_not_fire_for_effective() {
        let handle = EffectivenessState::new_handle();
        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.consecutive_bad_cycles.insert(52, 10);
        }
        let mut cats = HashMap::new();
        cats.insert(52u64, EffectivenessCategory::Effective);
        let (candidates, _) = apply_tick_write(&handle, cats, 3);
        assert!(
            candidates.is_empty(),
            "Effective must never produce quarantine candidates"
        );
    }

    // ---------------------------------------------------------------------------
    // AC-10 / AC-11 — Quarantine fires at threshold
    // ---------------------------------------------------------------------------

    #[test]
    fn test_auto_quarantine_fires_at_threshold() {
        let handle = EffectivenessState::new_handle();
        // Simulate 3 prior ticks of Ineffective
        let mut cats = HashMap::new();
        cats.insert(100u64, EffectivenessCategory::Ineffective);
        apply_tick_write(&handle, cats.clone(), 99); // counter=1
        apply_tick_write(&handle, cats.clone(), 99); // counter=2
        let (candidates, _) = apply_tick_write(&handle, cats, 3); // counter=3, threshold=3

        assert_eq!(candidates.len(), 1);
        let (id, count, cat) = candidates[0];
        assert_eq!(id, 100);
        assert_eq!(count, 3);
        assert_eq!(cat, EffectivenessCategory::Ineffective);
    }

    #[test]
    fn test_auto_quarantine_does_not_fire_below_threshold() {
        let handle = EffectivenessState::new_handle();
        let mut cats = HashMap::new();
        cats.insert(101u64, EffectivenessCategory::Ineffective);
        apply_tick_write(&handle, cats.clone(), 99); // counter=1
        let (candidates, _) = apply_tick_write(&handle, cats, 3); // counter=2, threshold=3

        assert!(
            candidates.is_empty(),
            "counter=2 must not produce candidates when threshold=3"
        );
    }

    #[test]
    fn test_auto_quarantine_fires_at_threshold_1() {
        // AC-11: threshold=1 means quarantine after first consecutive bad tick
        let handle = EffectivenessState::new_handle();
        let mut cats = HashMap::new();
        cats.insert(102u64, EffectivenessCategory::Noisy);
        let (candidates, _) = apply_tick_write(&handle, cats, 1); // counter=1 >= threshold=1

        assert_eq!(
            candidates.len(),
            1,
            "threshold=1 must fire on first bad tick"
        );
        assert_eq!(candidates[0].0, 102);
        assert_eq!(candidates[0].2, EffectivenessCategory::Noisy);
    }

    // ---------------------------------------------------------------------------
    // AC-12 — Quarantine disabled when threshold = 0
    // ---------------------------------------------------------------------------

    #[test]
    fn test_auto_quarantine_disabled_when_threshold_zero() {
        let handle = EffectivenessState::new_handle();
        // Tick many times with Ineffective
        let mut cats = HashMap::new();
        cats.insert(200u64, EffectivenessCategory::Ineffective);
        for _ in 0..10 {
            let (candidates, _) = apply_tick_write(&handle, cats.clone(), 0);
            assert!(
                candidates.is_empty(),
                "threshold=0 must never produce quarantine candidates"
            );
        }
    }

    // ---------------------------------------------------------------------------
    // parse_auto_quarantine_cycles validation (Constraint 14)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_parse_auto_quarantine_cycles_default_is_3() {
        // Test the inner parse function with the default value "3" — avoids
        // unsafe env var manipulation forbidden by #![forbid(unsafe_code)].
        let result = parse_auto_quarantine_cycles_str("3");
        assert!(result.is_ok(), "default parse must succeed");
        assert_eq!(result.unwrap(), 3, "default must be 3");
    }

    #[test]
    fn test_parse_auto_quarantine_cycles_zero_is_valid() {
        let result = parse_auto_quarantine_cycles_str("0");
        assert!(
            result.is_ok(),
            "zero must be accepted (disables auto-quarantine)"
        );
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_parse_auto_quarantine_cycles_boundary_1000_accepted() {
        let result = parse_auto_quarantine_cycles_str("1000");
        assert!(result.is_ok(), "1000 is the accepted upper boundary");
        assert_eq!(result.unwrap(), 1000);
    }

    #[test]
    fn test_parse_auto_quarantine_cycles_rejects_over_1000() {
        let result = parse_auto_quarantine_cycles_str("1001");
        assert!(
            result.is_err(),
            "1001 must be rejected as implausibly large"
        );
        let msg = result.unwrap_err();
        assert!(
            msg.contains("1001"),
            "error message must reference the invalid value"
        );
    }

    #[test]
    fn test_parse_auto_quarantine_cycles_rejects_non_integer() {
        let result = parse_auto_quarantine_cycles_str("abc");
        assert!(result.is_err(), "non-integer must be rejected");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("abc"),
            "error message must include the bad value"
        );
    }

    #[test]
    fn test_parse_auto_quarantine_cycles_rejects_negative() {
        // Negative strings like "-1" are rejected because u32 parse fails on '-'.
        let result = parse_auto_quarantine_cycles_str("-1");
        assert!(result.is_err(), "negative value must be rejected");
    }

    // ---------------------------------------------------------------------------
    // Audit event constants (FR-11, AC-13) — code-level assertions
    // ---------------------------------------------------------------------------

    #[test]
    fn test_audit_constants_have_correct_values() {
        // Security Risk 2: SYSTEM_AGENT_ID must be a hardcoded compile-time constant
        assert_eq!(SYSTEM_AGENT_ID, "system");
        assert_eq!(OP_AUTO_QUARANTINE, "auto_quarantine");
        assert_eq!(OP_TICK_SKIPPED, "tick_skipped");
    }

    #[test]
    fn test_auto_quarantine_max_cycles_constant() {
        assert_eq!(
            AUTO_QUARANTINE_CYCLES_MAX, 1000,
            "DoS mitigation constant must be 1000"
        );
    }

    // ---------------------------------------------------------------------------
    // emit_auto_quarantine_audit field correctness (AC-13 / FR-11)
    // ---------------------------------------------------------------------------

    /// Read the most recent audit_log rows (up to `limit`) from a store.
    fn read_recent_audit_events(
        store: &unimatrix_store::Store,
        limit: i64,
    ) -> Vec<unimatrix_store::AuditEvent> {
        let conn = store.lock_conn();
        let mut stmt = match conn.prepare(
            "SELECT event_id, timestamp, session_id, agent_id, operation,
                    target_ids, outcome, detail
             FROM audit_log ORDER BY event_id DESC LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        let rows = match stmt.query_map(unimatrix_store::rusqlite::params![limit], |row| {
            let target_ids_json: String = row.get("target_ids")?;
            let target_ids: Vec<u64> = serde_json::from_str(&target_ids_json).unwrap_or_default();
            Ok(unimatrix_store::AuditEvent {
                event_id: row.get::<_, i64>("event_id")? as u64,
                timestamp: row.get::<_, i64>("timestamp")? as u64,
                session_id: row.get("session_id")?,
                agent_id: row.get("agent_id")?,
                operation: row.get("operation")?,
                target_ids,
                outcome: Outcome::try_from(row.get::<_, i64>("outcome")? as u8)
                    .unwrap_or(Outcome::Error),
                detail: row.get("detail")?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return vec![],
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    #[test]
    fn test_emit_auto_quarantine_audit_detail_fields() {
        use crate::infra::audit::AuditLog;
        use tempfile::TempDir;

        let tmp = TempDir::new().expect("tempdir");
        let db_path = tmp.path().join("test.db");

        let store = Arc::new(unimatrix_store::Store::open(&db_path).expect("store open"));
        let audit_log = Arc::new(AuditLog::new(Arc::clone(&store)));

        emit_auto_quarantine_audit(
            &audit_log,
            42,
            "test-entry",
            "eng-practices",
            "convention",
            EffectivenessCategory::Ineffective,
            3,
            3,
        );

        let events = read_recent_audit_events(&store, 10);
        let event = events
            .iter()
            .find(|e| e.operation == "auto_quarantine")
            .expect("auto_quarantine event must be written");

        assert_eq!(
            event.agent_id, "system",
            "agent_id must be hardcoded system"
        );
        assert_eq!(
            event.target_ids,
            vec![42u64],
            "target_ids must include entry_id"
        );
        assert_eq!(event.outcome, Outcome::Success, "outcome must be Success");
        assert!(
            event.detail.contains("entry_title"),
            "detail must contain entry_title key"
        );
        assert!(
            event.detail.contains("test-entry"),
            "detail must contain entry title value"
        );
        assert!(
            event.detail.contains("consecutive_cycles"),
            "detail must contain consecutive_cycles"
        );
        assert!(
            event.detail.contains("threshold"),
            "detail must contain threshold"
        );
        assert!(
            event.detail.contains("Ineffective"),
            "detail must contain classification"
        );
    }

    #[test]
    fn test_emit_tick_skipped_audit_detail_fields() {
        use crate::infra::audit::AuditLog;
        use tempfile::TempDir;

        let tmp = TempDir::new().expect("tempdir");
        let db_path = tmp.path().join("test2.db");

        let store = Arc::new(unimatrix_store::Store::open(&db_path).expect("store open"));
        let audit_log = Arc::new(AuditLog::new(Arc::clone(&store)));

        emit_tick_skipped_audit(&audit_log, "db locked".to_string());

        let events = read_recent_audit_events(&store, 10);
        let event = events
            .iter()
            .find(|e| e.operation == "tick_skipped")
            .expect("tick_skipped event must be written");

        assert_eq!(event.agent_id, "system");
        assert_eq!(
            event.outcome,
            Outcome::Error,
            "tick_skipped outcome must be Error"
        );
        assert!(
            event.detail.contains("db locked"),
            "detail must contain the error reason"
        );
        assert!(
            event.target_ids.is_empty(),
            "tick_skipped has no target ids"
        );
    }

    // ---------------------------------------------------------------------------
    // R-13 — Write lock released before quarantine: verify read lock acquirable
    // ---------------------------------------------------------------------------

    #[test]
    fn test_write_lock_not_held_after_tick_write_block() {
        // The apply_tick_write helper mirrors the exact scoped block pattern in
        // maintenance_tick. After the function returns, the write guard is dropped.
        // Verify that a subsequent read lock acquisition is non-blocking.
        let handle = EffectivenessState::new_handle();
        let mut cats = HashMap::new();
        cats.insert(1u64, EffectivenessCategory::Ineffective);

        apply_tick_write(&handle, cats, 99);

        // try_read() must succeed (non-blocking) — write lock is released
        let try_result = handle.try_read();
        assert!(
            try_result.is_ok(),
            "read lock must be acquirable after tick write block completes (write lock released)"
        );
    }

    // ---------------------------------------------------------------------------
    // Empty report edge case
    // ---------------------------------------------------------------------------

    #[test]
    fn test_tick_write_with_empty_report_clears_categories() {
        let handle = EffectivenessState::new_handle();

        // Pre-seed categories and counters
        {
            let mut state = handle.write().unwrap_or_else(|e| e.into_inner());
            state.categories.insert(1, EffectivenessCategory::Effective);
            state.consecutive_bad_cycles.insert(2, 3);
            state.generation = 5;
        }

        // Empty report (no entries classified)
        apply_tick_write(&handle, HashMap::new(), 3);

        let state = handle.read().unwrap_or_else(|e| e.into_inner());
        assert!(
            state.categories.is_empty(),
            "categories must be cleared by empty report"
        );
        assert!(
            state.consecutive_bad_cycles.is_empty(),
            "consecutive_bad_cycles must be cleared when all entries absent"
        );
        assert_eq!(
            state.generation, 6,
            "generation still increments on empty tick"
        );
    }

    // ---------------------------------------------------------------------------
    // Multiple consecutive bad ticks without quarantine (no-op when threshold high)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_tick_write_with_no_quarantine_candidates_is_noop() {
        let handle = EffectivenessState::new_handle();
        let mut cats = HashMap::new();
        cats.insert(300u64, EffectivenessCategory::Ineffective);

        // threshold = 99, only 2 ticks → no candidates
        apply_tick_write(&handle, cats.clone(), 99);
        let (candidates, _) = apply_tick_write(&handle, cats, 99);

        assert!(
            candidates.is_empty(),
            "must return empty candidates when counter < threshold"
        );
    }
}
