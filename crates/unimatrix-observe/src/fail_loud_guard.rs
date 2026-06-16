//! Fail-loud presentation guard — per-metric availability + coarse/directional honesty.
//!
//! crt-055 Component 7 (Wave 1, sequenced FIRST to de-risk the believable-zero class
//! before any durable column lands). Presentation-only: nothing here is persisted on
//! `CycleReviewRecord`, none of it gates the single writer, and none of it changes which
//! source a metric reads (ADR-003 #5046).
//!
//! Two independent honesty axes:
//!   1. **available vs unavailable** — a metric whose source class is empty renders the
//!      literal string `"unavailable"` (with a terse reason), NEVER a bare `0`. An honest
//!      computation must not emit a dishonest `0` indistinguishable from a measured zero
//!      (lesson #4998 / #750 believable-zero class).
//!   2. **exact vs coarse/directional** — behavioral signals (`transcript_error_count`,
//!      `transcript_refusal_count`, `signal_class_counts_json`) derive from unvalidated,
//!      content-opaque regex matches; they CANNOT be audited post-hoc. They ALWAYS render
//!      with a coarse/directional qualifier (`~` / "directional"), visibly distinct from
//!      exactly-counted aggregates (phase counts, session ratios, compaction count), which
//!      render bare. This is a constant rule, not a flag (Constraint 6 / ADR-003).
//!
//! Per-metric, NOT one cycle-wide flag: the aggregates draw from DIFFERENT source classes
//! (cycle_events, SessionRecord.outcome, query_log ∪ injection_log, the activity fold,
//! compaction_events), so presence is tracked per metric. Each flag is INDEPENDENT — one
//! empty source never flips another's flag, one present source never masks another's
//! emptiness (R-06).

use serde::{Deserialize, Serialize};

/// Marker shown next to behavioral-signal counts, indicating the value is directional.
pub const DIRECTIONAL_TILDE: &str = "~";

/// Literal rendered for any metric whose source class is empty. NEVER a bare `0`.
pub const UNAVAILABLE: &str = "unavailable";

/// Per-metric source-presence, carried on the `RetrospectiveReport` presentation layer.
///
/// This is NOT a `CycleReviewRecord` DB column — it has no schema impact and adds no leak
/// surface. It is computed at render time from the report's own counts/flags. Each flag
/// reflects whether *that metric's* source class is non-empty for the cycle.
///
/// Behavioral signals (`transcript_error_count` / `transcript_refusal_count` /
/// `signal_class_counts_json`) are intentionally absent here: they are ALWAYS
/// coarse/directional by construction (a constant presentation rule), and their
/// availability rides `transcript_fold_available` — the coarse marking is orthogonal to
/// the available flag (a directional count can still be unavailable when its fold is
/// missing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetricAvailability {
    /// `cycle_events` non-empty for the cycle (drives phase_* metrics).
    pub phase_metrics_available: bool,
    /// `total_session_count > 0` — a `SessionRecord` denominator exists (rework ratio).
    pub rework_ratio_available: bool,
    /// `query_log ∪ injection_log` had ≥1 served row (knowledge reuse).
    pub knowledge_reuse_available: bool,
    /// ≥1 declared session produced a transcript fold (Component 6 landing).
    pub transcript_fold_available: bool,
    /// ≥1 attributed `compaction_events` row (Component 5).
    pub compaction_available: bool,
    /// ≥2 sessions in the cycle — a cross-session reload window exists.
    pub context_reload_available: bool,
}

