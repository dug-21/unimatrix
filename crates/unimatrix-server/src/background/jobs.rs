//! crt-056 Wave 2 — the 9 tick operations wrapped as `BackgroundJob`s (ADR-004).
//!
//! Each job's `run` delegates to the EXISTING op fn / helper in `background.rs`
//! (no logic copied, C-8 / NFR-07). Handles come from `ctx`; read-only resources
//! from `shared`; rayon from `shared.rayon_pool`. A job touches ONLY its
//! `PerSlugTickContext` handle set + read-only `shared` + rayon — the sole
//! mutation route (AC-4, the per-slug funnel).
//!
//! Registry order (see `build_job_registry`) IS the ordering invariant.

use std::sync::Arc;

use async_trait::async_trait;

use unimatrix_observe::domain::DomainPackRegistry;

use crate::services::co_access_promotion_tick::run_co_access_promotion_tick;
use crate::services::contradiction_cache::CONTRADICTION_SCAN_INTERVAL_TICKS;
use crate::services::graph_enrichment_tick::run_graph_enrichment_tick;
use crate::services::nli_detection_tick::run_graph_inference_tick;
use crate::services::status::StatusService;

use super::job::{BackgroundJob, Cadence, PerSlugTickContext, ResourceClass, SharedTickResources};
use super::{
    extraction_tick, maintenance_tick, now_secs, run_contradiction_scan,
    run_orphaned_edge_compaction, run_phase_freq_rebuild, run_typed_graph_rebuild,
};

/// Job 1 — maintenance tick (effectiveness classification + auto-quarantine +
/// existing maintenance). Delegates to `maintenance_tick` (`background.rs:814`).
pub struct MaintenanceJob;

