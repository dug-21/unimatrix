//! crt-056 Wave 2 — the `BackgroundJob` work-unit seam (ADR-004, #5167) and the
//! per-slug borrow bundle `PerSlugTickContext` (ADR-003, #5166).
//!
//! This is the **shape**, not a scheduler. It defines just enough to express
//! today's tick operations as registered jobs over one slug's analytics handle
//! set + read-only shared resources. There is deliberately NO queue, worker
//! pool, residency/eviction, cron, cadence-signal, or concurrent-rayon
//! machinery (C-2, R-09 — Step B stays out). `ResourceClass` is a declaration
//! only; nothing in crt-056 reads it.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use unimatrix_core::{Store, VectorIndex};
use unimatrix_engine::confidence::ConfidenceParams;
use unimatrix_observe::extraction::ExtractionContext;
use unimatrix_observe::extraction::neural::NeuralEnhancer;
use unimatrix_observe::extraction::shadow::ShadowEvaluator;

use unimatrix_adapt::AdaptationService;

use crate::http::ProjectSlug;
use crate::infra::audit::AuditLog;
use crate::infra::categories::CategoryAllowlist;
use crate::infra::config::{InferenceConfig, RetentionConfig};
use crate::infra::embed_handle::EmbedServiceHandle;
use crate::infra::nli_handle::NliServiceHandle;
use crate::infra::rayon_pool::RayonPool;
use crate::infra::session::SessionRegistry;
use crate::server::PendingEntriesAnalysis;
use crate::services::ServiceLayer;
use crate::services::confidence::ConfidenceStateHandle;
use crate::services::contradiction_cache::ContradictionScanCacheHandle;
use crate::services::effectiveness::EffectivenessStateHandle;
use crate::services::phase_freq_table::PhaseFreqTableHandle;
use crate::services::typed_graph::TypedGraphStateHandle;

use super::TickMetadata;

/// Per-slug mutable scratch state for the tick loop (crt-056).
///
/// The pre-crt-056 single-store tick loop (`background_tick_loop`) owned an
/// `ExtractionContext` (the observation watermark) plus the neural enhancer and
/// its shadow evaluator as task-local mutable state. Under the per-slug serial
/// loop each slug MUST own its OWN copy — otherwise slug A's watermark would
/// skip slug B's observations (a cross-slug correctness defect, R-02 adjacent).
///
/// Held behind a `Mutex` because `BackgroundJob::run` takes `&self`/`&ctx`
/// (shared refs) but extraction mutates the watermark + stats. The serial loop
/// means the mutex is never contended across slugs; it exists purely to satisfy
/// the shared-ref interface.
pub struct TickMutableState {
    /// Per-slug observation watermark + extraction stats.
    pub extraction_ctx: ExtractionContext,
    /// Per-slug neural enhancer (shadow mode). `None` if init failed.
    pub neural_enhancer: Option<NeuralEnhancer>,
    /// Per-slug shadow evaluator paired with `neural_enhancer`.
    pub shadow_evaluator: Option<ShadowEvaluator>,
}

impl std::fmt::Debug for TickMutableState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TickMutableState")
            .field("extraction_ctx", &self.extraction_ctx)
            .field("has_neural_enhancer", &self.neural_enhancer.is_some())
            .finish_non_exhaustive()
    }
}

/// A thin **borrow** bundle of one slug's tick work-unit state (ADR-003).
///
/// The five analytics handle fields are `Arc::clone`s of the slug's
/// [`ServiceLayer`] `*_handle()` accessors — the SAME `Arc<RwLock<_>>` the
/// serving path reads, so what the tick writes is exactly what serving reads
/// (in-memory hot path, principle 7; FR-15). They are NEVER freshly constructed
/// handles (pattern #4097: a copied inner `T` makes post-tick writes invisible).
/// `Arc::ptr_eq` between a context handle and the serving accessor is the test.
///
/// `tick_metadata` is the slug's own `Arc<Mutex<TickMetadata>>` so the per-slug
/// counter falls out for free (ADR-005, R-07) — interval gates fire per-slug,
/// not loop-global.
pub struct PerSlugTickContext {
    /// The slug this context ticks (the daemon's own context uses [`Self::DAEMON_SLUG`]).
    pub slug: ProjectSlug,
    /// The slug's own store (sole write capability for its analytics).
    pub store: Arc<Store>,
    /// The slug's own vector index.
    pub vector_index: Arc<VectorIndex>,

