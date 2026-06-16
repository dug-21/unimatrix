//! crt-054 Surface B believable-zero integration guards (AC-06 / AC-07).
//!
//! These are the NON-NEGOTIABLY integration tests the Stage 3b waves deferred to
//! Stage 3c (see `session_transcript_tests.rs` comment at the registered-route
//! fold). They drive the REAL `SessionRegistry` + crt-052 Wave B `TranscriptHold`
//! through the production entry points (register=readopt, apply_delta=route-to-
//! held, drain=hold_on_drain), exercising the fold on the HELD route — the
//! believable-zero seam (#750/#5025). A registered-only or unit-only test does
//! NOT satisfy AC-06/AC-07 (pattern #3624): a no-op held-route path would read 0
//! while passing a unit test, which is exactly the class of bug that slips
//! through.
//!
//! Test plans:
//! - product/features/crt-054/test-plan/apply-delta-fold.md (AC-06, held route)
//! - product/features/crt-054/test-plan/activity-snapshot.md (AC-07, read-before-purge)
//! - product/features/crt-054/test-plan/activity-collector.md (AC-06/AC-12 read seam)
//!
//! Child of `transcript_hold.rs::tests`; reuses the parent's `CapturingSink` /
//! `TestClock` and the registry+hold wiring pattern from `ac11` — no isolated
//! scaffolding (test infra is cumulative, CLAUDE.md).
//!
//! NEGATIVE-MUTATION CONTRACT (mandatory, ADR-009): the continuity test asserts
//! K+M where M bytes are streamed to the buffer AFTER drain (on the held route).
//! If the held-route `apply_delta` fold call were removed (or the held branch
//! bypassed the fold), the post-drain M bytes would not fold and the snapshot
//! would read `bytes_total == K` (not K+M) → the test fails RED. A test that
//! stayed green under that mutation would be invalid.

use super::{
    Arc, CapturingSink, Duration, HeldBufferScan, SessionRegistry, TestClock, TranscriptHold,
};
use crate::infra::transcript_activity::ActivitySnapshot;

/// Wire a `SessionRegistry` to a real `TranscriptHold` (the production pair) with
/// an injectable clock — the same construction the AC-11 merge gate uses.
fn registry_with_hold() -> (SessionRegistry, Arc<TranscriptHold>, Arc<TestClock>) {
    let clock = TestClock::new();
    let sink = Arc::new(CapturingSink::default());
    let hold = Arc::new(TranscriptHold::with_clock(64, sink, clock.clone()));
    let registry = SessionRegistry::with_transcript_cap(4 * 1024 * 1024)
        .with_transcript_hold(Arc::clone(&hold) as Arc<dyn HeldBufferScan>);
    (registry, hold, clock)
}

/// Read the activity snapshot for `session_id` within `feature_cycle` via the
/// crt-055 read seam (`activity_snapshots_for_feature`) — registered ∪ held.
fn snapshot_for(
    registry: &SessionRegistry,
    feature_cycle: &str,
    session_id: &str,
) -> Option<ActivitySnapshot> {
    registry
        .activity_snapshots_for_feature(feature_cycle)
        .into_iter()
        .find(|(sid, _)| sid == session_id)
        .map(|(_, snap)| snap)
}

// =============================================================================
// AC-06 — held-route fold (Critical, R-01)
// =============================================================================

/// AC-06 mandatory held-route regression guard: a representative TS-client cycle
/// (register → stream → DRAIN → stream more on the held Arc → review) yields a
/// NON-EMPTY activity snapshot at review, read via the held-aware collector
/// BEFORE purge. A registered-only path cannot satisfy this — the post-drain
/// bytes are routed through the held branch of `apply_transcript_delta`.
#[test]
fn test_held_route_fold_nonempty_at_review() {
    let (registry, hold, clock) = registry_with_hold();
    let cycle = "crt-054";
    let sid = "S";

    // Turn 1: register, stream deltas on the REGISTERED route, then drain
    // (Stop/SessionClose) — the buffer rides the Wave B hold.
    registry.register_session(sid, None, Some(cycle.to_string()));
    registry.apply_transcript_delta(sid, 0, b"TURN1 bytes ");
    let (_o, purge) = registry
        .drain_and_signal_session(sid, "success")
        .expect("drain");
    assert!(purge.is_none(), "held buffer NOT purged at close (ADR-009)");
    assert!(hold.is_held(sid), "buffer survived drain (held)");

    // Post-drain: stream MORE deltas. The session is no longer in the registry,
    // so these route through the HELD branch and MUST still fold (AC-06).
    clock.advance(Duration::from_secs(1));
    registry.apply_transcript_delta(sid, 12, b"HELD bytes after drain");

    // Review read (before purge), via the held-aware collector.
    let snap = snapshot_for(&registry, cycle, sid).expect("held session contributes an entry");
    assert!(
        snap.bytes_total > 0,
        "AC-06: held-route fold must yield non-zero bytes_total at review (believable-zero guard); \
         a held-route fold MISS reads 0 here"
    );
    assert!(
        snap.delta_count > 0,
        "AC-06: held-route fold must yield non-zero delta_count at review"
    );
}

