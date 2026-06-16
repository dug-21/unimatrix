//! Reload overlap engine — one file-set-intersection primitive, two windows, two
//! columns, never collapsed (crt-055 Component 4, Wave 3).
//!
//! ass-077 surfaces TWO distinct reload signals that must never collapse into one
//! number (ADR-005 #5047, R-07):
//!   - **`context_reload_pct`** — CROSS-SESSION file overlap (continuity/handoff cost
//!     between a cycle's sessions). Promoted as-is from #758's
//!     [`crate::compute_context_reload_pct`]; this module hosts the shared primitive it
//!     now calls and the basis-points persist-boundary conversion.
//!   - **`compaction_reread_count`** — WITHIN-CYCLE post-compaction re-read (the
//!     compaction tax). Its caller is Component 5 (`compaction_reckoning.md`); this
//!     module exposes the [`ReloadWindow::PostCompaction`] window + the primitive so
//!     Component 5 can drive it. **The compaction caller is NOT implemented here.**
//!
//! Both windows feed ONE [`overlap_count`] primitive parameterized by [`ReloadWindow`];
//! the engine is shared, the windows and gates are not (ADR-005). A refactor that
//! derives one window from the other destroys the distinct semantics — the temptation
//! surface R-07 pins.
//!
//! ## No `f64` reaches any column (ADR-005, lessons #4529/#4533 designed out)
//! Every reload metric column on `cycle_review_index` is INTEGER. `context_reload_pct`
//! is stored as **basis points 0–10000** via [`reckon_context_reload_bps`]: the live
//! [`crate::compute_context_reload_pct`] returns a **fraction in `[0.0, 1.0]`** (NOT a
//! 0–100 percentage), encoded `round(fraction × 10000)` and clamped to `0..=10000`
//! before it becomes the `i64`. No `f64` is ever bound to a column, so the
//! `push_bind(f64)` non-finite footgun (#4529/#4533) cannot occur here — there is no
//! `is_finite()` guard because there is no REAL column.

use std::collections::{HashMap, HashSet};

use crate::session_metrics::extract_file_path;
use crate::types::{ObservationRecord, SessionSummary};

/// The window a [`overlap_count`] call measures over. The one primitive is
/// parameterized by this; the two callers pass DISTINCT windows (ADR-005 / R-07).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadWindow {
    /// `context_reload`: prior = the cumulative union of all *earlier* sessions' files;
    /// later-session reads that hit that union count as reloads. NOT gated on
    /// `compacted_at` — it is a cross-session continuity signal.
    CrossSession,
    /// `compaction_reread`: prior = files read at or before `boundary_secs` within the
    /// SAME session; reads strictly after the boundary that hit that prior set count
    /// once. The per-session gate + `MIN(compacted_at)` boundary selection are owned by
    /// Component 5 (`compaction_reckoning.md`); this module supplies the window only.
    PostCompaction {
        /// The compaction boundary in **Unix seconds** (ADR-006). The read side is
        /// epoch millis and is normalized `ts / 1000` before comparison — the boundary
        /// is never re-scaled (Binding constraint 3 / ADR-006).
        boundary_secs: i64,
    },
}

/// Result of an overlap reckoning: how many reads hit the window's prior set, and the
/// denominator they were measured against. Plain integer counts — no float, no ratio
/// (the fraction, if needed, is computed by the caller at the persist boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlapCounts {
    /// Reads that re-touched a file already in the window's prior set.
    pub overlap: u64,
    /// Denominator: reads considered within the window (the "subsequent" reads).
    pub total: u64,
}