    // ── the borrowed analytics handle set (Arc::clone of the ServiceLayer's) ──
    pub confidence: ConfidenceStateHandle,
    pub effectiveness: EffectivenessStateHandle,
    pub typed_graph: TypedGraphStateHandle,
    pub contradiction: ContradictionScanCacheHandle,
    pub phase_freq: PhaseFreqTableHandle,

    /// Per-slug tick counter (ADR-005). Read+incremented via [`Self::next_tick`].
    pub tick_metadata: Arc<Mutex<TickMetadata>>,

    // ── per-slug subsystems the existing ops need (per-slug owned, not shared) ──
    /// Per-slug adaptation service (ADR-006: per-slug independent state).
    pub adapt_service: Arc<AdaptationService>,
    /// Per-slug session registry (stale-session sweep during maintenance).
    pub session_registry: Arc<SessionRegistry>,
    /// Per-slug pending-entries analysis buffer.
    pub pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
    /// Per-slug audit log (analytics writes route through the slug's store).
    pub audit: Arc<AuditLog>,

    /// Per-slug mutable scratch state (extraction watermark, neural enhancer).
    pub mutable: Arc<Mutex<TickMutableState>>,
}

impl std::fmt::Debug for PerSlugTickContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerSlugTickContext")
            .field("slug", &self.slug)
            .finish_non_exhaustive()
    }
}

impl PerSlugTickContext {
    /// The synthetic slug identity for the single-project daemon's own context.
    ///
    /// The daemon serves one store with no `[[projects]]` slug; it still ticks
    /// through the SAME serial loop (C-6, one isolation seam). "daemon" passes
    /// the [`ProjectSlug`] allowlist charset.
    pub const DAEMON_SLUG: &'static str = "daemon";

    /// Build a context from a slug's [`ServiceLayer`] + per-slug subsystems.
    ///
    /// INVARIANT (ADR-003, pattern #4097): every handle field is an `Arc::clone`
    /// of the slug's `ServiceLayer` accessor — NEVER a freshly-constructed handle.
    #[allow(clippy::too_many_arguments)]
    pub fn from_service_layer(
        slug: ProjectSlug,
        store: Arc<Store>,
        vector_index: Arc<VectorIndex>,
        sl: &ServiceLayer,
        tick_metadata: Arc<Mutex<TickMetadata>>,
        adapt_service: Arc<AdaptationService>,
        session_registry: Arc<SessionRegistry>,
        pending_entries: Arc<Mutex<PendingEntriesAnalysis>>,
        audit: Arc<AuditLog>,
    ) -> Self {
        let (neural_enhancer, shadow_evaluator) = match super::init_neural_enhancer() {
            Some((e, s)) => (Some(e), Some(s)),
            None => (None, None),
        };
        PerSlugTickContext {
            slug,
            store,
            vector_index,
            confidence: sl.confidence_state_handle(),
            effectiveness: sl.effectiveness_state_handle(),
            typed_graph: sl.typed_graph_handle(),
            contradiction: sl.contradiction_cache_handle(),
            phase_freq: sl.phase_freq_table_handle(),
            tick_metadata,
            adapt_service,
            session_registry,
            pending_entries,
            audit,
            mutable: Arc::new(Mutex::new(TickMutableState {
                extraction_ctx: ExtractionContext::new(),
                neural_enhancer,
                shadow_evaluator,
            })),
        }
    }

