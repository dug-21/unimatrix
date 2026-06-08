//! AC-11 MERGE GATE — `continuity_simulated_lifecycle` (crt-052 Wave B).
//!
//! The ONLY pre-merge proof of the primary path. Faithful per-turn-drain
//! simulation: >=3 drain cycles with deltas applied BETWEEN each drain, then
//! re-register and cycle review. A single-turn happy path does NOT satisfy this
//! (R-05 faithfulness guard). Split into its own file to keep both test files
//! under the 500-line rule (#693); helpers come from the parent test module via
//! `super::*`.

// Helpers (`CapturingSink`, `TestClock`, `buf_with`, `hold_with_clock`,
// `held_snapshot_bytes`) and the store/registry types live in the parent test
// module; the child module sees them through `super::*` because items are in
// scope within the same crate test build.
use super::{
    Arc, CapturingSink, Duration, HeldBufferScan, SessionRegistry, TRIGGER_CAP_EVICT, TestClock,
    TranscriptHold, buf_with, held_snapshot_bytes, hold_with_clock,
};

/// Wires a `SessionRegistry` to a real `TranscriptHold` (the production pair)
/// and drives the full multi-turn lifecycle through the registry's own
/// drain/register/delta entry points — proving the hold integration end-to-end,
/// not just the store in isolation.
#[test]
fn continuity_simulated_lifecycle() {
    let clock = TestClock::new();
    let sink = Arc::new(CapturingSink::default());
    let hold = Arc::new(TranscriptHold::with_clock(64, sink.clone(), clock.clone()));
    let registry = SessionRegistry::with_transcript_cap(4 * 1024 * 1024)
        .with_transcript_hold(Arc::clone(&hold) as Arc<dyn HeldBufferScan>);

    let cycle = "crt-052";
    let sid = "S";

    // The faithful per-turn lifecycle, driven ENTIRELY through the registry's
    // production entry points (register=readopt, apply_delta=route-to-held,
    // drain=hold_on_drain). >=3 drain cycles, deltas applied BETWEEN each drain.

    // --- turn 1: register(S, cycle=X) → deltas → drain #1 ---
    registry.register_session(sid, None, Some(cycle.to_string()));
    registry.apply_transcript_delta(sid, 0, b"TURN1 ");
    let (_o, purge1) = registry
        .drain_and_signal_session(sid, "success")
        .expect("drain #1");
    assert!(
        purge1.is_none(),
        "held buffer NOT purged at close (ADR-009)"
    );
    assert!(hold.is_held(sid), "buffer survived drain #1 (held)");

    // Inter-drain delta to the HELD buffer (not re-registered) — proves
    // merge-while-held (assertion f). Offset 6 continues the logical stream.
    clock.advance(Duration::from_secs(1));
    registry.apply_transcript_delta(sid, 6, b"HELD1 ");
    assert_eq!(
        held_snapshot_bytes(&hold, cycle, sid),
        b"TURN1 HELD1 ",
        "(f) inter-drain delta merged into the held buffer"
    );

    // --- turn 2: re-register (readopt rebinds) → deltas → drain #2 ---
    clock.advance(Duration::from_secs(1));
    registry.register_session(sid, None, Some(cycle.to_string()));
    assert!(
        !hold.is_held(sid),
        "readopt removed it from the hold (rebound to registry)"
    );
    registry.apply_transcript_delta(sid, 12, b"TURN2 ");
    let (_o, purge2) = registry
        .drain_and_signal_session(sid, "success")
        .expect("drain #2");
    assert!(purge2.is_none());
    assert!(hold.is_held(sid));

    // Inter-drain delta #2 to the held buffer.
    clock.advance(Duration::from_secs(1));
    registry.apply_transcript_delta(sid, 18, b"HELD2 ");

    // --- turn 3: re-register → deltas → drain #3 (>= 3 drains total) ---
    clock.advance(Duration::from_secs(1));
    registry.register_session(sid, None, Some(cycle.to_string()));
    registry.apply_transcript_delta(sid, 24, b"TURN3");
    let (_o, purge3) = registry
        .drain_and_signal_session(sid, "success")
        .expect("drain #3");
    assert!(purge3.is_none());
    assert!(hold.is_held(sid));

    // --- re-register(S, cycle=X) → readopt rebinds the live held buffer ---
    clock.advance(Duration::from_secs(1));
    registry.register_session(sid, None, Some(cycle.to_string()));
    assert!(
        !hold.is_held(sid),
        "(b) re-adopt on cycle MATCH removed it from the hold"
    );

    // --- context_cycle_review(X): snapshot → distill → purge ---
    let snaps = registry.take_transcripts_for_feature(cycle);
    assert_eq!(snaps.len(), 1, "the re-adopted session is snapshotted");
    let review_bytes = &snaps[0].1.bytes;

    // (a) CROSS-TURN content — all three turns present, not just the last.
    assert!(
        contains(review_bytes, b"TURN1")
            && contains(review_bytes, b"TURN2")
            && contains(review_bytes, b"TURN3"),
        "(a) review snapshot must carry content from ALL turns (merge-while-held)"
    );

    // (b) fail-loud on mismatch — a held buffer under cycle-X never re-adopts under cycle-Y.
    hold.hold_on_drain("MISMATCH", buf_with(b"X"), cycle);
    assert!(
        hold.readopt("MISMATCH", "cycle-OTHER").is_none(),
        "(b) mismatch fails loud"
    );
    assert!(!hold.is_held("MISMATCH"));
    assert_eq!(
        sink.count_for("MISMATCH"),
        1,
        "(b) mismatch drop audited once"
    );

    // (c) held-count bounded + observable eviction.
    let evict_clock = TestClock::new();
    let evict_sink = Arc::new(CapturingSink::default());
    let small = TranscriptHold::with_clock(2, evict_sink.clone(), evict_clock.clone());
    for i in 0..5 {
        small.hold_on_drain(&format!("e{i}"), buf_with(b"data"), cycle);
        evict_clock.advance(Duration::from_secs(1));
        assert!(small.held_count() <= 2, "(c) held-count within cap");
    }
    assert!(
        evict_sink
            .rows()
            .iter()
            .all(|(t, _, _)| *t == TRIGGER_CAP_EVICT),
        "(c) every eviction observable via audit"
    );
    assert_eq!(
        evict_sink.rows().len(),
        3,
        "(c) cap=2, 5 holds → 3 evictions"
    );

    // (d) stale sweep reclaims WITHOUT review.
    let sweep_clock = TestClock::new();
    let (sweep_hold, _s) = hold_with_clock(8, sweep_clock.clone());
    sweep_hold.hold_on_drain("never-reviewed", buf_with(b"orphan"), cycle);
    sweep_clock.advance(Duration::from_secs(100));
    let reclaimed = sweep_hold.sweep_expired(Duration::from_secs(50));
    assert_eq!(
        reclaimed.len(),
        1,
        "(d) TTL sweep reclaims independent of review"
    );

    // (e) audit EXACTLY ONCE per held session at the terminal purge — across the
    //     three drains above S was never cap-evicted or mismatched, so the
    //     cap/mismatch sink recorded NOTHING for S. S is now re-adopted back into
    //     the registry, so its single terminal purge fires via the registered
    //     buffer clear at cycle review (the held set is empty for S).
    assert_eq!(
        sink.count_for(sid),
        0,
        "(e) no per-turn audit across 3 drains"
    );
    let held_terminal = hold.purge_held_for_feature(cycle);
    assert!(
        !held_terminal.iter().any(|r| r.session_id == sid),
        "(e) S re-adopted → not purged from the hold"
    );
    let reg_records = registry.clear_transcripts_for_feature(cycle);
    let s_terminal = reg_records.iter().filter(|r| r.session_id == sid).count();
    assert_eq!(
        s_terminal, 1,
        "(e) exactly one terminal purge for S at review"
    );
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
