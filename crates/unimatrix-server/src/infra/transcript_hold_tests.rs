//! C8 held-buffer store tests (crt-052 Wave B, ADR-008/ADR-009).
//!
//! Covers R-01 (loud re-adoption), R-02 (cap + independent TTL), R-03
//! (exactly-once audit), R-13 (purge-for-feature), R-16 (eviction never silent),
//! R-17 (O(1) keyed delta routing), R-19 (metadata-only Debug), and the AC-11
//! MERGE GATE `continuity_simulated_lifecycle` — the only pre-merge proof of the
//! primary path. All clock-dependent tests use an injectable clock; no sleeps,
//! no wall-clock — deterministic (test-plan invariant).

use std::sync::Mutex as StdMutex;
use std::time::Instant;

use super::*;
use crate::infra::session::{HeldBufferScan, SessionRegistry};

// -- Test doubles --------------------------------------------------------------

/// Capturing audit sink: records every `(trigger, session_id, bytes)` the hold
/// emits on cap-evict / readopt-mismatch so eviction is OBSERVABLE without a
/// tokio runtime (R-16). Sweep/review emissions are observed via the returned
/// `Vec<TranscriptPurgeRecord>` (the caller emits those).
#[derive(Default)]
struct CapturingSink {
    rows: StdMutex<Vec<(&'static str, String, u64)>>,
}

impl PurgeAuditSink for CapturingSink {
    fn emit(&self, records: Vec<TranscriptPurgeRecord>, trigger: &'static str) {
        let mut rows = self.rows.lock().unwrap_or_else(|p| p.into_inner());
        for r in records {
            rows.push((trigger, r.session_id, r.bytes_purged));
        }
    }
}

impl CapturingSink {
    fn rows(&self) -> Vec<(&'static str, String, u64)> {
        self.rows.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
    fn count_for(&self, session_id: &str) -> usize {
        self.rows()
            .iter()
            .filter(|(_, sid, _)| sid == session_id)
            .count()
    }
}

/// Controllable monotonic clock (no wall-clock, no sleeps).
struct TestClock {
    now: StdMutex<Instant>,
}
impl TestClock {
    fn new() -> Arc<Self> {
        Arc::new(TestClock {
            now: StdMutex::new(Instant::now()),
        })
    }
    fn advance(&self, by: Duration) {
        let mut g = self.now.lock().unwrap_or_else(|p| p.into_inner());
        *g += by;
    }
}
impl Clock for TestClock {
    fn now(&self) -> Instant {
        *self.now.lock().unwrap_or_else(|p| p.into_inner())
    }
}

fn buf_with(bytes: &[u8]) -> Arc<Mutex<TranscriptBuffer>> {
    let mut b = TranscriptBuffer::new(
        4 * 1024 * 1024,
        Arc::new(crate::infra::transcript_activity::SignatureScanner::empty()),
    );
    b.apply_delta(0, bytes);
    Arc::new(Mutex::new(b))
}

fn hold_with(max: usize) -> (Arc<TranscriptHold>, Arc<CapturingSink>) {
    let sink = Arc::new(CapturingSink::default());
    let hold = Arc::new(TranscriptHold::new(max, sink.clone()));
    (hold, sink)
}

fn hold_with_clock(max: usize, clock: Arc<dyn Clock>) -> (Arc<TranscriptHold>, Arc<CapturingSink>) {
    let sink = Arc::new(CapturingSink::default());
    let hold = Arc::new(TranscriptHold::with_clock(max, sink.clone(), clock));
    (hold, sink)
}

/// Snapshot a held buffer's contiguous bytes via the seam scan.
fn held_snapshot_bytes(hold: &TranscriptHold, feature_cycle: &str, session_id: &str) -> Vec<u8> {
    hold.held_arcs_for_feature(feature_cycle)
        .into_iter()
        .find(|(sid, _)| sid == session_id)
        .map(|(_, arc)| {
            arc.lock()
                .unwrap_or_else(|p| p.into_inner())
                .snapshot()
                .bytes
        })
        .unwrap_or_default()
}

// -- R-01: loud re-adoption ----------------------------------------------------

#[test]
fn test_readopt_cycle_match_rebinds() {
    let (hold, _sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"turn-1"), "cycle-X");
    let arc = hold.readopt("S", "cycle-X").expect("match re-adopts");
    let bytes = arc.lock().unwrap().snapshot().bytes;
    assert_eq!(bytes, b"turn-1", "re-adopted buffer carries S's bytes");
    assert!(!hold.is_held("S"), "re-adopted buffer leaves the hold");
}

#[test]
fn test_readopt_cycle_mismatch_fails_loud() {
    let (hold, sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"secret"), "cycle-X");
    let res = hold.readopt("S", "cycle-Y");
    assert!(
        res.is_none(),
        "mismatch must NOT re-adopt under the wrong cycle"
    );
    assert!(
        !hold.is_held("S"),
        "mismatched held buffer is DROPPED (treated fresh)"
    );
    // Fail-loud terminal purge audit fired exactly once.
    let rows = sink.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, TRIGGER_READOPT_MISMATCH);
    assert_eq!(rows[0].1, "S");
}

