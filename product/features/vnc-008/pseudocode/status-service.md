# Pseudocode: status-service

## Purpose
Extract the ~628-line `context_status` computation from `mcp/tools.rs` into `services/status.rs` as StatusService.

## Files Created
- `src/services/status.rs`

## Files Modified
- `src/services/mod.rs`
- `src/mcp/tools.rs`
- `src/server.rs` (wire StatusService into ServiceLayer)

## Pseudocode

### src/services/status.rs

```
use std::collections::BTreeMap;
use std::sync::Arc;

use redb::ReadableTable;
use unimatrix_core::{CoreError, EmbedService, Store, VectorAdapter, VectorIndex};
use unimatrix_core::async_wrappers::AsyncEntryStore;
use unimatrix_store::{
    ENTRIES, CATEGORY_INDEX, TOPIC_INDEX, COUNTERS,
    deserialize_entry, EntryRecord, Status,
};
use unimatrix_store::sessions::{TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS};

use crate::infra::coherence;
use crate::infra::contradiction;
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::session::SessionRegistry;
use crate::mcp::response::status::{StatusReport, CoAccessClusterEntry};
use crate::services::gateway::SecurityGateway;
use crate::services::ServiceError;
use crate::uds::listener::{run_confidence_consumer, run_retrospective_consumer, write_signals_to_queue};
use crate::server::PendingEntriesAnalysis;

/// Transport-agnostic status computation service.
///
/// Extracted from the context_status handler (ADR-001).
/// Inherits direct-table access — Store API expansion deferred.
pub(crate) struct StatusService {
    store: Arc<Store>,
    vector_index: Arc<VectorIndex>,
    embed_service: Arc<EmbedServiceHandle>,
    gateway: Arc<SecurityGateway>,
}

/// Result of maintenance operations.
pub(crate) struct MaintenanceResult {
    pub confidence_refreshed: u64,
    pub graph_compacted: bool,
    pub stale_pairs_cleaned: u64,
    pub observation_files_cleaned: bool,
    pub sessions_swept: bool,
    pub session_gc_performed: bool,
}

impl StatusService {
    pub(crate) fn new(
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        embed_service: Arc<EmbedServiceHandle>,
        gateway: Arc<SecurityGateway>,
    ) -> Self {
        StatusService { store, vector_index, embed_service, gateway }
    }

    /// Compute the full status report. Read-only, single transaction for counters and entries.
    ///
    /// Matches the exact computation from the inline context_status handler.
    /// Returns (StatusReport, active_entries) for optional maintenance pass.
    pub(crate) async fn compute_report(
        &self,
        topic_filter: Option<String>,
        category_filter: Option<String>,
        check_embeddings: bool,
    ) -> Result<(StatusReport, Vec<EntryRecord>), ServiceError> {
        // Phase 1: Read transaction (spawn_blocking)
        //   - Read COUNTERS: total_active, total_deprecated, total_proposed, total_quarantined
        //   - Read CATEGORY_INDEX: category distribution (with optional filter)
        //   - Read TOPIC_INDEX: topic distribution (with optional filter)
        //   - Scan ENTRIES: correction chain metrics, trust source distribution, active_entries collection
        //   - Outcome statistics from CATEGORY_INDEX("outcome") range scan
        //   - Build initial StatusReport with default coherence fields

        // Phase 2: Contradiction scanning (always, outside read txn)
        //   - Get embed adapter
        //   - spawn_blocking: scan_contradictions()
        //   - Graceful degradation on failure

        // Phase 3: Embedding consistency (opt-in via check_embeddings)
        //   - spawn_blocking: check_embedding_consistency()
        //   - Graceful degradation on failure

        // Phase 4: Co-access stats (read-only portion, maintain=false)
        //   - co_access_stats(), top_co_access_pairs(5)
        //   - Resolve titles for top pairs
        //   - Graceful degradation on failure

        // Phase 5: Coherence dimensions (always computed)
        //   - confidence_freshness_score
        //   - graph_quality_score
        //   - embedding_consistency_score (if check_embeddings)
        //   - contradiction_density_score
        //   - compute_lambda
        //   - generate_recommendations

        // Phase 6: Observation stats
        //   - scan_observation_stats

        // Phase 7: Retrospected feature count
        //   - store.list_all_metrics()

        Ok((report, active_entries))
    }

    /// Run maintenance operations. Requires Admin capability (enforced by caller).
    ///
    /// Operations (matches maintain=true path in original handler):
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
        // Additional dependencies needed for maintenance:
        session_registry: &SessionRegistry,
        entry_store: &Arc<AsyncEntryStore<unimatrix_core::StoreAdapter>>,
        pending_entries_analysis: &Arc<std::sync::Mutex<PendingEntriesAnalysis>>,
        adapt_service: &Arc<unimatrix_adapt::AdaptationService>,
    ) -> Result<MaintenanceResult, ServiceError> {
        // 1. Co-access cleanup
        //    store.cleanup_stale_co_access(staleness_cutoff)

        // 2. Confidence refresh
        //    Identify stale entries, compute new confidence, store.update_confidence()
        //    Update report.confidence_refreshed_count

        // 3. Graph compaction
        //    If graph_stale_ratio > DEFAULT_STALE_RATIO_TRIGGER:
        //      Re-embed all active entries
        //      Adapt through AdaptationService
        //      vector_index.compact()
        //    Update report.graph_compacted

        // 4. Observation file cleanup
        //    identify_expired, remove_file

        // 5. Stale session sweep
        //    session_registry.sweep_stale_sessions()
        //    write_signals_to_queue, run_confidence_consumer, run_retrospective_consumer

        // 6. Session GC
        //    store.gc_sessions()

        Ok(MaintenanceResult { ... })
    }
}
```

