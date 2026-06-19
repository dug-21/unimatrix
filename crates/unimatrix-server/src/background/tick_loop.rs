//! crt-056 Wave 2 — the per-slug serial tick loop (ADR-005, #5168).
//!
//! Drives the registered `BackgroundJob`s over each registered
//! `PerSlugTickContext` **serially**, one slug at a time. Serial execution gives
//! serialized rayon for free (C-1, C-3, R-08) and lets each slug use its OWN
//! `tick_counter` (R-07).
//!
//! Scope of this path (C-6, one isolation seam): the **multi-project HTTP
//! daemon** drives this loop exclusively — its own context plus one
//! `PerSlugTickContext` per registered slug, all over per-slug stores. The legacy
//! global-handle `spawn_background_tick` is **RETIRED on that daemon path**: the
//! HTTP boot has no global-handle extraction and never calls it (see
//! `main.rs` HTTP branch → `spawn_per_slug_tick`). That retirement is the
//! corruption-relevant guarantee (R-02/NFR-5): no two slugs can ever share a
//! global analytics handle, because there is no global handle on the daemon path.
//!
//! Carve-out (accepted scope): the **stdio single-store path** (`tokio_main_stdio`)
//! is single-project, N=1, with NO `[[projects]]` and NO per-slug servers. It
//! retains the legacy single-store `spawn_background_tick` over the one global
//! handle set. This is correct, not a divergence the NFR-5 corruption hazard
//! cares about: that hazard requires N>=2 slugs sharing global handles, which the
//! stdio single store cannot represent. So the "global-handle tick is retired"
//! claim is scoped to the daemon path; stdio is the deliberate single-store
//! carve-out, never wired through `spawn_per_slug_tick`.

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use super::job::{BackgroundJob, PerSlugTickContext, SharedTickResources};
use super::{build_job_registry, read_tick_interval};

/// Run one loop pass over all contexts: for each slug, advance its per-slug
/// counter and run every job whose cadence fires, in registry order.
///
/// Per-slug, per-job failure isolation: an `Err` from `job.run` is logged and
/// the loop continues (next job, then next slug). One slug's failure never
/// aborts another's tick (mirrors `background.rs:393-395`). An empty `contexts`
/// slice is a no-op (N=0 edge case).
///
/// Serialization is structural: a job's rayon work is entered+exited WITHIN
/// `job.run` for that single slug; the loop NEVER wraps all N contexts in one
/// rayon closure (FR-17, NFR-3, R-08).
pub async fn run_per_slug_tick_pass(
    contexts: &[PerSlugTickContext],
    registry: &[Box<dyn BackgroundJob>],
    shared: &SharedTickResources,
) {
    for ctx in contexts {
        // PER-SLUG counter (FR-18, R-07): read+increment THIS slug's counter only.
        let current_tick = ctx.next_tick();
        for job in registry {
            if job.cadence().fires(current_tick) {
                match job.run(ctx, shared).await {
                    Ok(()) => {}
                    Err(e) => {
                        tracing::error!(
                            slug = %ctx.slug,
                            job = job.name(),
                            "per-slug tick job failed: {e}; continuing"
                        );
                    }
                }
            }
        }
        // Update next-scheduled on this slug's metadata (status reporting parity).
        if let Ok(mut meta) = ctx.tick_metadata.lock() {
            meta.next_scheduled = Some(super::now_secs() + shared.tick_interval_secs);
        }
    }
}

/// Spawn the per-slug serial tick loop (the multi-project HTTP daemon's tick
/// driver; replaces `spawn_background_tick` ON THE DAEMON PATH — the stdio
/// single-store path retains the legacy `spawn_background_tick`, see module doc).
///
/// Returns a `JoinHandle` for the outer supervisor (stored in
/// `LifecycleHandles.tick_handle`, aborted on graceful shutdown). The supervisor
/// wraps the inner interval loop; on inner panic it logs and restarts after a
/// 30-second cooldown; on cancellation (graceful shutdown) it exits cleanly.
/// This is the EXISTING `spawn_background_tick` supervisor structure
/// (`background.rs:286-301`) re-targeted from a single global store to a
/// `Vec<PerSlugTickContext>`.
pub fn spawn_per_slug_tick(
    contexts: Vec<PerSlugTickContext>,
    shared: SharedTickResources,
) -> JoinHandle<()> {
    // `contexts` is wrapped in an `Arc` so it can be cheaply handed to a fresh
    // inner task on each panic-restart (the contexts themselves are not `Clone`;
    // their handle fields are `Arc::clone`s, so sharing the whole set is correct
    // and cheap). `SharedTickResources` is `Clone` (all-`Arc`).
    let contexts = Arc::new(contexts);
    // Outer supervisor — this handle is stored as tick_handle and aborted on shutdown.
    tokio::spawn(async move {
        loop {
            let contexts = Arc::clone(&contexts);
            let shared = shared.clone();
            // Inner task: the interval loop. If it panics, the JoinError is caught
            // here (not killing the supervisor); on cancel the supervisor exits.
            let inner_handle = tokio::spawn(run_interval_loop(contexts, shared));

            match inner_handle.await {
                // Normal return: should not happen in practice.
                Ok(()) => break,
                // Cancelled: outer handle aborted by graceful_shutdown — exit cleanly.
                Err(ref join_err) if join_err.is_cancelled() => break,
                // Panic: log and restart after a 30-second cooldown (#276).
                Err(join_err) => {
                    tracing::error!(
                        error = %join_err,
                        "per-slug background tick panicked; restarting in 30s"
                    );
                    tokio::time::sleep(Duration::from_secs(30)).await;
                }
            }
        }
    })
}