#[test]
fn test_readopt_null_cycle_no_silent_adopt() {
    // cite #981: an empty (NULL/mis-set) re-registering cycle must never silently
    // re-adopt the held buffer under a NULL cycle.
    let (hold, sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"data"), "cycle-X");
    let res = hold.readopt("S", "");
    assert!(res.is_none(), "empty cycle must not re-adopt");
    assert!(
        !hold.is_held("S"),
        "buffer dropped, not bound to a NULL cycle"
    );
    assert_eq!(sink.count_for("S"), 1, "fail-loud audit fires once");
}

#[test]
fn test_readopt_absent_session_returns_none() {
    let (hold, sink) = hold_with(8);
    assert!(hold.readopt("never-held", "cycle-X").is_none());
    assert!(sink.rows().is_empty(), "no audit for an absent session");
}

#[test]
fn test_readopt_mismatch_diagnostic_metadata_only() {
    // R-04 overlap: the fail-loud audit detail carries only the byte count.
    let (hold, sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"SENTINEL-leak-check"), "cycle-X");
    let _ = hold.readopt("S", "cycle-Y");
    let rows = sink.rows();
    assert_eq!(
        rows[0].2,
        b"SENTINEL-leak-check".len() as u64,
        "byte count, not content"
    );
    // The captured tuple never carries the buffer bytes themselves.
    assert!(!rows[0].1.contains("SENTINEL"));
}

// -- R-02: memory bound (cap + independent TTL) --------------------------------

#[test]
fn test_hold_cap_evicts_oldest() {
    let clock = TestClock::new();
    let (hold, sink) = hold_with_clock(2, clock.clone());
    hold.hold_on_drain("old", buf_with(b"oldest"), "cycle-X");
    clock.advance(Duration::from_secs(1));
    hold.hold_on_drain("mid", buf_with(b"middle"), "cycle-X");
    clock.advance(Duration::from_secs(1));
    // (cap+1)th insert evicts the oldest-last_activity victim ("old").
    hold.hold_on_drain("new", buf_with(b"newest"), "cycle-X");

    assert_eq!(hold.held_count(), 2, "held-count never exceeds the cap");
    assert!(!hold.is_held("old"), "oldest evicted first");
    assert!(hold.is_held("mid") && hold.is_held("new"));
    // R-16: the eviction emitted an audit (never silent).
    let rows = sink.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, TRIGGER_CAP_EVICT);
    assert_eq!(rows[0].1, "old");
}

#[test]
fn test_hold_at_exactly_cap_no_evict() {
    let (hold, sink) = hold_with(2);
    hold.hold_on_drain("a", buf_with(b"a"), "cycle-X");
    hold.hold_on_drain("b", buf_with(b"b"), "cycle-X");
    assert_eq!(hold.held_count(), 2, "exactly at cap → no eviction");
    assert!(sink.rows().is_empty());
    // cap+1 → exactly one eviction.
    hold.hold_on_drain("c", buf_with(b"c"), "cycle-X");
    assert_eq!(hold.held_count(), 2);
    assert_eq!(sink.rows().len(), 1, "cap+1 → exactly one eviction");
}

#[test]
fn test_hold_ttl_sweep_without_review() {
    let clock = TestClock::new();
    let (hold, _sink) = hold_with_clock(8, clock.clone());
    hold.hold_on_drain("S1", buf_with(b"aaa"), "cycle-X");
    hold.hold_on_drain("S2", buf_with(b"bbbb"), "cycle-X");
    clock.advance(Duration::from_secs(100));
    // No cycle review fires — TTL sweep alone reclaims.
    let records = hold.sweep_expired(Duration::from_secs(50));
    assert_eq!(
        records.len(),
        2,
        "both reclaimed by TTL, independent of review"
    );
    assert_eq!(hold.held_count(), 0);
}

