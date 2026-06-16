//! Unit tests for crt-055 Component 9 review-aggregate orchestration.
//!
//! These cover the pure (non-DB) state methods: rank-2/3 + rank-1 population,
//! reload basis-points landing, the STEP-5 availability derivation, and the
//! fail-loud render block (AC-01 "unavailable" not "0", AC-21 coarse/directional).
//! The full pipeline ordering, the four-returns / no-clobber #5022 assertions
//! (AC-17/18), the leak gate (AC-19), and the #206-4 no-durable-column check
//! (AC-16) live in `tools.rs`'s handler test module where the handler state and a
//! real store are available.

use super::ReviewAggregateState;
use unimatrix_observe::CycleEventRecord;
use unimatrix_store::{InjectionLogRecord, QueryLogRecord, SessionLifecycleStatus, SessionRecord};

fn session(session_id: &str, outcome: Option<&str>) -> SessionRecord {
    SessionRecord {
        session_id: session_id.to_string(),
        feature_cycle: Some("feat-x".to_string()),
        agent_role: None,
        started_at: 0,
        ended_at: None,
        status: SessionLifecycleStatus::Completed,
        compaction_count: 0,
        outcome: outcome.map(str::to_string),
        total_injections: 0,
        keywords: None,
    }
}

fn cycle_start(seq: i64, ts: i64, next_phase: &str) -> CycleEventRecord {
    CycleEventRecord {
        seq,
        event_type: "cycle_start".to_string(),
        phase: None,
        outcome: None,
        next_phase: Some(next_phase.to_string()),
        timestamp: ts,
    }
}

fn phase_end(seq: i64, ts: i64, phase: &str, next_phase: Option<&str>) -> CycleEventRecord {
    CycleEventRecord {
        seq,
        event_type: "cycle_phase_end".to_string(),
        phase: Some(phase.to_string()),
        outcome: None,
        next_phase: next_phase.map(str::to_string),
        timestamp: ts,
    }
}

fn cycle_stop(seq: i64, ts: i64) -> CycleEventRecord {
    CycleEventRecord {
        seq,
        event_type: "cycle_stop".to_string(),
        phase: None,
        outcome: Some("result:success".to_string()),
        next_phase: None,
        timestamp: ts,
    }
}

fn query_log(session_id: &str, ids_json: &str) -> QueryLogRecord {
    QueryLogRecord {
        query_id: 0,
        session_id: session_id.to_string(),
        query_text: String::new(),
        ts: 0,
        result_count: 0,
        result_entry_ids: ids_json.to_string(),
        similarity_scores: String::new(),
        retrieval_mode: String::new(),
        source: String::new(),
        phase: None,
    }
}

fn injection_log(session_id: &str, entry_id: u64) -> InjectionLogRecord {
    InjectionLogRecord {
        log_id: 0,
        session_id: session_id.to_string(),
        entry_id,
        confidence: 0.0,
        timestamp: 0,
    }
}

// ── STEP 3 rank-2 / rank-3 ──────────────────────────────────────────────────

#[test]
fn test_populate_ranks_2_3_rework_ratio_pair() {
    let mut state = ReviewAggregateState::new();
    let sessions = vec![
        session("s1", Some("result:success")),
        session("s2", Some("result:rework")),
        session("s3", Some("result:failed")),
    ];
    state.populate_ranks_2_3(&sessions, &[], &[]);
    let agg = state.aggregates();
    // rework = 2 (rework + failed), total = 3 — carried as a PAIR (never pre-divided).
    assert_eq!(agg.rework_session_count, 2);
    assert_eq!(agg.total_session_count, 3);
}

#[test]
fn test_populate_ranks_2_3_knowledge_union_dedup() {
    let mut state = ReviewAggregateState::new();
    let sessions = vec![session("s1", Some("result:success"))];
    let q = vec![query_log("s1", "[1,2,3]")];
    let i = vec![injection_log("s1", 3), injection_log("s1", 4)];
    state.populate_ranks_2_3(&sessions, &q, &i);
    // union {1,2,3} ∪ {3,4} = {1,2,3,4} = 4 (entry 3 served via both counts once).
    assert_eq!(state.aggregates().knowledge_reuse_served_count, 4);
}

#[test]
fn test_populate_ranks_2_3_sets_knowledge_log_nonempty_ctx() {
    let mut state = ReviewAggregateState::new();
    state.populate_ranks_2_3(&[session("s1", None)], &[query_log("s1", "[1]")], &[]);
    let avail = state.availability();
    assert!(
        avail.knowledge_reuse_available,
        "a served query makes knowledge reuse available"
    );
}

