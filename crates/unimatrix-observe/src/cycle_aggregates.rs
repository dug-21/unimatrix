//! Rank-1/2/3 durable aggregate reckoning (crt-055 Component 3, Wave 2).
//!
//! Derives the highest decision-value per-cycle aggregates (ass-077 RQ-2) from
//! **content-opaque, durable** streams — never the transcript:
//!   - **rank-1** phase aggregates from `cycle_events` (incl. #556 never-closed),
//!   - **rank-2** rework/total session num/den from `SessionRecord.outcome`,
//!   - **rank-3** knowledge-reuse-all-served from `query_log ∪ injection_log` (#320).
//!
//! Every output is a plain `i64`. Ratios are stored as num/den **pairs**, never
//! pre-divided (R-17 / ADR-004 #5039) — the rate is derived at presentation so a
//! measured `0 of N` stays distinguishable from an unavailable `0 of 0` (ADR-003).
//!
//! These functions produce values **into** a [`CycleAggregates`]; they never write the
//! `cycle_review_index` table. The single `store_cycle_review()` writer (Component 2)
//! persists at the full-pipeline return only (ADR-002, pattern #4178).
//!
//! ## Evicted / undeclared sessions (#4140, lesson #4140)
//! An evicted SM session can drop a cycle's attribution entirely, so an empty input
//! slice is an **honest partial**, not a fabricated zero. These functions return the
//! genuine count for what they were handed (0 when handed nothing); the per-metric
//! availability flags (Component 7 `MetricAvailability`) mark the metric *unavailable*
//! rather than reporting a believable `0`. Reckoning never fabricates a source.

use std::collections::HashSet;

use unimatrix_store::{InjectionLogRecord, QueryLogRecord};

use crate::fail_loud_guard::CycleAggregates;
use crate::types::CycleEventRecord;

pub mod compaction_reckoning;
pub use compaction_reckoning::reckon_compaction_reread;

/// Rank-1 phase aggregates derived from a cycle's `cycle_events` timeline.
///
/// All counts are `i64` to match the `cycle_review_index` v5 column widths. An empty
/// timeline yields all-zero — the caller marks `phase_metrics_available=false` (the zero
/// is never presented bare).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PhaseAggregates {
    /// Distinct phase names ever entered in the cycle. A re-opened phase does NOT
    /// increment this (it is rework, not a new phase — R-15).
    pub phase_count: i64,
    /// Number of `cycle_phase_end` transitions observed.
    pub phase_transition_count: i64,
    /// Phase re-entries: a phase name entered again after it was already seen (loops).
    pub phase_rework_count: i64,
    /// #556: phases entered but never closed — no matching `cycle_phase_end` and no
    /// `cycle_stop` closed them. A genuine declared-but-never-closed hotspot.
    pub phase_unclosed_count: i64,
    /// Σ of closed-phase window durations in seconds. An unclosed phase contributes 0
    /// (no fabricated end time).
    pub phase_total_duration_secs: i64,
}