#[test]
fn test_hold_ttl_boundary() {
    let clock = TestClock::new();
    let (hold, _sink) = hold_with_clock(8, clock.clone());
    hold.hold_on_drain("S", buf_with(b"data"), "cycle-X");
    clock.advance(Duration::from_secs(10));
    // Just under TTL → retained.
    assert!(hold.sweep_expired(Duration::from_secs(11)).is_empty());
    assert!(hold.is_held("S"));
    // Exactly at / over TTL → swept (>= comparison).
    let swept = hold.sweep_expired(Duration::from_secs(10));
    assert_eq!(swept.len(), 1);
    assert!(!hold.is_held("S"));
}

#[test]
fn test_hold_memory_bounded_no_review() {
    // Disable review entirely; cap + TTL alone keep held-count bounded under churn.
    let clock = TestClock::new();
    let cap = 4;
    let (hold, _sink) = hold_with_clock(cap, clock.clone());
    for i in 0..50 {
        hold.hold_on_drain(&format!("s{i}"), buf_with(b"0123456789"), "cycle-X");
        clock.advance(Duration::from_secs(1));
        assert!(hold.held_count() <= cap, "held-count must stay within cap");
    }
    // TTL sweep then reclaims the survivors — bounded without review.
    clock.advance(Duration::from_secs(10_000));
    hold.sweep_expired(Duration::from_secs(1));
    assert_eq!(hold.held_count(), 0);
}

// -- R-03 / R-16: audit exactly-once per held session --------------------------

#[test]
fn test_audit_once_terminal_via_sweep() {
    // crt-057: the review-purge is gone; the TTL sweep is the surviving terminal
    // reclamation. sweep_expired(ZERO) reclaims every held buffer at once.
    let (hold, sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"content"), "cycle-X");
    let records = hold.sweep_expired(Duration::ZERO);
    assert_eq!(records.len(), 1, "terminal sweep yields one record");
    assert_eq!(records[0].session_id, "S");
    // The cap-evict/mismatch sink fired NOTHING (sweep records go to the caller).
    assert!(sink.rows().is_empty(), "no extra per-turn audit emission");
    assert!(!hold.is_held("S"));
}

#[test]
fn test_audit_once_at_sweep() {
    let clock = TestClock::new();
    let (hold, _sink) = hold_with_clock(8, clock.clone());
    hold.hold_on_drain("S", buf_with(b"content"), "cycle-X");
    clock.advance(Duration::from_secs(100));
    let records = hold.sweep_expired(Duration::from_secs(10));
    assert_eq!(records.len(), 1, "one record at sweep");
    assert_eq!(records[0].session_id, "S");
}

#[test]
fn test_audit_once_at_eviction() {
    let (hold, sink) = hold_with(1);
    hold.hold_on_drain("a", buf_with(b"first"), "cycle-X");
    hold.hold_on_drain("b", buf_with(b"second"), "cycle-X"); // evicts "a"
    assert_eq!(sink.count_for("a"), 1, "eviction emits exactly one audit");
    assert_eq!(sink.rows()[0].0, TRIGGER_CAP_EVICT);
}

#[test]
fn test_audit_once_across_multi_readopt() {
    // drain→hold→re-adopt→drain→hold→review: STILL exactly one terminal audit.
    let (hold, sink) = hold_with(8);
    let buf = buf_with(b"r1");
    hold.hold_on_drain("S", buf.clone(), "cycle-X");
    let arc = hold.readopt("S", "cycle-X").expect("round 1 re-adopt");
    assert!(sink.rows().is_empty(), "re-adopt is not a purge");
    // round 2 drain → hold the same Arc again
    hold.hold_on_drain("S", arc, "cycle-X");
    let review = hold.sweep_expired(Duration::ZERO); // crt-057: sweep is the terminal path
    assert_eq!(review.len(), 1, "exactly one terminal purge across rounds");
    assert!(
        sink.rows().is_empty(),
        "no cap/mismatch audit across the rounds"
    );
}

#[test]
fn test_audit_detail_content_free() {
    // The record's bytes_purged is a count; the session_id never carries content.
    let (hold, _sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"SENTINEL-detail"), "cycle-X");
    let records = hold.sweep_expired(Duration::ZERO); // crt-057: sweep is the terminal path
    assert_eq!(records[0].bytes_purged, b"SENTINEL-detail".len() as u64);
    assert!(!records[0].session_id.contains("SENTINEL"));
}