/// The interval loop: build the registry once, skip the immediate t=0 tick, then
/// run one pass per interval. Mirrors `background_tick_loop` (`background.rs:330-396`).
async fn run_interval_loop(contexts: Arc<Vec<PerSlugTickContext>>, shared: SharedTickResources) {
    let registry = build_job_registry();
    let tick_interval_secs = read_tick_interval();
    let mut interval = tokio::time::interval(Duration::from_secs(tick_interval_secs));

    // Skip the immediate first tick (fires at t=0) — existing behavior.
    interval.tick().await;

    loop {
        interval.tick().await;
        run_per_slug_tick_pass(&contexts, &registry, &shared).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::background::job::{
        BackgroundJob, Cadence, PerSlugTickContext, ResourceClass, SharedTickResources,
    };
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A no-op job that records how many times it ran (AC-7a registry dispatch).
    struct CountingJob {
        name: String,
        cadence: Cadence,
        count: Arc<AtomicU32>,
    }

    #[async_trait]
    impl BackgroundJob for CountingJob {
        fn name(&self) -> &str {
            &self.name
        }
        fn cadence(&self) -> Cadence {
            self.cadence
        }
        fn resource_class(&self) -> ResourceClass {
            ResourceClass::Io
        }
        async fn run(
            &self,
            _ctx: &PerSlugTickContext,
            _shared: &SharedTickResources,
        ) -> Result<(), String> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_empty_loop_is_noop_no_panic() {
        // N=0 contexts: a pass over an empty slice is a no-op and never panics.
        let shared = crate::background::tick_loop::tests::test_shared();
        let registry: Vec<Box<dyn BackgroundJob>> = vec![];
        run_per_slug_tick_pass(&[], &registry, &shared).await;
    }

    /// Build a context from a fresh test server (helper for loop-level tests).
    async fn ctx(slug: &str) -> (crate::server::UnimatrixServer, PerSlugTickContext) {
        use crate::http::ProjectSlug;
        let server = crate::server::tests::make_server().await;
        let c = PerSlugTickContext::from_service_layer(
            ProjectSlug::try_from(slug).expect("valid slug"),
            Arc::clone(&server.store),
            server.vector_index(),
            server.service_layer(),
            server.tick_metadata(),
            server.adapt_service(),
            Arc::clone(&server.session_registry),
            Arc::clone(&server.pending_entries_analysis),
            server.audit_log(),
        );
        (server, c)
    }

    /// AC-7a / R-01.3 — a registered no-op job runs with ZERO loop-body edits, and
    /// an unregistered job never runs (registry-derived dispatch, not hardcoded).
    #[tokio::test(flavor = "multi_thread")]
    async fn test_noop_job_runs_when_registered_unregistered_does_not() {
        let (_s, c) = ctx("slug-a").await;
        let registered_count = Arc::new(AtomicU32::new(0));
        let unregistered_count = Arc::new(AtomicU32::new(0));

        // Only the registered job is in the registry.
        let registry: Vec<Box<dyn BackgroundJob>> = vec![Box::new(CountingJob {
            name: "registered".to_string(),
            cadence: Cadence::EveryTick,
            count: Arc::clone(&registered_count),
        })];
        // The unregistered job exists but is NOT in the registry.
        let _unregistered = CountingJob {
            name: "unregistered".to_string(),
            cadence: Cadence::EveryTick,
            count: Arc::clone(&unregistered_count),
        };

        let shared = test_shared();
        run_per_slug_tick_pass(std::slice::from_ref(&c), &registry, &shared).await;

        assert_eq!(
            registered_count.load(Ordering::SeqCst),
            1,
            "registered job must run"
        );
        assert_eq!(
            unregistered_count.load(Ordering::SeqCst),
            0,
            "unregistered job must NOT run (registry-derived)"
        );
    }

    /// FR-12 — N registered contexts ⇒ each visited exactly once per pass.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_loop_visits_each_registered_context_once() {
        let (_sa, ca) = ctx("slug-a").await;
        let (_sb, cb) = ctx("slug-b").await;
        let count = Arc::new(AtomicU32::new(0));
        let registry: Vec<Box<dyn BackgroundJob>> = vec![Box::new(CountingJob {
            name: "counter".to_string(),
            cadence: Cadence::EveryTick,
            count: Arc::clone(&count),
        })];
        let shared = test_shared();
        run_per_slug_tick_pass(&[ca, cb], &registry, &shared).await;
        // One EveryTick job over two contexts ⇒ 2 runs.
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    /// R-07.1 — per-slug interval gate fires independently: with an EveryN(4) job
    /// and two contexts at different offsets, only the on-boundary slug runs it.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_interval_gate_fires_per_slug_independently_through_loop() {
        let (_sa, ca) = ctx("slug-a").await; // counter starts at 0
        let (_sb, cb) = ctx("slug-b").await;
        cb.next_tick(); // advance B to 1 so it is off the EveryN(4) boundary

        let count = Arc::new(AtomicU32::new(0));
        let registry: Vec<Box<dyn BackgroundJob>> = vec![Box::new(CountingJob {
            name: "interval".to_string(),
            cadence: Cadence::EveryN(4),
            count: Arc::clone(&count),
        })];
        let shared = test_shared();
        // A ticks at 0 (fires), B ticks at 1 (does not fire) ⇒ exactly 1 run.
        run_per_slug_tick_pass(&[ca, cb], &registry, &shared).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            1,
            "EveryN(4) must fire only for the on-boundary slug"
        );
    }

    /// Per-slug failure isolation — a failing job for one context is logged and the
    /// loop continues; a counting job after it still runs for the next context.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_failing_job_does_not_abort_loop() {
        let (_sa, ca) = ctx("slug-a").await;
        let (_sb, cb) = ctx("slug-b").await;

        struct FailingJob;
        #[async_trait]
        impl BackgroundJob for FailingJob {
            fn name(&self) -> &str {
                "failing"
            }
            fn cadence(&self) -> Cadence {
                Cadence::EveryTick
            }
            fn resource_class(&self) -> ResourceClass {
                ResourceClass::Io
            }
            async fn run(
                &self,
                _ctx: &PerSlugTickContext,
                _shared: &SharedTickResources,
            ) -> Result<(), String> {
                Err("intentional failure".to_string())
            }
        }

        let count = Arc::new(AtomicU32::new(0));
        let registry: Vec<Box<dyn BackgroundJob>> = vec![
            Box::new(FailingJob),
            Box::new(CountingJob {
                name: "after".to_string(),
                cadence: Cadence::EveryTick,
                count: Arc::clone(&count),
            }),
        ];
        let shared = test_shared();
        // Both contexts: failing job errors (logged), counting job still runs ⇒ 2.
        run_per_slug_tick_pass(&[ca, cb], &registry, &shared).await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            2,
            "a job failure must not abort the loop or other jobs"
        );
    }

    /// Build a minimal SharedTickResources for loop tests (no real model load).
    pub(super) fn test_shared() -> SharedTickResources {
        use crate::infra::categories::CategoryAllowlist;
        use crate::infra::config::{InferenceConfig, RetentionConfig};
        use crate::infra::embed_handle::EmbedServiceHandle;
        use crate::infra::nli_handle::NliServiceHandle;
        use crate::infra::rayon_pool::RayonPool;
        use unimatrix_engine::confidence::ConfidenceParams;

        SharedTickResources {
            embed_service: EmbedServiceHandle::new(),
            nli_handle: NliServiceHandle::new(),
            inference_config: Arc::new(InferenceConfig::default()),
            confidence_params: Arc::new(ConfidenceParams::default()),
            rayon_pool: Arc::new(RayonPool::new(1, "test_tick_loop").expect("test rayon pool")),
            category_allowlist: Arc::new(CategoryAllowlist::new()),
            retention_config: Arc::new(RetentionConfig::default()),
            auto_quarantine_cycles: 3,
            tick_interval_secs: 900,
        }
    }
}