/// AC-06 continuity + NEGATIVE-MUTATION guard (mandatory, ADR-009): K bytes on
/// the registered route, DRAIN, then M bytes on the held Arc. The snapshot at
/// review MUST read `bytes_total == K + M` and `delta_count == 2` — continuity
/// ACROSS the drain boundary into the SAME embedded accumulator, not two
/// isolated non-zero reads.
///
/// This is the negative-mutation guard: if the held-route fold call were removed,
/// the M post-drain bytes would not fold and this asserts `K` instead of `K+M`,
/// failing RED. A green result under that mutation would be invalid (#3624).
#[test]
fn test_held_route_fold_continuity_across_drain() {
    let (registry, hold, clock) = registry_with_hold();
    let cycle = "crt-054";
    let sid = "S";

    let pre = b"K-bytes-pre-drain-"; // K bytes, registered route
    let post = b"M-bytes-post-drain"; // M bytes, held route
    let k = pre.len() as u64;
    let m = post.len() as u64;

    registry.register_session(sid, None, Some(cycle.to_string()));
    registry.apply_transcript_delta(sid, 0, pre);

    // DRAIN — buffer goes to the hold; the SAME Arc/accumulator persists.
    let (_o, purge) = registry
        .drain_and_signal_session(sid, "success")
        .expect("drain");
    assert!(purge.is_none());
    assert!(hold.is_held(sid));

    // M bytes AFTER the drain, on the held route (offset continues the stream).
    clock.advance(Duration::from_secs(1));
    registry.apply_transcript_delta(sid, k, post);

    let snap = snapshot_for(&registry, cycle, sid).expect("held session present at review");
    assert_eq!(
        snap.bytes_total,
        k + m,
        "AC-06 continuity: snapshot must equal K+M across the drain boundary \
         (held-route fold MISS would read K={k}, not K+M={})",
        k + m
    );
    assert_eq!(
        snap.delta_count, 2,
        "AC-06 continuity: both the pre-drain and post-drain deltas count into the same accumulator"
    );
}

/// AC-06 / AC-12 (R-08): the collector includes a declared HELD session and
/// EXCLUDES an undeclared one (no fabricated zero). Exercises the read seam's
/// registered ∪ held reach plus attribution honesty in one drive.
#[test]
fn test_collector_includes_declared_held_excludes_undeclared() {
    let (registry, hold, _clock) = registry_with_hold();
    let cycle = "crt-054";

    // Declared session: folds bytes, then drains to the hold.
    registry.register_session("declared", None, Some(cycle.to_string()));
    registry.apply_transcript_delta("declared", 0, b"declared transcript bytes");
    let _ = registry
        .drain_and_signal_session("declared", "success")
        .expect("drain declared");
    assert!(hold.is_held("declared"));

    // Undeclared session (no feature_cycle): folds bytes, drains — its buffer
    // purges at drain (fold dies). It must contribute NO entry to ANY cycle.
    registry.register_session("undeclared", None, None);
    registry.apply_transcript_delta("undeclared", 0, b"undeclared bytes");
    let _ = registry
        .drain_and_signal_session("undeclared", "success")
        .expect("drain undeclared");

    let snaps = registry.activity_snapshots_for_feature(cycle);
    assert!(
        snaps
            .iter()
            .any(|(sid, snap)| sid == "declared" && snap.bytes_total > 0),
        "AC-06: the declared held session contributes a non-zero entry"
    );
    assert!(
        !snaps.iter().any(|(sid, _)| sid == "undeclared"),
        "AC-12: the undeclared session contributes NO entry — no fabricated zero"
    );
}