/// Presentation-layer view of the per-cycle aggregate counts the formatter branches on.
///
/// Mirrors the forthcoming v5 `cycle_review_index` columns but is NOT a DB row — it is the
/// counts already gathered in the review pipeline, handed to the formatter. Every count is
/// `i64` to match the column widths (Constraint 10). Ratios are carried as num/den PAIRS,
/// never pre-divided, so `0 of 0` (unavailable) stays distinguishable from `0 of N`
/// (measured 0%) (R-17).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleAggregates {
    // ── exactly-counted aggregates (rank-1, from cycle_events) ──
    pub phase_count: i64,
    pub phase_transition_count: i64,
    pub phase_rework_count: i64,
    /// #556 declared-but-never-closed phases.
    pub phase_unclosed_count: i64,
    /// Σ of closed-phase window durations in seconds (rank-1). Unclosed phases add 0.
    pub phase_total_duration_secs: i64,

    // ── rank-2 rework ratio num/den (SessionRecord.outcome) ──
    pub rework_session_count: i64,
    pub total_session_count: i64,

    // ── rank-3 knowledge reuse (query_log ∪ injection_log, #320) ──
    pub knowledge_reuse_served_count: i64,

    // ── compaction (exactly-counted) ──
    pub compaction_count: i64,
    pub compaction_reread_count: i64,

    // ── reload (basis points 0–10000, rendered as percent) ──
    pub context_reload_pct: i64,

    // ── behavioral signals (ALWAYS coarse/directional) ──
    pub transcript_error_count: i64,
    pub transcript_refusal_count: i64,

    // ── transcript fold throughput (exactly-counted bytes/deltas) + class map ──
    /// Σ `ActivitySnapshot.bytes_total` across the cycle's held sessions (fold).
    pub transcript_bytes_total: i64,
    /// Σ `ActivitySnapshot.delta_count` across the cycle's held sessions (fold).
    pub transcript_delta_count: i64,
    /// Full `class_name → count` map as JSON (fold). Default `"{}"` (empty catalog).
    /// Carried so the single writer (Component 2) lands all 16 columns 1:1 from one
    /// struct. Content-free — a count map, never transcript text (NFR-01 leak gate).
    pub signal_class_counts_json: String,
}

/// Presentation-layer view of cycle-context flags/counts NOT carried in `CycleAggregates`.
///
/// These describe source presence directly (source-class non-emptiness) rather than a
/// computed aggregate, and feed `compute_availability`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CycleContext {
    /// Number of `cycle_events` rows for the cycle.
    pub cycle_events_count: i64,
    /// `query_log ∪ injection_log` had ≥1 served row.
    pub knowledge_log_nonempty: bool,
    /// ≥1 declared session produced a fold (Component 6 `landing.available`).
    pub any_declared_fold: bool,
    /// ≥1 attributed compaction boundary exists (independent of the counted rows).
    pub any_compaction_boundary: bool,
    /// Number of declared sessions in the cycle (cross-session window needs ≥2).
    pub session_count: i64,
    /// Denominator for the knowledge-reuse ratio (query ∪ injection candidates).
    pub reuse_denominator: i64,
}

/// Derive per-metric availability from the cycle's aggregates and context.
///
/// Each flag is INDEPENDENT (R-06): one empty source does not flip another's flag, and one
/// present source does not mask another's emptiness.
pub fn compute_availability(agg: &CycleAggregates, ctx: &CycleContext) -> MetricAvailability {
    MetricAvailability {
        phase_metrics_available: ctx.cycle_events_count > 0,
        rework_ratio_available: agg.total_session_count > 0,
        knowledge_reuse_available: ctx.knowledge_log_nonempty,
        transcript_fold_available: ctx.any_declared_fold,
        compaction_available: agg.compaction_count > 0 || ctx.any_compaction_boundary,
        context_reload_available: ctx.session_count >= 2,
    }
}

/// Terse, actionable reason shown alongside an `unavailable` metric. Precise enough to be
/// actionable, never alarming (ADR-003 consequences).
fn terse_reason(label: &str) -> &'static str {
    match label {
        "Phases" | "Phase transitions" | "Rework loops" | "Never-closed phases" => {
            "no cycle_events tracked"
        }
        "Compactions" | "Compaction re-reads" => "no compaction recorded",
        "Errors (signal)" | "Refusals (signal)" => "no transcript fold",
        "Rework rate" => "no sessions recorded",
        "Knowledge reuse" => "no served knowledge",
        "Context reload" => "single-session cycle",
        _ => "no data",
    }
}