#[test]
fn test_empty_held_buffer_purge_emits_nothing() {
    // A held buffer that was already cleared yields no purge record (zero-byte).
    let (hold, _sink) = hold_with(8);
    let empty = Arc::new(Mutex::new(TranscriptBuffer::new(
        4096,
        Arc::new(crate::infra::transcript_activity::SignatureScanner::empty()),
    )));
    hold.hold_on_drain("S", empty, "cycle-X");
    let records = hold.sweep_expired(Duration::ZERO); // crt-057: sweep is the terminal path
    assert!(records.is_empty(), "empty buffer emits no purge record");
}

// crt-057: `test_purge_held_for_feature_clears_held` (feature-scoped review purge)
// was DELETED — the review has no purge verb anymore (NG-6). Feature-scoped
// reclamation is gone; the surviving reclamation is TTL/cap-based and covered by
// the sweep/eviction tests above.

#[test]
fn test_no_held_survives_post_sweep() {
    let (hold, _sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"x"), "cycle-X");
    hold.sweep_expired(Duration::ZERO); // crt-057: the surviving terminal path
    assert!(hold.held_arcs_for_feature("cycle-X").is_empty());
}

// -- R-17: delta routing keyed, no linear scan --------------------------------

#[test]
fn test_held_buffer_keeps_merging_deltas() {
    // A held (drained, not re-registered) buffer keeps accepting deltas.
    let (hold, _sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"turn-1 "), "cycle-X");
    let arc = hold.held_arc_for_session("S").expect("held arc present");
    arc.lock().unwrap().apply_delta(7, b"turn-2");
    let bytes = held_snapshot_bytes(&hold, "cycle-X", "S");
    assert_eq!(bytes, b"turn-1 turn-2", "delta merged into the held buffer");
}

#[test]
fn test_held_arc_for_session_bumps_activity() {
    // Routing a delta bumps last_activity_at, deferring TTL sweep (R-17/R-02).
    let clock = TestClock::new();
    let (hold, _sink) = hold_with_clock(8, clock.clone());
    hold.hold_on_drain("S", buf_with(b"x"), "cycle-X");
    clock.advance(Duration::from_secs(8));
    let _ = hold.held_arc_for_session("S"); // activity bump at t=8
    clock.advance(Duration::from_secs(5)); // t=13, 5s since last activity
    assert!(
        hold.sweep_expired(Duration::from_secs(10)).is_empty(),
        "activity bump defers TTL sweep"
    );
    assert!(hold.is_held("S"));
}

// -- R-19: metadata-only Debug ------------------------------------------------

#[test]
fn test_heldbuffer_debug_metadata_only() {
    let (hold, _sink) = hold_with(8);
    hold.hold_on_drain("S", buf_with(b"SENTINEL-debug-leak"), "cycle-X");
    // HeldBuffer Debug is reached via the hold's map; assert no content leak by
    // formatting the whole store (Debug is metadata-only by construction).
    let dbg = format!("{hold:?}");
    assert!(
        !dbg.contains("SENTINEL"),
        "Debug must not carry buffer bytes"
    );
    assert!(dbg.contains("held_count"));
}

// -- Config floor: zero cap clamps to 1 (defense in depth) --------------------

#[test]
fn test_zero_cap_clamped_to_one() {
    let (hold, _sink) = hold_with(0);
    hold.hold_on_drain("a", buf_with(b"a"), "cycle-X");
    hold.hold_on_drain("b", buf_with(b"b"), "cycle-X"); // evicts a (cap clamped to 1)
    assert_eq!(hold.held_count(), 1);
}

// =============================================================================
// AC-11 MERGE GATE — `continuity_simulated_lifecycle`
// Lives in its own file to keep both test files under the 500-line rule (#693).
// It is the ONLY pre-merge proof of the primary path: a faithful per-turn-drain
// simulation (>=3 drain cycles, deltas applied BETWEEN each drain, re-register,
// cycle review). The child module reaches the helpers above via `super::`.
// =============================================================================
#[path = "transcript_hold_ac11_tests.rs"]
mod ac11;

// =============================================================================
// crt-054 Surface B believable-zero integration guards (AC-06 / AC-07).
// Held-route fold + read-before-purge, driven through the REAL registry+hold.
// Own file (500-line rule, #693); reaches the helpers above via `super::`.
// =============================================================================
#[path = "transcript_hold_activity_tests.rs"]
mod activity;
