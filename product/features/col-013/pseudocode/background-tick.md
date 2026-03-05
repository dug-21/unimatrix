# Pseudocode: background-tick (Wave 3)

## 1. background.rs (NEW file in unimatrix-server/src/)

```rust
use std::sync::Arc;
use std::time::Duration;
use tokio::time;

use unimatrix_core::{Store, VectorIndex, StoreAdapter};
use unimatrix_core::async_wrappers::AsyncEntryStore;
use unimatrix_observe::extraction::{
    ExtractionContext, ExtractionStats, ExtractionRule, ProposedEntry,
    default_extraction_rules, quality_gate, QualityGateResult,
};

use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::session::SessionRegistry;
use crate::services::status::StatusService;
use crate::server::PendingEntriesAnalysis;

const TICK_INTERVAL_SECS: u64 = 900; // 15 minutes

/// Shared tick metadata (read by context_status, written by tick loop)
pub struct TickMetadata {
    pub last_maintenance_run: Option<u64>,
    pub last_extraction_run: Option<u64>,
    pub next_scheduled: Option<u64>,
    pub extraction_stats: ExtractionStats,
}

impl TickMetadata {
    pub fn new() -> Self {
        TickMetadata {
            last_maintenance_run: None,
            last_extraction_run: None,
            next_scheduled: None,
            extraction_stats: ExtractionStats {
                entries_extracted_total: 0,
                entries_rejected_total: 0,
                last_extraction_run: None,
                rules_fired: HashMap::new(),
            },
        }
    }
}

/// Launch the background tick loop. Call once at server startup.
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
        store, vector_index, embed_service, adapt_service,
        session_registry, entry_store, pending_entries, tick_metadata,
    ))
}

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
    let mut interval = time::interval(Duration::from_secs(TICK_INTERVAL_SECS));
    let mut extraction_ctx = ExtractionContext::new();

    // Skip the first immediate tick (fires at t=0)
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
            &store,
            &session_registry,
            &entry_store,
            &pending_entries,
        ).await {
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
            &adapt_service,
            &entry_store,
            &mut extraction_ctx,
        ).await {
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

        // Update next scheduled
        if let Ok(mut meta) = tick_metadata.lock() {
            meta.next_scheduled = Some(now_secs() + TICK_INTERVAL_SECS);
        }

        let duration = now_secs() - tick_start;
        tracing::info!(duration_secs = duration, "background tick complete");
    }
}
```

## 2. maintenance_tick()

```rust
/// Run maintenance operations. Extracted from StatusService::run_maintenance().
pub async fn maintenance_tick(
    status_svc: &StatusService,
    store: &Arc<Store>,
    session_registry: &SessionRegistry,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    pending_entries: &Arc<Mutex<PendingEntriesAnalysis>>,
) -> Result<(), ServiceError> {
    // Compute a lightweight report to get active entries
    let (mut report, active_entries) = status_svc.compute_report(None, None, false).await?;

    // Run existing maintenance logic
    status_svc.run_maintenance(
        &active_entries,
        &mut report,
        session_registry,
        entry_store,
        pending_entries,
    ).await?;

    Ok(())
}
```

## 3. extraction_tick()