/// Render one exactly-counted-or-directional metric.
///
/// - not available → `"{label}: unavailable ({reason})"` — NEVER a bare `0`.
/// - available + coarse → `"{label}: ~{value} (directional)"` — the coarse/directional
///   qualifier marks a regex-derived behavioral signal (ADR-003 binding clause).
/// - available + exact → `"{label}: {value}"` — bare, auditable.
pub fn render_metric(label: &str, value: i64, available: bool, coarse: bool) -> String {
    if !available {
        return format!("{label}: {UNAVAILABLE} ({})", terse_reason(label));
    }
    if coarse {
        return format!("{label}: {DIRECTIONAL_TILDE}{value} (directional)");
    }
    format!("{label}: {value}")
}

/// Render a ratio from a num/den PAIR (R-17).
///
/// `0 of 0` (empty denominator OR the metric's source unavailable) → `"unavailable"`; a
/// genuine `0 of N` → a measured rate. This is how a measured zero stays distinguishable
/// from "never observed". The percentage is derived here at presentation time from the
/// stored pair — never a pre-divided single number.
pub fn render_ratio(label: &str, num: i64, den: i64, available: bool) -> String {
    if !available || den <= 0 {
        return format!("{label}: {UNAVAILABLE} ({})", terse_reason(label));
    }
    // Integer-domain percentage, rounded to nearest — no float column is involved.
    let pct = (num.saturating_mul(100) + den / 2) / den;
    format!("{label}: {num} of {den} ({pct}%)")
}

/// Render the `context_reload` metric from stored basis points (0–10000).
///
/// `3750` bps → `"37.5%"`. A single-session cycle (no cross-session window) →
/// `"unavailable"`, NEVER a fabricated `"0%"`.
pub fn render_context_reload(bps: i64, available: bool) -> String {
    if !available {
        return format!(
            "Context reload: {UNAVAILABLE} ({})",
            terse_reason("Context reload")
        );
    }
    let clamped = bps.clamp(0, 10_000);
    let whole = clamped / 100;
    let frac = clamped % 100;
    format!("Context reload: {whole}.{frac:02}%")
}