// ── STEP 3 rank-1 ───────────────────────────────────────────────────────────

#[test]
fn test_populate_rank_1_unclosed_phase_is_556_hotspot() {
    let mut state = ReviewAggregateState::new();
    // Two declared phases, second never closed, NO cycle_stop → 1 never-closed.
    let events = vec![
        cycle_start(0, 100, "design"),
        phase_end(1, 200, "design", Some("build")),
    ];
    state.populate_rank_1(&events);
    let agg = state.aggregates();
    assert_eq!(agg.phase_count, 2, "design + build declared");
    assert_eq!(
        agg.phase_unclosed_count, 1,
        "build never closed (#556 never-closed)"
    );
}

#[test]
fn test_populate_rank_1_auto_close_stop_clears_unclosed() {
    let mut state = ReviewAggregateState::new();
    // Same timeline but WITH a cycle_stop (as auto_close writes) → not never-closed.
    let events = vec![
        cycle_start(0, 100, "design"),
        phase_end(1, 200, "design", Some("build")),
        cycle_stop(2, 300),
    ];
    state.populate_rank_1(&events);
    assert_eq!(
        state.aggregates().phase_unclosed_count,
        0,
        "a cycle_stop (auto_close) closes the final phase — not a false never-closed (R-14)"
    );
}

#[test]
fn test_populate_rank_1_sets_cycle_events_count_ctx() {
    let mut state = ReviewAggregateState::new();
    state.populate_rank_1(&[cycle_start(0, 1, "design")]);
    assert!(
        state.availability().phase_metrics_available,
        "non-empty cycle_events → phase metrics available"
    );
}

// ── STEP 4 reload ───────────────────────────────────────────────────────────

#[test]
fn test_populate_reload_single_session_unavailable() {
    let mut state = ReviewAggregateState::new();
    state.populate_reload(&[], &[], 1);
    assert!(
        !state.availability().context_reload_available,
        "single-session cycle has no cross-session window — unavailable, never 0%"
    );
}

#[test]
fn test_populate_reload_two_sessions_available() {
    let mut state = ReviewAggregateState::new();
    state.populate_reload(&[], &[], 2);
    assert!(
        state.availability().context_reload_available,
        "≥2 sessions → a cross-session reload window exists"
    );
}

// ── STEP 5 availability + render (AC-01 / AC-21) ────────────────────────────

#[test]
fn test_empty_cycle_renders_unavailable_never_zero() {
    // A fully empty cycle: no fold, no cycle_events, no sessions, no compaction.
    let state = ReviewAggregateState::new();
    let avail = state.availability();
    let block = state.render_block(&avail);

    // Every per-metric flag is false; each renders "unavailable", NEVER a bare "0".
    assert!(block.contains("Phases: unavailable"), "block: {block}");
    assert!(
        block.contains("Rework rate: unavailable"),
        "0 of 0 → unavailable, not a measured 0 (R-17): {block}"
    );
    assert!(
        block.contains("Context reload: unavailable"),
        "single-session → unavailable, never 0%: {block}"
    );
    assert!(
        block.contains("Errors (signal): unavailable"),
        "no fold → unavailable: {block}"
    );
}

#[test]
fn test_behavioral_signals_render_directional_when_present() {
    let mut state = ReviewAggregateState::new();
    // Provide a non-empty fold so the behavioral signals are available, then assert
    // they carry the coarse/directional qualifier, distinct from exact aggregates.
    state.land_fold_for_test(7, 2);
    let avail = state.availability();
    let block = state.render_block(&avail);
    assert!(
        block.contains("Errors (signal): ~7 (directional)"),
        "regex-derived error count is coarse/directional (AC-21): {block}"
    );
    assert!(
        block.contains("Refusals (signal): ~2 (directional)"),
        "regex-derived refusal count is coarse/directional (AC-21): {block}"
    );
}

#[test]
fn test_measured_zero_distinct_from_unavailable() {
    let mut state = ReviewAggregateState::new();
    // 0 rework of 3 sessions → a MEASURED 0%, distinct from "unavailable".
    state.populate_ranks_2_3(
        &[
            session("s1", Some("result:success")),
            session("s2", Some("result:success")),
            session("s3", Some("result:success")),
        ],
        &[],
        &[],
    );
    let avail = state.availability();
    let block = state.render_block(&avail);
    assert!(
        block.contains("Rework rate: 0 of 3"),
        "0 of N is a measured rate, not 'unavailable': {block}"
    );
}