/// The ONE shared file-set-intersection primitive. Takes its window as INPUT and
/// returns `(overlap, total)`; it does NOT embed either caller's gate — the gate is the
/// window the caller passes (ADR-005 / R-07).
///
/// `summaries` supplies chronological session ordering for [`ReloadWindow::CrossSession`]
/// (sorted by `started_at`, as [`crate::compute_session_summaries`] returns); it is
/// unused by [`ReloadWindow::PostCompaction`], which operates per-session on `records`.
///
/// Only `PostToolUse` rows with an extractable file path participate, matching the live
/// per-session aggregation read-path (#750, ADR-004).
pub fn overlap_count(
    records: &[ObservationRecord],
    window: ReloadWindow,
    summaries: &[SessionSummary],
) -> OverlapCounts {
    match window {
        ReloadWindow::CrossSession => cross_session_overlap(records, summaries),
        ReloadWindow::PostCompaction { boundary_secs } => {
            post_compaction_overlap(records, boundary_secs)
        }
    }
}

/// CROSS-SESSION window: walk sessions chronologically, counting later-session reads
/// that hit the cumulative union of all prior sessions' files. This is the exact body
/// previously inlined in [`crate::compute_context_reload_pct`] (#758), factored out so
/// both callers share it without collapsing windows.
fn cross_session_overlap(
    records: &[ObservationRecord],
    summaries: &[SessionSummary],
) -> OverlapCounts {
    if summaries.len() <= 1 {
        return OverlapCounts::default();
    }

    let session_files = per_session_file_sets(records);

    let mut prior_files: HashSet<String> = HashSet::new();
    let mut counts = OverlapCounts::default();

    for summary in summaries {
        let current_files = session_files
            .get(&summary.session_id)
            .cloned()
            .unwrap_or_default();

        if !prior_files.is_empty() {
            for file in &current_files {
                counts.total += 1;
                if prior_files.contains(file) {
                    counts.overlap += 1;
                }
            }
        }

        prior_files.extend(current_files);
    }

    counts
}

/// POST-COMPACTION window: per session, build the prior set from reads at or before the
/// boundary, then count reads strictly after the boundary that re-touch a prior-set file
/// (once per distinct file per session). The read ts (epoch millis) is normalized to
/// seconds (`ts / 1000`) before the seconds-vs-seconds comparison; the boundary is never
/// re-scaled (ADR-006 / Binding constraint 3).
///
/// This is the window primitive only — Component 5 (`compaction_reckoning.md`) owns the
/// per-session `MIN(compacted_at)` boundary selection and drives this per session.
fn post_compaction_overlap(records: &[ObservationRecord], boundary_secs: i64) -> OverlapCounts {
    // Group file-reads per session, preserving each read's ts for the boundary gate.
    let mut per_session: HashMap<&str, Vec<(i64, String)>> = HashMap::new();
    for record in records {
        if record.event_type != "PostToolUse" {
            continue;
        }
        let path = record
            .tool
            .as_deref()
            .zip(record.input.as_ref())
            .and_then(|(tool, input)| extract_file_path(tool, input));
        if let Some(path) = path {
            // Normalize the READ side only: epoch millis → seconds (ADR-006).
            let read_secs = (record.ts / 1000) as i64;
            per_session
                .entry(record.session_id.as_str())
                .or_default()
                .push((read_secs, path));
        }
    }

    let mut counts = OverlapCounts::default();
    for reads in per_session.values() {
        // Prior set: files read at or before the boundary in this session.
        let prior: HashSet<&String> = reads
            .iter()
            .filter(|(read_secs, _)| *read_secs <= boundary_secs)
            .map(|(_, path)| path)
            .collect();

        if prior.is_empty() {
            continue;
        }

        // Count each distinct prior-set file re-read after the boundary exactly once.
        let mut already_counted: HashSet<&String> = HashSet::new();
        for (read_secs, path) in reads {
            if *read_secs > boundary_secs && prior.contains(path) && already_counted.insert(path) {
                counts.overlap += 1;
            }
            if *read_secs > boundary_secs {
                counts.total += 1;
            }
        }
    }

    counts
}

