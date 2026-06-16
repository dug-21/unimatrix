//! crt-055 Component 9 — review-pipeline aggregate orchestration.
//!
//! The `context_cycle_review` handler (`tools.rs`) is a long, multi-stage
//! pipeline whose source data (the transcript fold, `cycle_events`, session
//! records, observation records, compaction boundaries) becomes available at
//! DIFFERENT points. This module carries a single [`ReviewAggregateState`] that
//! the handler populates incrementally — in the binding pipeline order — and
//! finalizes into one [`CycleReviewRecord`]-ready [`CycleAggregates`] plus the
//! rendered fail-loud presentation block.
//!
//! ## Binding pipeline order (brief §"Review Pipeline Order", ADRs 002/006/007/010)
//!
//! 1. `auto_close` writes `cycle_stop` BEFORE rank-1 reads the timeline (handled
//!    in `tools.rs` via `maybe_auto_close`).
//! 2. [`ReviewAggregateState::land_fold`] reads the transcript fold STRICTLY
//!    BEFORE `purge_cycle_transcripts` (read-before-purge, ADR-007 / R-03).
//! 3. [`ReviewAggregateState::populate_ranks`] — rank-1/2/3 reckoning.
//! 4. [`ReviewAggregateState::populate_reload`] +
//!    [`ReviewAggregateState::populate_compaction`] — dual reload, two windows.
//! 5. [`ReviewAggregateState::availability`] — per-metric presence flags.
//! 6. The handler builds the `CycleReviewRecord` from
//!    [`ReviewAggregateState::aggregates`] and writes it via the SINGLE
//!    full-pipeline `store_cycle_review()` (ADR-002 / no zero-clobber).
//!
//! Every value is `i64`/`String` and content-free (integers + a class-count map),
//! so the structural leak gate holds (NFR-01 / AC-19).

use std::collections::HashMap;

use unimatrix_observe::{
    CycleAggregates, CycleContext, CycleEventRecord, MetricAvailability, ObservationRecord,
    SessionSummary, compute_availability, populate_rank_1_2_3, reckon_compaction_reread,
    reckon_context_reload_bps, render_metric, render_metrics_block, render_ratio,
};
use unimatrix_store::{SessionRecord, SqlxStore};

use crate::infra::session::SessionRegistry;
use crate::mcp::activity_fold_handler::land_activity_fold;

/// Incrementally-populated review aggregates + source-presence context.
///
/// Defaulted at the top of the full-pipeline block; each `populate_*` method
/// fills the fields whose source has just been loaded. After all stages run,
/// [`Self::aggregates`] yields the value bundle the single writer persists, and
/// [`Self::render_block`] yields the fail-loud presentation text.
#[derive(Debug, Default)]
pub(crate) struct ReviewAggregateState {
    agg: CycleAggregates,
    ctx: CycleContext,
}

impl ReviewAggregateState {
    /// Fresh, all-zero/absent state (an empty cycle renders every metric
    /// "unavailable", never a fabricated `0`).
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// STEP 2 — read-before-purge transcript-fold landing (ADR-007 / R-03).
    ///
    /// MUST be called STRICTLY BEFORE `purge_cycle_transcripts`. Reads the
    /// per-session `ActivitySnapshot` fold for the cycle, sums across held
    /// sessions, width-converts to `i64`, and lands the transcript columns +
    /// `signal_class_counts_json`. `transcript_fold_available` is set from the
    /// landing's `available` flag (≥1 declared session produced a fold).
    pub(crate) fn land_fold(
        &mut self,
        registry: &SessionRegistry,
        feature_cycle: &str,
        class_names: &[String],
    ) {
        let landing = land_activity_fold(registry, feature_cycle, class_names);
        self.agg.transcript_bytes_total = landing.transcript_bytes_total;
        self.agg.transcript_delta_count = landing.transcript_delta_count;
        self.agg.transcript_error_count = landing.transcript_error_count;
        self.agg.transcript_refusal_count = landing.transcript_refusal_count;
        self.agg.signal_class_counts_json = landing.signal_class_counts_json;
        self.ctx.any_declared_fold = landing.available;
    }

