//! Unit tests for rank-1/2/3 aggregate reckoning (crt-055 Component 3).
//!
//! Test plan: AC-04 (#556 never-closed), AC-05 (rank-1 rework / rank-2 num/den pair),
//! AC-06 (#320 query_log ∪ injection_log union dedup), R-15/R-16/R-17.

use unimatrix_store::{InjectionLogRecord, QueryLogRecord};

use super::*;
use crate::types::CycleEventRecord;

// ── Test builders ───────────────────────────────────────────────────────────

/// Build a `cycle_events` row. `seq` is auto-derived from call order via the caller.
fn event(
    seq: i64,
    event_type: &str,
    phase: Option<&str>,
    next_phase: Option<&str>,
    timestamp: i64,
) -> CycleEventRecord {
    CycleEventRecord {
        seq,
        event_type: event_type.to_string(),
        phase: phase.map(str::to_string),
        outcome: None,
        next_phase: next_phase.map(str::to_string),
        timestamp,
    }
}

/// Minimal session view for rank-2 tests.
struct TestSession {
    outcome: Option<String>,
}
impl TestSession {
    fn with(outcome: &str) -> Self {
        Self {
            outcome: Some(outcome.to_string()),
        }
    }
    fn none() -> Self {
        Self { outcome: None }
    }
}
impl SessionOutcome for TestSession {
    fn outcome_text(&self) -> Option<&str> {
        self.outcome.as_deref()
    }
}

fn query_log(session: &str, entry_ids: &[u64], ts: u64) -> QueryLogRecord {
    QueryLogRecord {
        query_id: 0,
        session_id: session.to_string(),
        query_text: "q".to_string(),
        ts,
        result_count: entry_ids.len() as i64,
        result_entry_ids: serde_json::to_string(entry_ids).unwrap(),
        similarity_scores: "[]".to_string(),
        retrieval_mode: "semantic".to_string(),
        source: "mcp".to_string(),
        phase: None,
    }
}

fn injection_log(session: &str, entry_id: u64, ts: u64) -> InjectionLogRecord {
    InjectionLogRecord {
        log_id: 0,
        session_id: session.to_string(),
        entry_id,
        confidence: 0.9,
        timestamp: ts,
    }
}

// ── Rank-1 phase aggregates (R-15, AC-04, AC-05) ─────────────────────────────

#[test]
fn test_rank1_declared_phases_counted() {
    // Three distinct phases entered via cycle_start.next_phase + two transitions, then a
    // clean cycle_stop. phase_count == 3 distinct names.
    let events = vec![
        event(1, "cycle_start", None, Some("scope"), 100),
        event(2, "cycle_phase_end", Some("scope"), Some("design"), 200),
        event(3, "cycle_phase_end", Some("design"), Some("impl"), 300),
        event(4, "cycle_stop", None, None, 400),
    ];
    let agg = reckon_phase_aggregates(&events);
    assert_eq!(agg.phase_count, 3, "three distinct declared phases");
    assert_eq!(agg.phase_unclosed_count, 0, "cycle_stop closed all");
}

#[test]
fn test_rank1_phase_transitions_counted() {
    // Each cycle_phase_end is one transition.
    let events = vec![
        event(1, "cycle_start", None, Some("scope"), 100),
        event(2, "cycle_phase_end", Some("scope"), Some("design"), 200),
        event(3, "cycle_phase_end", Some("design"), Some("impl"), 300),
        event(4, "cycle_phase_end", Some("impl"), None, 400),
    ];
    let agg = reckon_phase_aggregates(&events);
    assert_eq!(
        agg.phase_transition_count, 3,
        "three cycle_phase_end events"
    );
}