```rust
/// Run extraction pipeline on new observations since last watermark.
pub async fn extraction_tick(
    store: &Arc<Store>,
    vector_index: &Arc<VectorIndex>,
    embed_service: &Arc<EmbedServiceHandle>,
    adapt_service: &Arc<AdaptationService>,
    entry_store: &Arc<AsyncEntryStore<StoreAdapter>>,
    ctx: &mut ExtractionContext,
) -> Result<ExtractionStats, ServiceError> {
    let store_clone = Arc::clone(store);
    let watermark = ctx.last_watermark;

    // 1. Query new observations since watermark (spawn_blocking)
    let observations = tokio::task::spawn_blocking(move || {
        let conn = store_clone.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, ts_millis, hook_type, session_id, tool_name, input_json, response_size, response_snippet
             FROM observations WHERE id > ?1 ORDER BY id ASC LIMIT 10000"
        )?;
        // Parse rows into ObservationRecord + track max id
        let mut records = Vec::new();
        let mut max_id = watermark;
        let rows = stmt.query_map(rusqlite::params![watermark as i64], |row| {
            let id: i64 = row.get(0)?;
            let ts: i64 = row.get(1)?;
            let hook_str: String = row.get(2)?;
            let session_id: String = row.get(3)?;
            let tool: Option<String> = row.get(4)?;
            let input_str: Option<String> = row.get(5)?;
            let response_size: Option<i64> = row.get(6)?;
            let snippet: Option<String> = row.get(7)?;
            Ok((id, ts, hook_str, session_id, tool, input_str, response_size, snippet))
        })?;
        for row in rows {
            let (id, ts, hook_str, session_id, tool, input_str, response_size, snippet) = row?;
            if id as u64 > max_id { max_id = id as u64; }
            let hook = parse_hook_type(&hook_str);
            let input = input_str.and_then(|s| serde_json::from_str(&s).ok());
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
        Ok::<(Vec<ObservationRecord>, u64), ServerError>((records, max_id))
    }).await??;

    let (observations, new_watermark) = observations;

    if observations.is_empty() {
        return Ok(ctx.stats.clone());
    }

    // 2. Run extraction rules
    let rules = default_extraction_rules();
    let store_for_rules = Arc::clone(store);
    let obs_for_rules = observations.clone();

    let proposals = tokio::task::spawn_blocking(move || {
        let mut all_proposals = Vec::new();
        for rule in &rules {
            let rule_proposals = rule.evaluate(&obs_for_rules, &store_for_rules);
            all_proposals.extend(rule_proposals);
        }
        all_proposals
    }).await?;

    // 3. Quality gate (checks 1-4: cheap, in-memory)
    let mut accepted: Vec<ProposedEntry> = Vec::new();
    for proposal in proposals {
        match quality_gate(&proposal, ctx) {
            QualityGateResult::Accept => accepted.push(proposal),
            QualityGateResult::Reject { reason, check_name } => {
                tracing::debug!(rule = %proposal.source_rule, check = %check_name, reason = %reason, "extraction rejected");
                ctx.stats.entries_rejected_total += 1;
            }
        }
    }

    // 4. Quality gate checks 5-6: near-duplicate + contradiction (need embedding)
    if !accepted.is_empty() {
        if let Ok(adapter) = embed_service.get_adapter().await {
            let store_for_gate = Arc::clone(store);
            let vi_for_gate = Arc::clone(vector_index);

            let final_accepted = tokio::task::spawn_blocking(move || {
                let vs = VectorAdapter::new(vi_for_gate);
                let config = ContradictionConfig::default();
                let mut passed = Vec::new();

                for entry in accepted {
                    // Check 5: Near-duplicate
                    let embedding = match adapter.embed_entry(&entry.title, &entry.content) {
                        Ok(v) => v,
                        Err(_) => { passed.push(entry); continue; }
                    };
                    let neighbors = match vs.search(&embedding, 1, 32) {
                        Ok(n) => n,
                        Err(_) => { passed.push(entry); continue; }
                    };
                    if let Some(top) = neighbors.first() {
                        if top.similarity >= 0.92 {
                            // Near-duplicate, reject
                            continue;
                        }
                    }

                    // Check 6: Contradiction
                    match check_entry_contradiction(
                        &entry.content, &entry.title,
                        &store_for_gate, &vs, &*adapter, &config,
                    ) {
                        Ok(Some(_)) => continue, // contradiction detected
                        _ => {}
                    }

                    passed.push(entry);
                }
                passed
            }).await?;

            // 5. Store accepted entries
            for entry in final_accepted {
                let rule_name = entry.source_rule.clone();
                let feature = entry.source_features.first()
                    .cloned()
                    .unwrap_or_default();
                let tags: Vec<String> = vec![
                    "auto-extracted".to_string(),
                    format!("rule:{}", rule_name),
                    format!("source-features:{}", entry.source_features.join(",")),
                ];

                // Store via entry_store (same path as context_store)
                // ... store logic using NewEntry ...

                ctx.stats.entries_extracted_total += 1;
                *ctx.stats.rules_fired.entry(rule_name).or_insert(0) += 1;
            }
        }
    }

    // 6. Update watermark
    ctx.last_watermark = new_watermark;
    ctx.stats.last_extraction_run = Some(now_secs());

    Ok(ctx.stats.clone())
}
```

## 4. StatusReport Changes (response/status.rs)

```rust
// Add to StatusReport struct:
pub last_maintenance_run: Option<u64>,
pub next_maintenance_scheduled: Option<u64>,
pub extraction_stats: Option<ExtractionStatsResponse>,
pub coherence_by_source: Vec<(String, f64)>,

// New response type:
pub struct ExtractionStatsResponse {
    pub entries_extracted_total: u64,
    pub entries_rejected_total: u64,
    pub last_extraction_run: Option<u64>,
    pub rules_fired: Vec<(String, u64)>,
}
```

## 5. context_status Changes (mcp/tools.rs)

```rust
// In handle_context_status:
// Remove the maintain check:
// - let maintain_enabled = params.maintain.unwrap_or(false);
// - if maintain_enabled { ... }
// Replace with: silently ignore (parameter stays in struct, just don't act on it)

// Add tick_metadata read:
// let tick_meta = self.tick_metadata.lock().unwrap_or_else(|e| e.into_inner());
// report.last_maintenance_run = tick_meta.last_maintenance_run;
// report.next_maintenance_scheduled = tick_meta.next_scheduled;
// report.extraction_stats = Some(ExtractionStatsResponse { ... });
```

## 6. coherence_by_source (services/status.rs)

```rust
// In compute_report, after building active_entries:
// Group active entries by trust_source
// For each group, compute lambda using the same dimensions
// Store in report.coherence_by_source

let mut source_groups: HashMap<String, Vec<&EntryRecord>> = HashMap::new();
for entry in &active_entries {
    let source = if entry.trust_source.is_empty() { "(none)" } else { &entry.trust_source };
    source_groups.entry(source.to_string()).or_default().push(entry);
}

let mut coherence_by_source = Vec::new();
for (source, entries) in &source_groups {
    let (freshness, _) = coherence::confidence_freshness_score(entries, now_ts, threshold);
    let lambda = coherence::compute_lambda(freshness, graph_quality, embed_dim, contradiction, &weights);
    coherence_by_source.push((source.clone(), lambda));
}
report.coherence_by_source = coherence_by_source;
```

## 7. Server Startup (server.rs)

```rust
// In UnimatrixServer::new() or serve():
// Add tick_metadata field to UnimatrixServer
// After all subsystems initialized:
let tick_metadata = Arc::new(Mutex::new(TickMetadata::new()));
let _tick_handle = spawn_background_tick(
    store, vector_index, embed_service, adapt_service,
    session_registry, entry_store, pending_entries, tick_metadata,
);
// Store tick_metadata in UnimatrixServer for context_status reads
```

## 8. StatusReport JSON Serialization

```rust
// In format_status_report (response/mod.rs):
// Add the new fields to JSON output:
// - last_maintenance_run
// - next_maintenance_scheduled
// - extraction_stats
// - coherence_by_source
// Use serde(default) and skip_serializing_if for Option fields
```