    /// STEP 3 (rank-2/3) — rework ratio + knowledge-reuse union.
    ///
    /// rank-2 (`SessionRecord.outcome` rework ratio num/den), rank-3 (`query_log
    /// ∪ injection_log` served union, #320). Called while `session_records` and
    /// the served-knowledge logs are in scope, BEFORE rank-1 (which needs the
    /// `cycle_events` timeline read later in the handler). rank-1 lands via
    /// [`Self::populate_rank_1`].
    pub(crate) fn populate_ranks_2_3(
        &mut self,
        sessions: &[SessionRecord],
        query_logs: &[unimatrix_store::QueryLogRecord],
        injection_logs: &[unimatrix_store::InjectionLogRecord],
    ) {
        // populate_rank_1_2_3 with an EMPTY events slice fills rank-2/3 and leaves
        // rank-1 at zero; rank-1 is then overwritten by populate_rank_1 from the
        // real timeline. (Keeps the established observe reckoner as the one path.)
        populate_rank_1_2_3(&mut self.agg, &[], sessions, query_logs, injection_logs);
        self.ctx.knowledge_log_nonempty = !query_logs.is_empty() || !injection_logs.is_empty();
        // Knowledge-reuse ratio denominator: the cycle's session candidates.
        // Carried as a num/den PAIR so `0 of 0` (unavailable) stays distinguishable
        // from `0 of N` (R-17).
        self.ctx.reuse_denominator = self.agg.total_session_count.max(0);
    }

    /// STEP 3 (rank-1) — phase reckoning from the `cycle_events` timeline.
    ///
    /// `events` MUST already include any `auto_close` `cycle_stop` (STEP 1 ran
    /// first, R-14) so a closed final phase is not a false #556 never-closed.
    /// Sets `cycle_events_count` for the phase-metrics availability gate.
    pub(crate) fn populate_rank_1(&mut self, events: &[CycleEventRecord]) {
        let phase = unimatrix_observe::reckon_phase_aggregates(events);
        self.agg.phase_count = phase.phase_count;
        self.agg.phase_transition_count = phase.phase_transition_count;
        self.agg.phase_rework_count = phase.phase_rework_count;
        self.agg.phase_unclosed_count = phase.phase_unclosed_count;
        self.agg.phase_total_duration_secs = phase.phase_total_duration_secs;
        self.ctx.cycle_events_count = events.len() as i64;
    }

    /// STEP 4a — cross-session `context_reload_pct` (basis points 0–10000).
    ///
    /// `round(fraction × 10000)`, clamped — INTEGER, no float bound to the column
    /// (ADR-005, #4529/#4533 designed out). `session_count` drives the
    /// cross-session-window availability gate (≥2 sessions).
    pub(crate) fn populate_reload(
        &mut self,
        summaries: &[SessionSummary],
        records: &[ObservationRecord],
        session_count: i64,
    ) {
        self.agg.context_reload_pct = reckon_context_reload_bps(summaries, records);
        self.ctx.session_count = session_count;
    }

