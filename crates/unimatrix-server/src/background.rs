//! Background tick loop for automated maintenance and knowledge extraction (col-013).
//!
//! Spawns a tokio task that runs every 15 minutes (configurable):
//! 1. Maintenance tick: co-access cleanup, confidence refresh, graph compaction,
//!    observation retention, session GC.
//! 2. Extraction tick: runs extraction rules on new observations, quality-gates
//!    proposals, stores accepted entries.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use unimatrix_core::{
    CoreError, EmbedService, NewEntry, StoreAdapter, Store, VectorAdapter, VectorIndex,
    VectorStore,
};
use unimatrix_core::async_wrappers::AsyncEntryStore;
use unimatrix_observe::extraction::{
    ExtractionContext, ExtractionStats, ProposedEntry,
    default_extraction_rules, quality_gate, run_extraction_rules, QualityGateResult,
};
use unimatrix_observe::types::{HookType, ObservationRecord};
use unimatrix_store::rusqlite;

use unimatrix_adapt::AdaptationService;

use crate::infra::contradiction::{self, ContradictionConfig};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::session::SessionRegistry;
use crate::server::PendingEntriesAnalysis;
use crate::services::status::StatusService;
use crate::services::ServiceError;

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
) {
    let mut interval = tokio::time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
    let mut extraction_ctx = ExtractionContext::new();

    // Skip the immediate first tick (fires at t=0).
    interval.tick().await;

    loop {
        interval.tick().await;
        let tick_start = now_secs();
        tracing::info!("background tick starting");

        // 1. Maintenance tick
        let status_svc = StatusService::new(
            Arc::clone(&store),
            Arc::clone(&vector_index),
            Arc::clone(&embed_service),
            Arc::clone(&adapt_service),
        );
        match maintenance_tick(
            &status_svc,
            &session_registry,
            &entry_store,
            &pending_entries,
        )
        .await
        {
            Ok(()) => {
                if let Ok(mut meta) = tick_metadata.lock() {
                    meta.last_maintenance_run = Some(tick_start);
                }
                tracing::info!("maintenance tick complete");
            }
            Err(e) => {
                tracing::warn!("maintenance tick failed: {}", e);
            }
        }

        // 2. Extraction tick
        match extraction_tick(
            &store,
            &vector_index,
            &embed_service,
            &mut extraction_ctx,
        )
        .await
        {
            Ok(stats) => {
                if let Ok(mut meta) = tick_metadata.lock() {
                    meta.last_extraction_run = Some(now_secs());
                    meta.extraction_stats = stats;
                }
                tracing::info!("extraction tick complete");
            }
            Err(e) => {
                tracing::warn!("extraction tick failed: {}", e);
            }
        }

        // Update next scheduled time
        if let Ok(mut meta) = tick_metadata.lock() {
            meta.next_scheduled = Some(now_secs() + TICK_INTERVAL_SECS);
        }

        let duration = now_secs() - tick_start;
        tracing::info!(duration_secs = duration, "background tick complete");
    }
}

/// Run maintenance operations via StatusService.
async fn maintenance_tick(
    status_svc: &StatusService,
    session_registry: &SessionRegistry,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: &Arc<Mutex<PendingEntriesAnalysis>>,
) -> Result<(), ServiceError> {
    // Compute lightweight report to get active entries
    let (mut report, active_entries) = status_svc
        .compute_report(None, None, false)
        .await?;

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
async fn extraction_tick(
    store: &Arc<Store>,
    vector_index: &Arc<VectorIndex>,
    embed_service: &Arc<EmbedServiceHandle>,
    ctx: &mut ExtractionContext,
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
                Ok((id, ts, hook_str, session_id, tool, input_str, response_size, snippet))
            })
            .map_err(|e| ServiceError::Core(CoreError::Store(
                unimatrix_store::StoreError::Sqlite(e),
            )))?;

        for row in rows {
            let (id, ts, hook_str, session_id, tool, input_str, response_size, snippet) =
                row.map_err(|e| ServiceError::Core(CoreError::Store(
                    unimatrix_store::StoreError::Sqlite(e),
                )))?;
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
            QualityGateResult::Reject {
                reason,
                check_name,
            } => {
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
                    trust_source: "auto".to_string(),
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
        assert!(matches!(parse_hook_type("PreToolUse"), HookType::PreToolUse));
        assert!(matches!(parse_hook_type("PostToolUse"), HookType::PostToolUse));
        assert!(matches!(parse_hook_type("SubagentStart"), HookType::SubagentStart));
        assert!(matches!(parse_hook_type("SubagentStop"), HookType::SubagentStop));
        assert!(matches!(parse_hook_type("Unknown"), HookType::PreToolUse));
    }

    #[test]
    fn now_secs_returns_reasonable_value() {
        let ts = now_secs();
        // Should be after 2024-01-01 (1704067200)
        assert!(ts > 1_704_067_200);
    }
}