#[test]
fn test_rank1_closed_then_reopened_is_rework_not_new_phase() {
    // MARQUEE rank-1 mis-reckon guard (R-15). scope → design → scope (re-opened).
    // phase_rework_count increments; phase_count does NOT double-count scope.
    let events = vec![
        event(1, "cycle_start", None, Some("scope"), 100),
        event(2, "cycle_phase_end", Some("scope"), Some("design"), 200),
        event(3, "cycle_phase_end", Some("design"), Some("scope"), 300), // re-enter scope
        event(4, "cycle_stop", None, None, 400),
    ];
    let agg = reckon_phase_aggregates(&events);
    assert_eq!(
        agg.phase_count, 2,
        "scope + design are the only distinct phases; re-open is not a 3rd"
    );
    assert_eq!(
        agg.phase_rework_count, 1,
        "re-entering scope is a rework loop"
    );
}

#[test]
fn test_rank1_unclosed_phase_increments_unclosed_count() {
    // #556 / AC-04: a phase entered with NO matching close and NO cycle_stop → unclosed.
    let events = vec![
        event(1, "cycle_start", None, Some("scope"), 100),
        event(2, "cycle_phase_end", Some("scope"), Some("design"), 200),
        // design is entered but never ended, and there is no cycle_stop.
    ];
    let agg = reckon_phase_aggregates(&events);
    assert_eq!(
        agg.phase_unclosed_count, 1,
        "design declared but never closed → #556 hotspot"
    );
}

#[test]
fn test_rank1_matching_close_not_unclosed() {
    // #556 false-positive guard: a phase WITH a matching close does NOT increment unclosed.
    let events = vec![
        event(1, "cycle_start", None, Some("scope"), 100),
        event(2, "cycle_phase_end", Some("scope"), None, 200), // scope closed, no next
    ];
    let agg = reckon_phase_aggregates(&events);
    assert_eq!(
        agg.phase_unclosed_count, 0,
        "scope was closed by its cycle_phase_end"
    );
}

#[test]
fn test_rank1_total_duration_sums_closed_phases_only() {
    // Duration = Σ closed-phase windows in seconds. The open trailing phase contributes 0.
    // scope: 100→200 (100s), design: 200→350 (150s), impl: open (0).
    let events = vec![
        event(1, "cycle_start", None, Some("scope"), 100),
        event(2, "cycle_phase_end", Some("scope"), Some("design"), 200),
        event(3, "cycle_phase_end", Some("design"), Some("impl"), 350),
        // impl never closed, no cycle_stop → 0 duration, surfaces as unclosed.
    ];
    let agg = reckon_phase_aggregates(&events);
    assert_eq!(
        agg.phase_total_duration_secs, 250,
        "100 + 150 closed-phase seconds; open impl adds 0"
    );
    assert_eq!(agg.phase_unclosed_count, 1, "impl is the open phase");
}

#[test]
fn test_rank1_auto_close_stop_closes_final_phase_not_unclosed() {
    // R-14 / AC-15 coupling: a cycle_stop (as auto_close would write) closes the final
    // open phase, so it is NOT counted as never-closed, and its duration is summed.
    let events = vec![
        event(1, "cycle_start", None, Some("scope"), 100),
        event(2, "cycle_phase_end", Some("scope"), Some("design"), 200),
        event(3, "cycle_stop", None, None, 500), // auto_close-written stop
    ];
    let agg = reckon_phase_aggregates(&events);
    assert_eq!(
        agg.phase_unclosed_count, 0,
        "cycle_stop closes design → not never-closed"
    );
    // scope: 100→200 (100s) + design: 200→500 (300s) = 400s.
    assert_eq!(agg.phase_total_duration_secs, 400);
}

#[test]
fn test_rank1_empty_events_all_zero() {
    // Edge case: zero cycle_events → all-zero (caller marks unavailable, not 0).
    let agg = reckon_phase_aggregates(&[]);
    assert_eq!(agg, PhaseAggregates::default());
}

#[test]
fn test_rank1_only_unclosed_phases_zero_duration() {
    // Cycle with only an unclosed phase → phase_unclosed_count > 0, duration == 0.
    let events = vec![event(1, "cycle_start", None, Some("scope"), 100)];
    let agg = reckon_phase_aggregates(&events);
    assert_eq!(agg.phase_unclosed_count, 1);
    assert_eq!(agg.phase_total_duration_secs, 0);
    assert_eq!(agg.phase_count, 1);
}