/// Render the full fail-loud metrics block for a cycle.
///
/// Exactly-counted aggregates render bare when available; behavioral signals always carry
/// the directional qualifier; ratios drive off num/den pairs; reload renders from bps.
pub fn render_metrics_block(agg: &CycleAggregates, avail: &MetricAvailability) -> String {
    let lines: Vec<String> = vec![
        // Exactly-counted aggregates (coarse = false → bare when available).
        render_metric(
            "Phases",
            agg.phase_count,
            avail.phase_metrics_available,
            false,
        ),
        render_metric(
            "Phase transitions",
            agg.phase_transition_count,
            avail.phase_metrics_available,
            false,
        ),
        render_metric(
            "Rework loops",
            agg.phase_rework_count,
            avail.phase_metrics_available,
            false,
        ),
        render_metric(
            "Never-closed phases",
            agg.phase_unclosed_count,
            avail.phase_metrics_available,
            false,
        ),
        render_metric(
            "Compactions",
            agg.compaction_count,
            avail.compaction_available,
            false,
        ),
        render_metric(
            "Compaction re-reads",
            agg.compaction_reread_count,
            avail.compaction_available,
            false,
        ),
        // Behavioral signals — ALWAYS coarse (coarse = true), still subject to availability.
        render_metric(
            "Errors (signal)",
            agg.transcript_error_count,
            avail.transcript_fold_available,
            true,
        ),
        render_metric(
            "Refusals (signal)",
            agg.transcript_refusal_count,
            avail.transcript_fold_available,
            true,
        ),
    ];

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx_all_present() -> CycleContext {
        CycleContext {
            cycle_events_count: 4,
            knowledge_log_nonempty: true,
            any_declared_fold: true,
            any_compaction_boundary: true,
            session_count: 3,
            reuse_denominator: 10,
        }
    }

    fn agg_all_present() -> CycleAggregates {
        CycleAggregates {
            phase_count: 3,
            phase_transition_count: 2,
            phase_rework_count: 1,
            phase_unclosed_count: 1,
            phase_total_duration_secs: 600,
            rework_session_count: 1,
            total_session_count: 4,
            knowledge_reuse_served_count: 2,
            compaction_count: 2,
            compaction_reread_count: 0,
            context_reload_pct: 3750,
            transcript_error_count: 3,
            transcript_refusal_count: 1,
            transcript_bytes_total: 4096,
            transcript_delta_count: 12,
            signal_class_counts_json: "{\"error\":3,\"refusal\":1}".to_string(),
        }
    }

    // ── Per-metric "unavailable" not "0" (R-06, AC-01) ──────────────────────

    #[test]
    fn test_empty_source_renders_unavailable_per_metric() {
        // A fully empty cycle: zero cycle_events, zero compaction, empty fold, no served
        // knowledge, single session. Each metric must render the literal "unavailable",
        // NEVER the literal "0".
        let agg = CycleAggregates::default();
        let ctx = CycleContext::default();
        let avail = compute_availability(&agg, &ctx);

        let block = render_metrics_block(&agg, &avail);

        // Phase metrics: zero cycle_events → unavailable.
        assert!(
            block.contains("Phases: unavailable"),
            "phase metrics must be unavailable: {block}"
        );
        // Compaction: zero compaction_events → unavailable.
        assert!(
            block.contains("Compactions: unavailable"),
            "compaction must be unavailable: {block}"
        );
        assert!(
            block.contains("Compaction re-reads: unavailable"),
            "reread must be unavailable: {block}"
        );
        // Transcript fold empty → behavioral signals unavailable.
        assert!(
            block.contains("Errors (signal): unavailable"),
            "errors must be unavailable: {block}"
        );
        assert!(
            block.contains("Refusals (signal): unavailable"),
            "refusals must be unavailable: {block}"
        );

        // NEVER a bare "0" anywhere in the rendered block for an empty cycle.
        for line in block.lines() {
            assert!(
                !line.trim_end().ends_with(": 0"),
                "no metric may render a bare 0 for an empty cycle: {line:?}"
            );
        }

        // Ratio + reload paths (rendered separately by the pipeline) also fail loud.
        let reuse = render_ratio(
            "Knowledge reuse",
            agg.knowledge_reuse_served_count,
            ctx.reuse_denominator,
            avail.knowledge_reuse_available,
        );
        assert!(reuse.contains("unavailable"), "reuse: {reuse}");
        assert!(!reuse.contains(": 0"), "reuse must not bare-zero: {reuse}");

        let reload = render_context_reload(agg.context_reload_pct, avail.context_reload_available);
        assert!(reload.contains("unavailable"), "reload: {reload}");
        assert!(
            !reload.contains("0%"),
            "reload must not fabricate 0%: {reload}"
        );
    }

    #[test]
    fn test_per_metric_flags_independent() {
        // Present compaction + reload, but everything else empty. One present source must
        // NOT mask another's emptiness, and one empty source must NOT flip a present flag.
        let agg = CycleAggregates {
            compaction_count: 5,
            ..Default::default()
        };
        let ctx = CycleContext {
            any_compaction_boundary: true,
            session_count: 2,
            ..Default::default()
        };
        let avail = compute_availability(&agg, &ctx);

        // The two present sources stay available...
        assert!(avail.compaction_available, "compaction present");
        assert!(avail.context_reload_available, "reload window present");
        // ...while every other flag stays independently false.
        assert!(!avail.phase_metrics_available);
        assert!(!avail.rework_ratio_available);
        assert!(!avail.knowledge_reuse_available);
        assert!(!avail.transcript_fold_available);
    }

    #[test]
    fn test_measured_zero_distinct_from_unavailable() {
        // A genuine measured zero: compaction happened (count > 0) but no re-reads (== 0).
        // The reread must render a bare measured "0", NOT "unavailable".
        let agg = CycleAggregates {
            compaction_count: 3,
            compaction_reread_count: 0,
            ..Default::default()
        };
        let ctx = CycleContext {
            any_compaction_boundary: true,
            ..Default::default()
        };
        let avail = compute_availability(&agg, &ctx);

        let reread = render_metric(
            "Compaction re-reads",
            agg.compaction_reread_count,
            avail.compaction_available,
            false,
        );
        assert_eq!(reread, "Compaction re-reads: 0");
        assert!(
            !reread.contains("unavailable"),
            "measured 0 is not unavailable"
        );
    }

    // ── Ratio honesty (R-17, AC-01) ─────────────────────────────────────────

    #[test]
    fn test_ratio_zero_of_zero_unavailable() {
        // 0 of 0 (empty denominator) → unavailable, never a fabricated rate.
        let out = render_ratio("Rework rate", 0, 0, false);
        assert!(
            out.contains("unavailable"),
            "0 of 0 must be unavailable: {out}"
        );
        assert!(
            !out.contains(" of "),
            "0 of 0 must not print a ratio: {out}"
        );
    }

    #[test]
    fn test_ratio_zero_of_n_measured() {
        // 0 of N → a genuine measured rate driven off the stored num/den pair.
        let out = render_ratio("Rework rate", 0, 8, true);
        assert!(out.contains("0 of 8"), "must print the pair: {out}");
        assert!(out.contains("0%"), "measured zero rate: {out}");
        assert!(
            !out.contains("unavailable"),
            "0 of N is measured, not unavailable: {out}"
        );
    }

    #[test]
    fn test_ratio_nonzero_rounds_from_pair() {
        // 1 of 3 → 33% rounded from the pair (no pre-divided number).
        let out = render_ratio("Rework rate", 1, 3, true);
        assert!(out.contains("1 of 3"), "{out}");
        assert!(out.contains("33%"), "rounded percent from pair: {out}");
    }

    // ── Coarse-signal presentation honesty (R-06, AC-21) ────────────────────

    #[test]
    fn test_behavioral_signals_carry_directional_qualifier() {
        // transcript_error_count / refusal_count render WITH the coarse/directional
        // qualifier — a non-zero value reads as a directional signal, never an exact tally.
        let agg = agg_all_present();
        let avail = compute_availability(&agg, &ctx_all_present());

        let errors = render_metric(
            "Errors (signal)",
            agg.transcript_error_count,
            avail.transcript_fold_available,
            true,
        );
        let refusals = render_metric(
            "Refusals (signal)",
            agg.transcript_refusal_count,
            avail.transcript_fold_available,
            true,
        );

        assert!(errors.contains('~'), "errors carry tilde: {errors}");
        assert!(
            errors.contains("directional"),
            "errors carry directional label: {errors}"
        );
        assert!(refusals.contains('~'), "refusals carry tilde: {refusals}");
        assert!(
            refusals.contains("directional"),
            "refusals carry directional label: {refusals}"
        );
        // The raw count is still present, just qualified.
        assert!(errors.contains("~3"), "directional signal of 3: {errors}");
    }

    #[test]
    fn test_exact_aggregates_do_not_carry_qualifier() {
        // Exactly-counted aggregates (compaction_count, phase counts) render bare — no
        // qualifier — and are DISTINGUISHABLE from the directional behavioral signals.
        let agg = agg_all_present();
        let avail = compute_availability(&agg, &ctx_all_present());

        let compactions = render_metric(
            "Compactions",
            agg.compaction_count,
            avail.compaction_available,
            false,
        );
        let phases = render_metric(
            "Phases",
            agg.phase_count,
            avail.phase_metrics_available,
            false,
        );

        assert_eq!(compactions, "Compactions: 2");
        assert_eq!(phases, "Phases: 3");
        assert!(
            !compactions.contains('~'),
            "exact aggregate has no tilde: {compactions}"
        );
        assert!(
            !compactions.contains("directional"),
            "exact aggregate not directional: {compactions}"
        );

        // The honesty boundary: directional vs exact must be distinguishable.
        let errors = render_metric(
            "Errors (signal)",
            agg.transcript_error_count,
            avail.transcript_fold_available,
            true,
        );
        assert_ne!(
            errors.contains("directional"),
            compactions.contains("directional"),
            "directional and exact presentations must be distinguishable"
        );
    }

    #[test]
    fn test_behavioral_signal_zero_still_directional() {
        // A behavioral signal of exactly 0 (with a present fold) still renders directional —
        // a "directional no-signal", not an exact "0".
        let agg = CycleAggregates {
            transcript_error_count: 0,
            ..Default::default()
        };
        let avail = MetricAvailability {
            phase_metrics_available: false,
            rework_ratio_available: false,
            knowledge_reuse_available: false,
            transcript_fold_available: true,
            compaction_available: false,
            context_reload_available: false,
        };
        let out = render_metric(
            "Errors (signal)",
            agg.transcript_error_count,
            avail.transcript_fold_available,
            true,
        );
        assert!(out.contains('~'), "zero signal still directional: {out}");
        assert!(out.contains("directional"), "{out}");
        assert!(!out.ends_with(": 0"), "must not be a bare exact 0: {out}");
    }

    // ── context_reload basis-points rendering ───────────────────────────────

    #[test]
    fn test_context_reload_bps_renders_percent() {
        // 3750 bps → 37.50%.
        let out = render_context_reload(3750, true);
        assert!(out.contains("37.50%"), "bps → percent: {out}");
    }

    #[test]
    fn test_context_reload_single_session_unavailable() {
        // Single-session cycle (no cross-session window) → unavailable, NOT "0%".
        let out = render_context_reload(0, false);
        assert!(
            out.contains("unavailable"),
            "single-session reload unavailable: {out}"
        );
        assert!(!out.contains("0%"), "must not fabricate 0%: {out}");
    }

    #[test]
    fn test_context_reload_clamps_basis_points() {
        // Out-of-range bps clamp to 0–10000 before rendering (Constraint 10 width safety).
        let high = render_context_reload(15_000, true);
        assert!(high.contains("100.00%"), "clamp high: {high}");
        let low = render_context_reload(-5, true);
        assert!(low.contains("0.00%"), "clamp low: {low}");
    }

    // ── Edge cases ──────────────────────────────────────────────────────────

    #[test]
    fn test_zero_declared_sessions_all_unavailable() {
        // A cycle with ZERO declared sessions → every source-derived metric unavailable.
        let agg = CycleAggregates::default();
        let ctx = CycleContext::default();
        let avail = compute_availability(&agg, &ctx);

        assert!(!avail.phase_metrics_available);
        assert!(!avail.rework_ratio_available);
        assert!(!avail.knowledge_reuse_available);
        assert!(!avail.transcript_fold_available);
        assert!(!avail.compaction_available);
        assert!(!avail.context_reload_available);
    }

    #[test]
    fn test_mixed_present_and_empty_sources() {
        // Phases present, compaction absent → per-metric granularity (not cycle-wide).
        let agg = CycleAggregates {
            phase_count: 2,
            ..Default::default()
        };
        let ctx = CycleContext {
            cycle_events_count: 5,
            ..Default::default()
        };
        let avail = compute_availability(&agg, &ctx);
        let block = render_metrics_block(&agg, &avail);

        assert!(block.contains("Phases: 2"), "phases measured: {block}");
        assert!(
            block.contains("Compactions: unavailable"),
            "compaction still unavailable: {block}"
        );
    }
}