    /// Read + increment THIS slug's tick counter (mirrors `background.rs:352-358`).
    ///
    /// Poison-tolerant and wrapping, exactly as the legacy loop. The returned
    /// value is the gate input for [`Cadence::fires`].
    pub fn next_tick(&self) -> u32 {
        let mut meta = self.tick_metadata.lock().unwrap_or_else(|e| e.into_inner());
        let t = meta.tick_counter;
        meta.tick_counter = meta.tick_counter.wrapping_add(1);
        t
    }
}

/// How often a job fires (ADR-004). A STATIC predicate — never a scheduler input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cadence {
    /// Fires on every tick.
    EveryTick,
    /// Fires when `tick % n == 0`. `EveryN(0)` never fires (div-by-zero guard).
    EveryN(u32),
}

impl Cadence {
    /// Whether this cadence fires on tick `t`.
    pub fn fires(&self, t: u32) -> bool {
        match self {
            Cadence::EveryTick => true,
            Cadence::EveryN(n) => *n != 0 && t.is_multiple_of(*n),
        }
    }
}

/// Resource class a job tags itself with (ADR-004).
///
/// DECLARATION ONLY — nothing in crt-056 reads it. It is a forward hook so Step
/// B's semaphore can group jobs without changing the work-unit. A reader of
/// `resource_class()` that gates execution would be Step B leakage (R-09).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceClass {
    Io,
    Rayon,
}

/// Read-only resources shared across all per-slug contexts in a tick pass
/// (ADR-004). All fields are `Arc` and read-only on the inference path; no
/// cross-context mutable state crosses `BackgroundJob::run`.
#[derive(Clone)]
pub struct SharedTickResources {
    pub embed_service: Arc<EmbedServiceHandle>,
    /// The ONE loaded NLI model — `Arc::clone`d, never rebuilt (C-3, AC-2).
    pub nli_handle: Arc<NliServiceHandle>,
    pub inference_config: Arc<InferenceConfig>,
    pub confidence_params: Arc<ConfidenceParams>,
    pub rayon_pool: Arc<RayonPool>,
    pub category_allowlist: Arc<CategoryAllowlist>,
    pub retention_config: Arc<RetentionConfig>,
    /// Auto-quarantine threshold (Copy scalar, parsed once at boot).
    pub auto_quarantine_cycles: u32,
    /// Configured tick interval in seconds (for `next_scheduled` reporting).
    pub tick_interval_secs: u64,
}

impl std::fmt::Debug for SharedTickResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedTickResources")
            .field("auto_quarantine_cycles", &self.auto_quarantine_cycles)
            .field("tick_interval_secs", &self.tick_interval_secs)
            .finish_non_exhaustive()
    }
}

/// The work-unit seam (ADR-004). `run` touches ONLY `ctx`'s handle set +
/// `shared` (read-only) + the rayon pool. NO cross-context shared mutable state.
///
/// `run` has NO trait-default body (anti-bypass, #4974 checklist 4): a `{}` /
/// `{ Ok(()) }` default could reintroduce a silent no-op. Every job MUST
/// implement it explicitly.
#[async_trait]
pub trait BackgroundJob: Send + Sync {
    fn name(&self) -> &str;
    fn cadence(&self) -> Cadence;
    fn resource_class(&self) -> ResourceClass;
    async fn run(
        &self,
        ctx: &PerSlugTickContext,
        shared: &SharedTickResources,
    ) -> Result<(), String>;
}