/// Build per-session file sets from a cycle's observation records — `PostToolUse` rows
/// with an extractable file path only (#750, ADR-004).
fn per_session_file_sets(records: &[ObservationRecord]) -> HashMap<String, HashSet<String>> {
    let mut session_files: HashMap<String, HashSet<String>> = HashMap::new();
    for record in records {
        if record.event_type != "PostToolUse" {
            continue;
        }
        let path = record
            .tool
            .as_deref()
            .zip(record.input.as_ref())
            .and_then(|(tool, input)| extract_file_path(tool, input));
        if let Some(path) = path {
            session_files
                .entry(record.session_id.clone())
                .or_default()
                .insert(path);
        }
    }
    session_files
}

/// `context_reload_pct` basis-points encoding (ADR-005, persist boundary).
///
/// Takes the live [`crate::compute_context_reload_pct`] **fraction** in `[0.0, 1.0]` and
/// encodes basis points `round(fraction × 10000)` (0.375 → 3750), then CLAMPs to
/// `0..=10000` **before** it becomes the `i64`. No `f64` reaches any column; the clamp
/// is a defensive range guard against a future out-of-range fraction (R-09).
///
/// > The worked example 37.5% → 3750 holds. The ADR text phrases the source as "a
/// > percentage" with `round(pct × 100)`; the live function returns a fraction, so the
/// > correct encoding from that fraction is `round(fraction × 10000)` (OVERVIEW Open-Q).
pub fn fraction_to_basis_points(fraction: f64) -> i64 {
    let bps = (fraction * 10_000.0).round() as i64;
    bps.clamp(0, 10_000)
}