    /// STEP 4b — within-cycle `compaction_reread_count` + `compaction_count`.
    ///
    /// Over the cycle's DECLARED session ids (never undeclared/evicted — #4140 /
    /// R-05): `compaction_count` is the store `COUNT`; `compaction_reread_count`
    /// drives the shared overlap primitive per session with that session's
    /// `MIN(compacted_at)` boundary (seconds), gating reads at
    /// `(read_ts_millis ÷ 1000) > compacted_at` (ADR-006). Any store read error
    /// degrades the metric to a zero/absent partial (honest, never aborts).
    pub(crate) async fn populate_compaction(
        &mut self,
        store: &SqlxStore,
        declared_session_ids: &[String],
        records: &[ObservationRecord],
    ) {
        self.agg.compaction_count = store
            .compaction_count_for_sessions(declared_session_ids)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("crt-055: compaction_count read failed: {e} — treating as 0");
                0
            });

        // Resolve each declared session's earliest boundary; skip sessions with
        // no compaction rows (None) so they contribute no gate (not a zero gate).
        let mut boundaries: HashMap<String, i64> = HashMap::new();
        for sid in declared_session_ids {
            match store.min_compacted_at(sid).await {
                Ok(Some(min_secs)) => {
                    boundaries.insert(sid.clone(), min_secs);
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(
                        "crt-055: min_compacted_at read failed for {sid}: {e} — \
                         session contributes no compaction_reread"
                    );
                }
            }
        }
        self.ctx.any_compaction_boundary = !boundaries.is_empty();
        self.agg.compaction_reread_count = reckon_compaction_reread(records, &boundaries);
    }

    /// STEP 5 — per-metric source-presence flags (drives the fail-loud guard).
    pub(crate) fn availability(&self) -> MetricAvailability {
        compute_availability(&self.agg, &self.ctx)
    }

    /// The populated value bundle the single full-pipeline writer persists.
    pub(crate) fn aggregates(&self) -> &CycleAggregates {
        &self.agg
    }

    /// Render the fail-loud presentation block for the cycle (STEP 5 honesty).
    ///
    /// Exactly-counted aggregates render bare when available; behavioral signals
    /// carry the coarse/directional qualifier; ratios drive off num/den pairs;
    /// reload renders from basis points. Empty sources render "unavailable",
    /// never a bare `0` (AC-01 / AC-21). Appended to the handler's response.
    pub(crate) fn render_block(&self, avail: &MetricAvailability) -> String {
        let mut block = String::from("Cycle metrics (crt-055):\n");
        block.push_str(&render_metrics_block(&self.agg, avail));
        block.push('\n');
        // Rank-2 rework ratio from the num/den PAIR (R-17).
        block.push_str(&render_ratio(
            "Rework rate",
            self.agg.rework_session_count,
            self.agg.total_session_count,
            avail.rework_ratio_available,
        ));
        block.push('\n');
        // Rank-3 knowledge reuse from the served-count / denominator PAIR.
        block.push_str(&render_ratio(
            "Knowledge reuse",
            self.agg.knowledge_reuse_served_count,
            self.ctx.reuse_denominator,
            avail.knowledge_reuse_available,
        ));
        block.push('\n');
        // context_reload from basis points (3750 → "37.50%"); single-session →
        // "unavailable", never "0%".
        block.push_str(&unimatrix_observe::render_context_reload(
            self.agg.context_reload_pct,
            avail.context_reload_available,
        ));
        block.push('\n');
        // Transcript-fold throughput: bytes/deltas are exact (bare); errors/
        // refusals already render coarse/directional inside render_metrics_block.
        block.push_str(&render_metric(
            "Transcript bytes",
            self.agg.transcript_bytes_total,
            avail.transcript_fold_available,
            false,
        ));
        block.push('\n');
        block.push_str(&render_metric(
            "Transcript deltas",
            self.agg.transcript_delta_count,
            avail.transcript_fold_available,
            false,
        ));
        block
    }
}

impl ReviewAggregateState {
    /// Test-only: directly set the transcript behavioral-signal counts + mark the
    /// fold available, without a live registry. Mirrors what `land_fold` lands for
    /// a non-empty held fold (exercises the coarse/directional render path).
    #[cfg(test)]
    pub(crate) fn land_fold_for_test(&mut self, error_count: i64, refusal_count: i64) {
        self.agg.transcript_error_count = error_count;
        self.agg.transcript_refusal_count = refusal_count;
        self.ctx.any_declared_fold = true;
    }
}

#[cfg(test)]
#[path = "review_aggregates_tests.rs"]
mod tests;