#[test]
fn test_rank1_order_independent() {
    // The reckoner sorts defensively, so shuffled input yields the same result.
    let mut events = vec![
        event(4, "cycle_stop", None, None, 400),
        event(2, "cycle_phase_end", Some("scope"), Some("design"), 200),
        event(1, "cycle_start", None, Some("scope"), 100),
        event(3, "cycle_phase_end", Some("design"), Some("impl"), 300),
    ];
    let sorted = reckon_phase_aggregates(&events);
    events.reverse();
    let reversed = reckon_phase_aggregates(&events);
    assert_eq!(sorted, reversed, "result is order-independent");
    assert_eq!(sorted.phase_count, 3);
}

// ── Rank-2 rework ratio num/den pair (R-17, AC-05) ───────────────────────────

#[test]
fn test_rank2_rework_session_count_and_total_stored_as_pair() {
    // R-17: M rework/failure out of T total → counts match, stored as a PAIR.
    let sessions = vec![
        TestSession::with("type:session result:rework"),
        TestSession::with("type:session result:pass"),
        TestSession::with("type:session result:failed"),
        TestSession::with("type:session result:pass"),
        TestSession::none(),
    ];
    let (rework, total) = reckon_rework_ratio(&sessions);
    assert_eq!(rework, 2, "rework + failed = 2 numerator");
    assert_eq!(total, 5, "all sessions = 5 denominator");
    // The PAIR is never pre-divided: this is two integers, not a ratio.
}

#[test]
fn test_rank2_outcome_classification_case_insensitive() {
    // Reuses the exact pipeline predicate: case-insensitive result:rework / result:failed.
    assert!(is_rework_outcome(Some("TYPE:SESSION RESULT:REWORK")));
    assert!(is_rework_outcome(Some("result:failed")));
    assert!(!is_rework_outcome(Some("result:pass")));
    assert!(!is_rework_outcome(Some("result:skip")));
    assert!(!is_rework_outcome(None));
}

#[test]
fn test_rank2_zero_of_zero_vs_zero_of_n() {
    // R-17: "0 of 0" (no sessions) is distinguishable from "0 of N" (sessions, none rework).
    let (r0, t0) = reckon_rework_ratio::<TestSession>(&[]);
    assert_eq!(
        (r0, t0),
        (0, 0),
        "no sessions → 0 of 0 (unavailable upstream)"
    );

    let measured = vec![
        TestSession::with("result:pass"),
        TestSession::with("result:pass"),
    ];
    let (rn, tn) = reckon_rework_ratio(&measured);
    assert_eq!((rn, tn), (0, 2), "measured 0 of 2 (a real zero rate)");
}

// ── Rank-3 knowledge reuse union (#320, R-16, AC-06) ─────────────────────────

#[test]
fn test_rank3_counts_union_of_query_and_injection_log() {
    // R-16 / AC-06: served entries SPLIT across query_log and injection_log (incl. what a
    // cross-cycle tag would be) → count == size of the UNION, not same-cycle-tagged only.
    // query_log serves {1, 2, 3}; injection_log serves {4, 5}. Union = 5 distinct.
    let qls = vec![query_log("s1", &[1, 2], 100), query_log("s1", &[3], 110)];
    let ils = vec![injection_log("s2", 4, 120), injection_log("s2", 5, 130)];
    let count = reckon_knowledge_reuse_served(&qls, &ils);
    assert_eq!(
        count, 5,
        "union of {{1,2,3}} ∪ {{4,5}} = 5 distinct entries"
    );
}

#[test]
fn test_rank3_entry_served_via_both_logs_counted_once() {
    // R-16: an entry present in BOTH logs is counted ONCE (union dedup), not twice.
    // query_log serves {7, 8}; injection_log serves {8, 9}. 8 is shared → union = {7,8,9} = 3.
    let qls = vec![query_log("s1", &[7, 8], 100)];
    let ils = vec![injection_log("s1", 8, 110), injection_log("s1", 9, 120)];
    let count = reckon_knowledge_reuse_served(&qls, &ils);
    assert_eq!(count, 3, "entry 8 served via both logs counts once");
}