/// Reckon rank-1 phase aggregates from a cycle's `cycle_events` rows.
///
/// The cycle event model (verified against the live writer, `tools.rs` / `phase_narrative.rs`):
/// - `cycle_start` opens the timeline and announces the first phase via `next_phase`.
/// - `cycle_phase_end` ends the phase named in `phase` and announces the next via
///   `next_phase`. Each one is a transition.
/// - `cycle_stop` closes the cycle and any still-open phase at stop time.
///
/// There is **no** `cycle_phase_start` event type in this codebase; a phase is "entered"
/// when it first becomes the active phase (via `cycle_start.next_phase` or
/// `cycle_phase_end.next_phase`). A phase whose name re-opens after being closed counts as
/// rework, not a second `phase_count` (R-15).
///
/// Durations sum from `timestamp` deltas (cycle_events `timestamp` is Unix **seconds**);
/// only closed windows contribute. `auto_close` (Component 8) writes a `cycle_stop` BEFORE
/// this runs when enabled, so a closed cycle does not surface a false never-closed (R-14).
pub fn reckon_phase_aggregates(events: &[CycleEventRecord]) -> PhaseAggregates {
    if events.is_empty() {
        return PhaseAggregates::default();
    }

    // Events arrive sorted (timestamp ASC, seq ASC) from the SQL query, but sort defensively
    // so the reckoning is order-independent of the caller (stable, seq tiebreak).
    let mut ordered: Vec<&CycleEventRecord> = events.iter().collect();
    ordered.sort_by(|a, b| a.timestamp.cmp(&b.timestamp).then(a.seq.cmp(&b.seq)));

    // Distinct phase names ever entered. A re-entry does not grow this set.
    let mut seen_phase: HashSet<String> = HashSet::new();
    // The currently-active phase and when it was entered (open window).
    let mut current_phase: Option<String> = None;
    let mut current_start_secs: i64 = 0;
    // Phase names left open after the full walk (no matching end, no cycle_stop) → #556.
    let mut open_unclosed: HashSet<String> = HashSet::new();

    let mut transitions: i64 = 0;
    let mut rework: i64 = 0;
    let mut total_duration_secs: i64 = 0;

    // Enter a phase: track distinctness + rework, mark it open.
    let enter_phase =
        |phase: String, ts: i64, seen: &mut HashSet<String>, rework: &mut i64| -> (String, i64) {
            if seen.contains(&phase) {
                // Re-entry of a phase already declared → rework loop, NOT a new phase.
                *rework += 1;
            } else {
                seen.insert(phase.clone());
            }
            (phase, ts)
        };

    for e in ordered {
        match e.event_type.as_str() {
            "cycle_start" => {
                // The first phase is announced via next_phase (matching phase_narrative.rs).
                if let Some(np) = e.next_phase.clone() {
                    open_unclosed.insert(np.clone());
                    let (p, ts) = enter_phase(np, e.timestamp, &mut seen_phase, &mut rework);
                    current_phase = Some(p);
                    current_start_secs = ts;
                }
            }
            "cycle_phase_end" => {
                transitions += 1;
                // Close the phase being ended. Prefer the explicit `phase` field; fall back
                // to the tracked current phase when the event omits it.
                let ending = e.phase.clone().or_else(|| current_phase.clone());
                if let Some(ending_name) = ending.as_ref() {
                    // Duration of the just-closed window (only when it was the open phase).
                    if current_phase.as_deref() == Some(ending_name.as_str()) {
                        total_duration_secs += (e.timestamp - current_start_secs).max(0);
                    }
                    open_unclosed.remove(ending_name);
                }
                // Transition to the announced next phase (if any).
                if let Some(np) = e.next_phase.clone() {
                    open_unclosed.insert(np.clone());
                    let (p, ts) = enter_phase(np, e.timestamp, &mut seen_phase, &mut rework);
                    current_phase = Some(p);
                    current_start_secs = ts;
                } else {
                    current_phase = None;
                }
            }
            "cycle_stop" => {
                // The cycle ended: close any still-open phase at stop time and count its
                // duration. A phase closed by cycle_stop is NOT never-closed (the cycle
                // genuinely ended) — clear the open set (#556 false-positive guard, R-14).
                if let Some(p) = current_phase.as_ref()
                    && open_unclosed.contains(p)
                {
                    total_duration_secs += (e.timestamp - current_start_secs).max(0);
                }
                open_unclosed.clear();
                current_phase = None;
            }
            _ => {
                // Unknown event type — ignore (audit-trail rows unrelated to phases).
            }
        }
    }

    PhaseAggregates {
        phase_count: seen_phase.len() as i64,
        phase_transition_count: transitions,
        phase_rework_count: rework,
        phase_unclosed_count: open_unclosed.len() as i64,
        phase_total_duration_secs: total_duration_secs,
    }
}

/// A minimal view of a session needed for rank-2: just its `outcome` text.
///
/// Implemented for `unimatrix_store::SessionRecord` so the handler can pass session records
/// directly, and trivially constructible in tests.
pub trait SessionOutcome {
    /// The session's outcome text, if recorded (e.g. tags including `result:rework`).
    fn outcome_text(&self) -> Option<&str>;
}

impl SessionOutcome for unimatrix_store::SessionRecord {
    fn outcome_text(&self) -> Option<&str> {
        self.outcome.as_deref()
    }
}