// =============================================================================
// AC-07 — read-before-purge ordering (Critical, R-02)
// =============================================================================

/// AC-07 read-before-purge ordering: collect a NON-ZERO snapshot first, THEN
/// purge the held buffer; a second collect returns no entry. This proves the
/// read provably happens BEFORE the purge drops the accumulator. A regression
/// that zeroed/dropped the accumulator before review would make the first read
/// empty → fails RED.
#[test]
fn test_read_before_purge_ordering() {
    let (registry, hold, clock) = registry_with_hold();
    let cycle = "crt-054";
    let sid = "S";

    // Drain→hold a session with non-trivial folded bytes.
    registry.register_session(sid, None, Some(cycle.to_string()));
    registry.apply_transcript_delta(sid, 0, b"bytes that must survive to review");
    let _ = registry
        .drain_and_signal_session(sid, "success")
        .expect("drain");
    clock.advance(Duration::from_secs(1));
    registry.apply_transcript_delta(sid, 33, b" plus held bytes");
    assert!(hold.is_held(sid));

    // READ first — non-zero (the crt-055 review read, before purge).
    let before = snapshot_for(&registry, cycle, sid).expect("entry present before purge");
    assert!(
        before.bytes_total > 0 && before.delta_count > 0,
        "AC-07: the review read returns non-zero counters BEFORE purge"
    );

    // PURGE second — `purge_cycle_transcripts` calls these for the held set
    // (server.rs:561). Dropping the held Arc removes the session from the seam.
    let purged = hold.purge_held_for_feature(cycle);
    assert!(
        purged.iter().any(|r| r.session_id == sid),
        "the held session was purged"
    );
    assert!(!hold.is_held(sid), "held buffer dropped after the read");

    // READ again — the buffer is gone; no entry (the read provably happened first).
    assert!(
        snapshot_for(&registry, cycle, sid).is_none(),
        "AC-07: after purge the session yields no entry — the read occurred BEFORE the purge"
    );
}

/// AC-07 survival across the full drain→hold→review lifecycle: the snapshot at
/// review equals the SUM of all folded deltas (no partial/reset), and the
/// crt-052 `clear()`-on-purge of a REGISTERED buffer preserves the accumulator
/// (ADR-006) — crt-054 never zeroes the accumulator on any path between fold and
/// review. (R-02 scenarios 2 + 3.)
#[test]
fn test_snapshot_survives_drain_hold_review() {
    let (registry, hold, clock) = registry_with_hold();
    let cycle = "crt-054";
    let sid = "S";

    // Multi-turn: register → delta → drain → held delta → re-register (readopt) →
    // delta → drain again → held delta. The accumulator folds every delta.
    let parts: [&[u8]; 4] = [b"AAAA", b"BBBB", b"CCCC", b"DDDD"];
    let expected: u64 = parts.iter().map(|p| p.len() as u64).sum();

    registry.register_session(sid, None, Some(cycle.to_string()));
    registry.apply_transcript_delta(sid, 0, parts[0]); // registered

    let _ = registry
        .drain_and_signal_session(sid, "success")
        .expect("drain 1");
    clock.advance(Duration::from_secs(1));
    registry.apply_transcript_delta(sid, 4, parts[1]); // held

    registry.register_session(sid, None, Some(cycle.to_string())); // readopt rebinds
    assert!(!hold.is_held(sid), "readopt rebound the live held buffer");
    registry.apply_transcript_delta(sid, 8, parts[2]); // registered again

    let _ = registry
        .drain_and_signal_session(sid, "success")
        .expect("drain 2");
    clock.advance(Duration::from_secs(1));
    registry.apply_transcript_delta(sid, 12, parts[3]); // held again

    // Review read at the end of the lifecycle — full sum, never partial/reset.
    let snap = snapshot_for(&registry, cycle, sid).expect("session present at review");
    assert_eq!(
        snap.bytes_total, expected,
        "AC-07: snapshot equals the SUM of all folded deltas across the full lifecycle"
    );
    assert_eq!(
        snap.delta_count, 4,
        "AC-07: every delta across registered+held routes counts once"
    );
}