Note on signature: The `run_maintenance` method needs several dependencies beyond what StatusService holds (session_registry, entry_store, pending_entries_analysis, adapt_service). These are passed as parameters rather than stored in StatusService to avoid making StatusService too large. Alternative: pass them at construction time. The implementation agent should evaluate which approach is cleaner — the key constraint is behavioral equivalence with the inline code.

### src/services/mod.rs changes

Add:
```
pub(crate) mod status;
pub(crate) use status::StatusService;
```

Update ServiceLayer:
```
pub struct ServiceLayer {
    pub(crate) search: SearchService,
    pub(crate) store_ops: StoreService,
    pub(crate) confidence: ConfidenceService,
    pub(crate) briefing: BriefingService,
    pub(crate) status: StatusService,  // NEW
}
```

Update `ServiceLayer::new()` to construct StatusService.

### src/mcp/tools.rs changes

Replace the ~628-line context_status handler body with:

```
async fn context_status(&self, Parameters(params): Parameters<StatusParams>) -> Result<CallToolResult, rmcp::ErrorData> {
    let ctx = self.build_context(&params.agent_id, &params.format)?;
    self.require_cap(&ctx.agent_id, Capability::Admin)?;
    validate_status_params(&params).map_err(rmcp::ErrorData::from)?;

    let check_embeddings = params.check_embeddings.unwrap_or(false);
    let (mut report, active_entries) = self.services.status
        .compute_report(params.topic, params.category, check_embeddings)
        .await
        .map_err(rmcp::ErrorData::from)?;

    if params.maintain.unwrap_or(false) {
        self.services.status.run_maintenance(
            &active_entries,
            &mut report,
            &self.session_registry,
            &self.entry_store,
            &self.pending_entries_analysis,
            &self.adapt_service,
        ).await.map_err(rmcp::ErrorData::from)?;
    }

    // Audit
    let _ = self.audit.log_event(AuditEvent { ... });

    Ok(format_status_report(&report, ctx.format))
}
```

Handler drops from ~628 lines to ~25 lines.

## Compilation Gate

After this step: `cargo check --workspace` must succeed. Status report tests pass unchanged.