/// Classify a session outcome as rework/failure.
///
/// Reuses the EXACT predicate established in the review pipeline (`tools.rs` Step 15,
/// human-override classification): a case-insensitive match on `result:rework` or
/// `result:failed`. Do NOT invent a new outcome set — this keeps the durable column
/// consistent with the existing `RetrospectiveReport.rework_session_count`.
pub fn is_rework_outcome(outcome: Option<&str>) -> bool {
    match outcome {
        Some(o) => {
            let lower = o.to_lowercase();
            lower.contains("result:rework") || lower.contains("result:failed")
        }
        None => false,
    }
}

/// Reckon the rank-2 rework ratio as a num/den **PAIR** (never pre-divided — R-17).
///
/// Returns `(rework_session_count, total_session_count)`. `total` is every session handed
/// in (the declared sessions attributed to the cycle); `rework` is the subset classified by
/// [`is_rework_outcome`]. The caller stores BOTH so presentation can distinguish
/// `0 of 0` (unavailable) from a measured `0 of N` (ADR-003).
pub fn reckon_rework_ratio<S: SessionOutcome>(sessions: &[S]) -> (i64, i64) {
    let total = sessions.len() as i64;
    let rework = sessions
        .iter()
        .filter(|s| is_rework_outcome(s.outcome_text()))
        .count() as i64;
    (rework, total)
}

/// Reckon the rank-3 knowledge-reuse-all-served count (#320): the size of the UNION of
/// distinct entry ids served via `query_log` ∪ `injection_log` for the cycle's sessions.
///
/// All served, NOT only same-cycle-tagged (R-16). An entry served via BOTH logs counts
/// ONCE (the `HashSet` dedups by entry id). This mirrors the established union path in the
/// review pipeline (`tools.rs::compute_knowledge_reuse_for_sessions`): query_log carries a
/// JSON array of ids in `result_entry_ids`; injection_log carries a single `entry_id` per
/// row.
///
/// The handler resolves the cycle's sessions and loads both logs via
/// `scan_query_log_by_sessions` / `scan_injection_log_by_sessions` before calling this.
/// Passing the wrong table/column would surface here as a believable zero — guarded by a
/// negative-control test that asserts a non-zero union for seeded records.
pub fn reckon_knowledge_reuse_served(
    query_logs: &[QueryLogRecord],
    injection_logs: &[InjectionLogRecord],
) -> i64 {
    let mut served: HashSet<u64> = HashSet::new();
    for record in query_logs {
        // result_entry_ids is a JSON array string; a malformed/empty value contributes
        // nothing (treated as no ids) rather than aborting — honest partial, never a panic.
        let ids: Vec<u64> = serde_json::from_str(&record.result_entry_ids).unwrap_or_default();
        served.extend(ids);
    }
    for record in injection_logs {
        served.insert(record.entry_id);
    }
    served.len() as i64
}

/// Populate the rank-1/2/3 fields of a [`CycleAggregates`] in one call.
///
/// Convenience for the review pipeline: it gathers `cycle_events`, the cycle's session
/// records, and the two served-knowledge logs, and writes the eight rank-1/2/3 `i64` fields
/// onto `agg`. It does NOT touch transcript / compaction / reload fields (Components 4/5/6),
/// and it never persists — Component 2's single writer does that.
pub fn populate_rank_1_2_3<S: SessionOutcome>(
    agg: &mut CycleAggregates,
    events: &[CycleEventRecord],
    sessions: &[S],
    query_logs: &[QueryLogRecord],
    injection_logs: &[InjectionLogRecord],
) {
    let phase = reckon_phase_aggregates(events);
    agg.phase_count = phase.phase_count;
    agg.phase_transition_count = phase.phase_transition_count;
    agg.phase_rework_count = phase.phase_rework_count;
    agg.phase_unclosed_count = phase.phase_unclosed_count;
    agg.phase_total_duration_secs = phase.phase_total_duration_secs;

    let (rework, total) = reckon_rework_ratio(sessions);
    agg.rework_session_count = rework;
    agg.total_session_count = total;

    agg.knowledge_reuse_served_count = reckon_knowledge_reuse_served(query_logs, injection_logs);
}

#[cfg(test)]
mod tests;