#[async_trait]
impl BackgroundJob for MaintenanceJob {
    fn name(&self) -> &str {
        "maintenance"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Io
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String> {
        // The background maintenance path uses load_maintenance_snapshot (not
        // compute_report), so the observation registry is not consulted — use the
        // built-in default exactly as run_single_tick does (col-023).
        let tick_observation_registry = Arc::new(DomainPackRegistry::with_builtin_claude_code());
        let status_svc = StatusService::new(
            Arc::clone(&ctx.store),
            Arc::clone(&ctx.vector_index),
            Arc::clone(&shared.embed_service),
            Arc::clone(&ctx.adapt_service),
            ctx.confidence.clone(),
            Arc::clone(&shared.confidence_params),
            Arc::clone(&ctx.contradiction),
            Arc::clone(&shared.rayon_pool),
            tick_observation_registry,
            Arc::clone(&shared.category_allowlist),
        );

        let tick_start = now_secs();
        match tokio::time::timeout(
            super::TICK_TIMEOUT,
            maintenance_tick(
                &status_svc,
                &ctx.session_registry,
                &ctx.store,
                &ctx.pending_entries,
                &ctx.effectiveness,
                &ctx.audit,
                shared.auto_quarantine_cycles,
                &ctx.store,
                &shared.inference_config,
                &shared.category_allowlist,
                &shared.retention_config,
            ),
        )
        .await
        {
            Ok(Ok(())) => {
                if let Ok(mut meta) = ctx.tick_metadata.lock() {
                    meta.last_maintenance_run = Some(tick_start);
                }
                tracing::info!(slug = %ctx.slug, "maintenance tick complete");
            }
            Ok(Err(e)) => tracing::warn!(slug = %ctx.slug, "maintenance tick failed: {e}"),
            Err(_) => tracing::warn!(
                slug = %ctx.slug,
                timeout_secs = super::TICK_TIMEOUT.as_secs(),
                "maintenance tick timed out; will retry next cycle"
            ),
        }
        Ok(())
    }
}

/// Job 2 — GRAPH_EDGES orphaned-edge compaction. Runs AFTER maintenance, BEFORE
/// co-access promotion + typed-graph rebuild (ordering invariant).
pub struct OrphanedEdgeCompactionJob;

#[async_trait]
impl BackgroundJob for OrphanedEdgeCompactionJob {
    fn name(&self) -> &str {
        "orphaned_edge_compaction"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Io
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        _shared: &SharedTickResources,
    ) -> Result<(), String> {
        run_orphaned_edge_compaction(&ctx.store).await;
        Ok(())
    }
}

/// Job 3 — co-access promotion. ORDERING INVARIANT: AFTER compaction, BEFORE
/// typed-graph rebuild (`background.rs:546-551`).
pub struct CoAccessPromotionJob;

#[async_trait]
impl BackgroundJob for CoAccessPromotionJob {
    fn name(&self) -> &str {
        "co_access_promotion"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Io
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String> {
        let current_tick = ctx
            .tick_metadata
            .lock()
            .map(|m| m.tick_counter.wrapping_sub(1))
            .unwrap_or(0);
        run_co_access_promotion_tick(&ctx.store, &shared.inference_config, current_tick).await;
        Ok(())
    }
}

/// Job 4 — typed graph state rebuild (PPR substrate). Rayon-class.
pub struct TypedGraphRebuildJob;

#[async_trait]
impl BackgroundJob for TypedGraphRebuildJob {
    fn name(&self) -> &str {
        "typed_graph_rebuild"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Rayon
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        _shared: &SharedTickResources,
    ) -> Result<(), String> {
        run_typed_graph_rebuild(&ctx.store, &ctx.typed_graph).await;
        Ok(())
    }
}

/// Job 5 — phase-conditioned frequency table rebuild (col-031).
pub struct PhaseFreqRebuildJob;

#[async_trait]
impl BackgroundJob for PhaseFreqRebuildJob {
    fn name(&self) -> &str {
        "phase_freq_rebuild"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Io
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String> {
        run_phase_freq_rebuild(&ctx.store, &shared.inference_config, &ctx.phase_freq).await;
        Ok(())
    }
}

/// Job 6 — contradiction scan. INTERVAL-GATED (`EveryN`), preserving today's
/// `current_tick % CONTRADICTION_SCAN_INTERVAL_TICKS` gating, now per-slug.
pub struct ContradictionScanJob;

#[async_trait]
impl BackgroundJob for ContradictionScanJob {
    fn name(&self) -> &str {
        "contradiction_scan"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryN(CONTRADICTION_SCAN_INTERVAL_TICKS)
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Rayon
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String> {
        // The loop only calls run() when the cadence fires; pass the fired tick
        // value purely for logging parity with the legacy path.
        let current_tick = ctx
            .tick_metadata
            .lock()
            .map(|m| m.tick_counter.wrapping_sub(1))
            .unwrap_or(0);
        run_contradiction_scan(
            &ctx.store,
            &ctx.vector_index,
            &shared.embed_service,
            &shared.rayon_pool,
            &ctx.contradiction,
            current_tick,
        )
        .await;
        Ok(())
    }
}

/// Job 7 — extraction tick (observation → proposals → quality-gate → store).
/// Rayon-class (spawn_blocking + rayon quality gate). Owns the per-slug
/// watermark + neural enhancer via `ctx.mutable`.
pub struct ExtractionJob;

#[async_trait]
impl BackgroundJob for ExtractionJob {
    fn name(&self) -> &str {
        "extraction"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Rayon
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String> {
        // Move the per-slug mutable state OUT of the Mutex for the duration of the
        // async op — a std MutexGuard is not Send and cannot be held across the
        // `.await` (the tick task is spawned on a multi-thread runtime). The serial
        // loop guarantees no other slug contends, so the take-run-restore window is
        // safe; this also matches the legacy loop where extraction_ctx + the neural
        // enhancer were task-local owned (not shared) state.
        let mut extraction_ctx;
        let neural_enhancer;
        let mut shadow_evaluator;
        {
            let mut mutable = ctx.mutable.lock().unwrap_or_else(|e| e.into_inner());
            extraction_ctx = mutable.extraction_ctx.clone();
            neural_enhancer = mutable.neural_enhancer.take();
            shadow_evaluator = mutable.shadow_evaluator.take();
        }

        let result = tokio::time::timeout(
            super::TICK_TIMEOUT,
            extraction_tick(
                &ctx.store,
                &ctx.vector_index,
                &shared.embed_service,
                &mut extraction_ctx,
                neural_enhancer.as_ref(),
                shadow_evaluator.as_mut(),
                &shared.rayon_pool,
            ),
        )
        .await;

        // Restore the (possibly advanced) per-slug state back into the Mutex.
        {
            let mut mutable = ctx.mutable.lock().unwrap_or_else(|e| e.into_inner());
            mutable.extraction_ctx = extraction_ctx;
            mutable.neural_enhancer = neural_enhancer;
            mutable.shadow_evaluator = shadow_evaluator;
        }

        match result {
            Ok(Ok((stats, friction_recs, dead_knowledge_recs))) => {
                if let Ok(mut meta) = ctx.tick_metadata.lock() {
                    meta.last_extraction_run = Some(now_secs());
                    meta.extraction_stats = stats;
                    meta.friction_signals = friction_recs;
                    meta.dead_knowledge_signals = dead_knowledge_recs;
                }
                tracing::info!(slug = %ctx.slug, "extraction tick complete");
            }
            Ok(Err(e)) => tracing::warn!(slug = %ctx.slug, "extraction tick failed: {e}"),
            Err(_) => tracing::warn!(
                slug = %ctx.slug,
                timeout_secs = super::TICK_TIMEOUT.as_secs(),
                "extraction tick timed out; will retry next cycle"
            ),
        }
        Ok(())
    }
}

/// Job 8 — structural graph + NLI inference. Rayon-class. Reads the ONE shared
/// loaded NLI model (`shared.nli_handle`).
pub struct GraphInferenceJob;

#[async_trait]
impl BackgroundJob for GraphInferenceJob {
    fn name(&self) -> &str {
        "graph_inference"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Rayon
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String> {
        run_graph_inference_tick(
            &ctx.store,
            &shared.nli_handle,
            &ctx.vector_index,
            &shared.rayon_pool,
            &shared.inference_config,
        )
        .await;
        Ok(())
    }
}

/// Job 9 — graph enrichment (S1/S2 always, S8 internally interval-gated, crt-041).
pub struct GraphEnrichmentJob;

#[async_trait]
impl BackgroundJob for GraphEnrichmentJob {
    fn name(&self) -> &str {
        "graph_enrichment"
    }
    fn cadence(&self) -> Cadence {
        Cadence::EveryTick
    }
    fn resource_class(&self) -> ResourceClass {
        ResourceClass::Io
    }
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String> {
        let current_tick = ctx
            .tick_metadata
            .lock()
            .map(|m| m.tick_counter.wrapping_sub(1))
            .unwrap_or(0);
        run_graph_enrichment_tick(&ctx.store, &shared.inference_config, current_tick).await;
        Ok(())
    }
}