/// Build the static job registry — today's 9 ops as the first jobs, IN the
/// ordering invariant (`background.rs:546-551`): co-access promotion runs AFTER
/// orphaned-edge compaction and BEFORE TypedGraph rebuild. Registry order IS the
/// ordering invariant; jobs are NOT reordered.
pub fn build_job_registry() -> Vec<Box<dyn BackgroundJob>> {
    use super::jobs::*;
    vec![
        Box::new(MaintenanceJob),
        Box::new(OrphanedEdgeCompactionJob),
        Box::new(CoAccessPromotionJob),
        Box::new(TypedGraphRebuildJob),
        Box::new(PhaseFreqRebuildJob),
        Box::new(ContradictionScanJob),
        Box::new(ExtractionJob),
        Box::new(GraphInferenceJob),
        Box::new(GraphEnrichmentJob),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cadence_every_tick_always_fires() {
        for t in [0u32, 1, 2, 3, 7, 100, u32::MAX] {
            assert!(Cadence::EveryTick.fires(t), "EveryTick must fire on {t}");
        }
    }

    #[test]
    fn test_cadence_every_n_fires_on_multiples() {
        let c = Cadence::EveryN(4);
        assert!(c.fires(0));
        assert!(c.fires(4));
        assert!(c.fires(8));
        assert!(!c.fires(1));
        assert!(!c.fires(2));
        assert!(!c.fires(3));
        assert!(!c.fires(5));
    }

    #[test]
    fn test_cadence_every_n_zero_never_fires() {
        // div-by-zero guard: EveryN(0) must never fire (and never panic).
        let c = Cadence::EveryN(0);
        for t in [0u32, 1, 2, 100] {
            assert!(!c.fires(t), "EveryN(0) must never fire (guard) at {t}");
        }
    }

    #[test]
    fn test_daemon_slug_is_valid_project_slug() {
        // The synthetic daemon slug must pass the ProjectSlug allowlist charset.
        let slug = ProjectSlug::try_from(PerSlugTickContext::DAEMON_SLUG);
        assert!(slug.is_ok(), "DAEMON_SLUG must be a valid ProjectSlug");
    }

    #[test]
    fn test_job_registry_preserves_op_order() {
        let registry = build_job_registry();
        let names: Vec<&str> = registry.iter().map(|j| j.name()).collect();
        assert_eq!(
            names,
            vec![
                "maintenance",
                "orphaned_edge_compaction",
                "co_access_promotion",
                "typed_graph_rebuild",
                "phase_freq_rebuild",
                "contradiction_scan",
                "extraction",
                "graph_inference",
                "graph_enrichment",
            ],
            "registry order IS the ordering invariant (co-access promotion AFTER \
             compaction, BEFORE typed-graph rebuild)"
        );
    }

    #[test]
    fn test_registered_jobs_declare_expected_cadence() {
        let registry = build_job_registry();
        for job in &registry {
            match job.name() {
                "contradiction_scan" => assert!(
                    matches!(job.cadence(), Cadence::EveryN(_)),
                    "contradiction scan must be EveryN (existing interval gate)"
                ),
                other => assert_eq!(
                    job.cadence(),
                    Cadence::EveryTick,
                    "{other} must be EveryTick (preserves today's gating)"
                ),
            }
        }
    }

    #[test]
    fn test_registered_jobs_declare_expected_resource_class() {
        let registry = build_job_registry();
        for job in &registry {
            let expected = match job.name() {
                "typed_graph_rebuild" | "contradiction_scan" | "extraction" | "graph_inference" => {
                    ResourceClass::Rayon
                }
                _ => ResourceClass::Io,
            };
            assert_eq!(
                job.resource_class(),
                expected,
                "{} resource_class mismatch",
                job.name()
            );
        }
    }

    /// Build a `PerSlugTickContext` from a real test server (its config-driven
    /// ServiceLayer + per-slug subsystems).
    async fn ctx_from_test_server(
        slug: &str,
    ) -> (crate::server::UnimatrixServer, PerSlugTickContext) {
        let server = crate::server::tests::make_server().await;
        let sl = server.service_layer();
        let ctx = PerSlugTickContext::from_service_layer(
            ProjectSlug::try_from(slug).expect("valid slug"),
            Arc::clone(&server.store),
            server.vector_index(),
            sl,
            server.tick_metadata(),
            server.adapt_service(),
            Arc::clone(&server.session_registry),
            Arc::clone(&server.pending_entries_analysis),
            server.audit_log(),
        );
        (server, ctx)
    }

    /// AC-5 (structural) — R-03.2: context handles ARE the ServiceLayer's handles
    /// (`Arc::ptr_eq`), not freshly constructed instances. A new RwLock would pass
    /// an "exists/changed" test while serving reads a stale instance.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_per_slug_context_handles_are_service_layer_arcs() {
        let (server, ctx) = ctx_from_test_server("slug-a").await;
        let sl = server.service_layer();
        assert!(
            Arc::ptr_eq(&ctx.confidence, &sl.confidence_state_handle()),
            "confidence handle must be the ServiceLayer's Arc"
        );
        assert!(
            Arc::ptr_eq(&ctx.effectiveness, &sl.effectiveness_state_handle()),
            "effectiveness handle must be the ServiceLayer's Arc"
        );
        assert!(
            Arc::ptr_eq(&ctx.typed_graph, &sl.typed_graph_handle()),
            "typed_graph handle must be the ServiceLayer's Arc"
        );
        assert!(
            Arc::ptr_eq(&ctx.contradiction, &sl.contradiction_cache_handle()),
            "contradiction handle must be the ServiceLayer's Arc"
        );
        assert!(
            Arc::ptr_eq(&ctx.phase_freq, &sl.phase_freq_table_handle()),
            "phase_freq handle must be the ServiceLayer's Arc"
        );
    }

    /// FR-11 — the context's store is the server's own store (R-02 adjacent).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_per_slug_context_store_is_slug_store() {
        let (server, ctx) = ctx_from_test_server("slug-a").await;
        assert!(
            Arc::ptr_eq(&ctx.store, &server.store),
            "context store must be the slug's own store"
        );
    }

    /// ADR-005 — two contexts own DISTINCT tick_metadata (per-slug counter).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_per_slug_context_owns_distinct_tick_metadata() {
        let (_sa, ctx_a) = ctx_from_test_server("slug-a").await;
        let (_sb, ctx_b) = ctx_from_test_server("slug-b").await;
        assert!(
            !Arc::ptr_eq(&ctx_a.tick_metadata, &ctx_b.tick_metadata),
            "each slug must own its own Arc<Mutex<TickMetadata>>"
        );
    }

    /// R-07 — per-slug counters advance independently; an EveryN(4) gate fires on
    /// one context but not the other when they are at different offsets.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_interval_gate_fires_per_slug_independently() {
        let (_sa, ctx_a) = ctx_from_test_server("slug-a").await;
        let (_sb, ctx_b) = ctx_from_test_server("slug-b").await;

        // Offset B by one tick so the two counters are out of phase.
        let _ = ctx_b.next_tick(); // B advances to 1

        let gate = Cadence::EveryN(4);
        // A starts at 0 ⇒ fires; B is at 1 ⇒ does not fire, on the same pass.
        let t_a = ctx_a.next_tick();
        let t_b = ctx_b.next_tick();
        assert!(gate.fires(t_a), "A at tick {t_a} must fire EveryN(4)");
        assert!(!gate.fires(t_b), "B at tick {t_b} must NOT fire EveryN(4)");
    }

    /// Per-slug counter advances by exactly 1 per `next_tick`, read from its OWN
    /// tick_metadata (no shared counter).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_each_context_counter_advances_independently() {
        let (_s, ctx) = ctx_from_test_server("slug-a").await;
        let t0 = ctx.next_tick();
        let t1 = ctx.next_tick();
        assert_eq!(t0, 0);
        assert_eq!(t1, 1);
        let stored = ctx
            .tick_metadata
            .lock()
            .map(|m| m.tick_counter)
            .unwrap_or(0);
        assert_eq!(stored, 2, "counter must be 2 after two next_tick calls");
    }
}