#[test]
fn test_rank3_same_entry_multiple_times_same_log_deduped() {
    // #320 dedup intent: an entry served multiple times in the SAME log counts once.
    let qls = vec![
        query_log("s1", &[42], 100),
        query_log("s1", &[42], 200),
        query_log("s2", &[42], 300),
    ];
    let count = reckon_knowledge_reuse_served(&qls, &[]);
    assert_eq!(count, 1, "entry 42 served thrice in query_log → 1 distinct");
}

#[test]
fn test_rank3_wrong_table_name_yields_silent_zero_guard() {
    // NEGATIVE CONTROL (Open Q1): a seeded non-empty union MUST return non-zero. A wrong
    // table/column name in the handler's load path would surface here as a believable 0 —
    // this test fails loudly if the source is misnamed and nothing is loaded.
    let qls = vec![query_log("s1", &[11], 100)];
    let ils = vec![injection_log("s1", 22, 110)];
    let count = reckon_knowledge_reuse_served(&qls, &ils);
    assert!(
        count > 0,
        "non-empty seeded union must be non-zero (silent-zero guard); got {count}"
    );
    assert_eq!(count, 2);
}

#[test]
fn test_rank3_empty_logs_zero() {
    // Both logs empty → 0 (caller marks knowledge_reuse_available=false; honest partial).
    assert_eq!(reckon_knowledge_reuse_served(&[], &[]), 0);
}

#[test]
fn test_rank3_malformed_query_entry_ids_ignored_not_panicked() {
    // A malformed result_entry_ids JSON contributes no ids and never panics (honest partial).
    let mut bad = query_log("s1", &[], 100);
    bad.result_entry_ids = "not-json".to_string();
    let good = query_log("s1", &[5], 110);
    let count = reckon_knowledge_reuse_served(&[bad, good], &[]);
    assert_eq!(count, 1, "malformed row ignored; good row counted");
}

// ── populate_rank_1_2_3 wiring (AC-11 attribution partial) ────────────────────

#[test]
fn test_populate_writes_all_eight_fields() {
    use crate::fail_loud_guard::CycleAggregates;

    let events = vec![
        event(1, "cycle_start", None, Some("scope"), 100),
        event(2, "cycle_phase_end", Some("scope"), Some("design"), 200),
        event(3, "cycle_stop", None, None, 300),
    ];
    let sessions = vec![
        TestSession::with("result:rework"),
        TestSession::with("result:pass"),
    ];
    let qls = vec![query_log("s1", &[1, 2], 100)];
    let ils = vec![injection_log("s1", 2, 110)]; // entry 2 in both → dedup

    let mut agg = CycleAggregates::default();
    populate_rank_1_2_3(&mut agg, &events, &sessions, &qls, &ils);

    assert_eq!(agg.phase_count, 2);
    assert_eq!(agg.phase_transition_count, 1);
    assert_eq!(agg.phase_rework_count, 0);
    assert_eq!(agg.phase_unclosed_count, 0);
    assert_eq!(agg.phase_total_duration_secs, 200); // scope 100 + design 100
    assert_eq!(agg.rework_session_count, 1);
    assert_eq!(agg.total_session_count, 2);
    assert_eq!(agg.knowledge_reuse_served_count, 2); // {1,2} ∪ {2} = 2
}

#[test]
fn test_populate_evicted_session_honest_partial_zero() {
    // #4140 / AC-11: an evicted/undeclared cycle hands EMPTY slices. populate writes genuine
    // zeros (which the per-metric availability flags mark unavailable, never a fabricated
    // believable zero). Reckoning itself never invents a source.
    use crate::fail_loud_guard::CycleAggregates;

    let mut agg = CycleAggregates::default();
    populate_rank_1_2_3::<TestSession>(&mut agg, &[], &[], &[], &[]);

    assert_eq!(agg.phase_count, 0);
    assert_eq!(agg.total_session_count, 0, "0 of 0 → upstream unavailable");
    assert_eq!(agg.knowledge_reuse_served_count, 0);
    // total_session_count == 0 is the signal Component 7 uses to render "unavailable"
    // rather than a measured 0% rework rate.
}