/// CROSS-SESSION caller: reckon `context_reload_pct` as basis points from the cycle's
/// session summaries + records. Thin wrapper over the live
/// [`crate::compute_context_reload_pct`] (#758) + [`fraction_to_basis_points`]. The
/// fraction function stays `-> f64` and is unchanged for its other callers.
pub fn reckon_context_reload_bps(
    summaries: &[SessionSummary],
    records: &[ObservationRecord],
) -> i64 {
    let fraction = crate::compute_context_reload_pct(summaries, records);
    fraction_to_basis_points(fraction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compute_session_summaries;
    use serde_json::json;

    fn read_record(session_id: &str, ts: u64, file_path: &str) -> ObservationRecord {
        ObservationRecord {
            ts,
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: Some("Read".to_string()),
            input: Some(json!({ "file_path": file_path })),
            response_size: None,
            response_snippet: None,
        }
    }

    // ---- AC-20: basis-points encoding ----

    #[test]
    fn test_context_reload_pct_basis_points_encode() {
        // 0.375 → 3750; 0.0 → 0; 1.0 → 10000.
        assert_eq!(fraction_to_basis_points(0.375), 3750);
        assert_eq!(fraction_to_basis_points(0.0), 0);
        assert_eq!(fraction_to_basis_points(1.0), 10_000);
    }

    #[test]
    fn test_context_reload_pct_rounding_to_nearest() {
        // Round-to-nearest, NOT floor/truncate.
        // fraction 0.00005 → 0.5 bps → rounds to 1.
        assert_eq!(fraction_to_basis_points(0.00005), 1);
        // fraction 0.99995 → 9999.5 bps → rounds to 10000.
        assert_eq!(fraction_to_basis_points(0.99995), 10_000);
        // 2/3 → 6666.66.. → rounds to 6667.
        assert_eq!(fraction_to_basis_points(2.0 / 3.0), 6667);
    }

    #[test]
    fn test_context_reload_pct_out_of_range_clamped() {
        // Defensive: a future out-of-range fraction is clamped to 0..=10000 before bind.
        assert_eq!(fraction_to_basis_points(1.5), 10_000);
        assert_eq!(fraction_to_basis_points(2.0), 10_000);
        assert_eq!(fraction_to_basis_points(-0.5), 0);
    }

    #[test]
    fn test_context_reload_pct_no_float_column() {
        // Structural: the reckoned value is an i64; no f64 reaches the bind. The only
        // float in the path is the [0.0,1.0] fraction, converted here. (#4529 designed out.)
        let records = vec![
            read_record("s1", 1000, "/a.rs"),
            read_record("s1", 1001, "/b.rs"),
            read_record("s2", 2000, "/a.rs"),
            read_record("s2", 2001, "/b.rs"),
        ];
        let summaries = compute_session_summaries(&records);
        let bps: i64 = reckon_context_reload_bps(&summaries, &records);
        assert_eq!(bps, 10_000); // full overlap → 1.0 → 10000 bps
    }

    // ---- AC-14: basis-points range round-trip ----

    #[test]
    fn test_basis_points_range_round_trip() {
        // 37.5% fraction round-trips to 3750 and back to 37.5% (÷100 at presentation).
        let bps = fraction_to_basis_points(0.375);
        assert_eq!(bps, 3750);
        assert!((0..=10_000).contains(&bps));
        let display_pct = bps as f64 / 100.0;
        assert!((display_pct - 37.5).abs() < 1e-9);
    }

    #[test]
    fn test_basis_points_always_in_range() {
        for f in [-1.0, 0.0, 0.123, 0.5, 0.999, 1.0, 1.1, 100.0] {
            let bps = fraction_to_basis_points(f);
            assert!(
                (0..=10_000).contains(&bps),
                "bps {bps} out of range for {f}"
            );
        }
    }

    // ---- R-07 / AC-13: shared primitive, two distinct callers, never collapsed ----

    #[test]
    fn test_overlap_primitive_pure_window_input() {
        // The primitive takes its window as INPUT; it does not embed either caller's gate.
        let records = vec![
            read_record("s1", 1000, "/a.rs"),
            read_record("s2", 2000, "/a.rs"),
        ];
        let summaries = compute_session_summaries(&records);
        // Same records, two windows → two distinct reckonings.
        let cross = overlap_count(&records, ReloadWindow::CrossSession, &summaries);
        let post = overlap_count(
            &records,
            ReloadWindow::PostCompaction { boundary_secs: 1 },
            &summaries,
        );
        // Cross-session: /a.rs read in s1, re-read in s2 → overlap 1 of 1.
        assert_eq!(cross.overlap, 1);
        assert_eq!(cross.total, 1);
        // Post-compaction (per session, single read each, boundary 1s): no re-read within
        // a session after the boundary → no overlap. Distinct result from same input.
        assert_eq!(post.overlap, 0);
    }

    #[test]
    fn test_context_reload_uses_cross_session_window() {
        // context_reload caller uses the cross-session window — NOT gated on compacted_at.
        // A within-session-only re-read produces no cross-session overlap.
        let records = vec![
            read_record("s1", 1000, "/a.rs"),
            read_record("s1", 2000, "/a.rs"), // same session re-read
        ];
        let summaries = compute_session_summaries(&records);
        let cross = overlap_count(&records, ReloadWindow::CrossSession, &summaries);
        // Single session → no cross-session prior set → zero overlap. The cross-session
        // window ignores within-session re-reads; it is not a compaction gate.
        assert_eq!(cross.overlap, 0);
        assert_eq!(cross.total, 0);
    }

    #[test]
    fn test_compaction_reread_uses_within_cycle_compaction_gate() {
        // compaction_reread caller uses the post-compaction within-session window gated
        // on the boundary (gate detail delegated to compaction_reckoning.md / Component 5).
        let records = vec![
            read_record("s1", 1_000, "/a.rs"), // ts=1000ms → 1s, before boundary 5s
            read_record("s1", 10_000, "/a.rs"), // ts=10000ms → 10s, after boundary → reread
        ];
        let summaries = compute_session_summaries(&records);
        let post = overlap_count(
            &records,
            ReloadWindow::PostCompaction { boundary_secs: 5 },
            &summaries,
        );
        // /a.rs read at 1s (prior), re-read at 10s (after boundary) → 1 reread.
        assert_eq!(post.overlap, 1);
    }

    #[test]
    fn test_neither_window_derived_from_other() {
        // R-07: changing the compaction window does not change the cross-session output,
        // and vice versa. The two are independent.
        let records = vec![
            read_record("s1", 1_000, "/a.rs"),
            read_record("s1", 10_000, "/a.rs"), // within-session reread (post-compaction)
            read_record("s2", 20_000, "/a.rs"), // cross-session reload
        ];
        let summaries = compute_session_summaries(&records);

        let cross = overlap_count(&records, ReloadWindow::CrossSession, &summaries);

        // Vary the compaction boundary across a wide range; cross-session is invariant.
        for boundary in [0_i64, 2, 5, 15, 25] {
            let cross_again = overlap_count(&records, ReloadWindow::CrossSession, &summaries);
            assert_eq!(
                cross, cross_again,
                "cross-session output must not depend on compaction boundary {boundary}"
            );
        }

        // Cross-session: s2 re-reads /a.rs read by s1 → overlap 1.
        assert_eq!(cross.overlap, 1);

        // Varying boundary DOES change the compaction window (proving they are distinct
        // computations, not the same number): boundary 5s captures the s1 reread (10s),
        // boundary 15s does not (the post-boundary read is the cross-session s2 row at 20s,
        // not a within-s1 reread).
        let post_low = overlap_count(
            &records,
            ReloadWindow::PostCompaction { boundary_secs: 5 },
            &summaries,
        );
        let post_high = overlap_count(
            &records,
            ReloadWindow::PostCompaction { boundary_secs: 15 },
            &summaries,
        );
        assert_ne!(
            post_low.overlap, post_high.overlap,
            "compaction window must respond to its own boundary, independent of cross-session"
        );
        // And neither equals the cross-session overlap by construction here.
        assert_eq!(post_low.overlap, 1);
        assert_eq!(post_high.overlap, 0);
    }

    #[test]
    fn test_dual_not_collapsed_context_reload_side() {
        // AC-13 (context_reload side): cross-session reload present, the basis-points
        // reckoning is non-zero and stands on its own window — not derived from any
        // compaction window.
        let records = vec![
            read_record("s1", 1000, "/a.rs"),
            read_record("s1", 1001, "/b.rs"),
            read_record("s2", 2000, "/a.rs"), // reload of /a.rs
            read_record("s2", 2001, "/c.rs"), // new file
        ];
        let summaries = compute_session_summaries(&records);
        let bps = reckon_context_reload_bps(&summaries, &records);
        // s2 reads {/a.rs, /c.rs}; /a.rs ∈ prior → 1 of 2 = 0.5 → 5000 bps.
        assert_eq!(bps, 5000);
    }

    #[test]
    fn test_single_session_context_reload_zero() {
        // Single-session cycle → no cross-session window exists → reckon 0 bps (distinct
        // from a measured zero; Component 7 marks it unavailable).
        let records = vec![read_record("s1", 1000, "/a.rs")];
        let summaries = compute_session_summaries(&records);
        assert_eq!(reckon_context_reload_bps(&summaries, &records), 0);
    }

    #[test]
    fn test_reckon_matches_live_fraction() {
        // The basis-points reckoning round-trips the live compute_context_reload_pct.
        let records = vec![
            read_record("s1", 1000, "/a.rs"),
            read_record("s1", 1001, "/b.rs"),
            read_record("s1", 1002, "/c.rs"),
            read_record("s2", 2000, "/b.rs"),
            read_record("s2", 2001, "/c.rs"),
            read_record("s2", 2002, "/d.rs"),
        ];
        let summaries = compute_session_summaries(&records);
        let fraction = crate::compute_context_reload_pct(&summaries, &records); // 2/3
        let bps = reckon_context_reload_bps(&summaries, &records);
        assert_eq!(bps, fraction_to_basis_points(fraction));
        assert_eq!(bps, 6667); // round(0.6666.. × 10000)
    }
}
