//! Overflow/ring-tail (R-03, AC-07 §3) and hole-metadata-bound (R-15 §4) tests
//! for `TranscriptBuffer`. Split from `session_transcript_tests.rs` (500-line
//! file cap); shares its harness via `use super::*`.
//!
//! Variance 1 (human-accepted): under overflow, AC-02 weakens to tail-window
//! equivalence — full-content equality is asserted ONLY below the cap (§1).
//! Do not strengthen these assertions.

use super::*;

// ----------------------------------- §3 overflow / ring-tail (R-03, AC-07) --

/// Cap-crossing delta sequence in multiple arrival orders: final
/// `contiguous_tail(window)` is byte-identical across orders. Full content is
/// explicitly NOT asserted (Variance 1 — tail-window equivalence only).
#[test]
fn test_overflow_reorder_tail_window_equivalence() {
    const CAP: usize = 256;
    let src = src_bytes(1000);
    // Ten contiguous 100-byte chunks covering [0, 1000) — 3.9x the cap.
    let base_set: Vec<(u64, usize)> = (0..10).map(|i| (i as u64 * 100, 100)).collect();
    // Reference: in-order arrival.
    let mut reference = TranscriptBuffer::new(CAP);
    apply_all(&mut reference, &src, &base_set);
    let ref_full = reference.contiguous_tail(CAP).expect("non-empty");
    let ref_sub = reference.contiguous_tail(100).expect("non-empty");
    assert_eq!(ref_full, &src[1000 - CAP..], "newest cap-window retained");

    let mut rng = StdRng::seed_from_u64(0x5203);
    for _ in 0..32 {
        let mut deltas = base_set.clone();
        deltas.shuffle(&mut rng);
        let mut buf = TranscriptBuffer::new(CAP);
        apply_all(&mut buf, &src, &deltas);
        assert_eq!(
            buf.contiguous_tail(CAP),
            Some(ref_full.clone()),
            "tail-window equivalence across arrival orders (cap window)"
        );
        assert_eq!(
            buf.contiguous_tail(100),
            Some(ref_sub.clone()),
            "tail-window equivalence across arrival orders (sub-window)"
        );
        assert_eq!(buf.high_water(), 1000, "high_water identical across orders");
    }
}

/// Drive 3x the cap; `len() <= max_bytes` after every apply; final tail equals
/// the programmatically-derived newest bytes.
#[test]
fn test_overflow_size_never_exceeds_cap() {
    const CAP: usize = 512;
    let total = 3 * CAP;
    let src = src_bytes(total);
    let mut buf = TranscriptBuffer::new(CAP);
    for (offset, len) in (0..total / 64).map(|i| (i as u64 * 64, 64usize)) {
        buf.apply_delta(offset, &src[offset as usize..offset as usize + len]);
        assert!(buf.len() <= CAP, "I1 holds after every apply");
    }
    assert_eq!(
        buf.contiguous_tail(CAP).as_deref(),
        Some(&src[total - CAP..]),
        "tail equals programmatically-derived newest bytes"
    );
    assert_eq!(buf.elided_bytes(), (total - CAP) as u64);
}

/// Elision is metadata only (AC-07, ADR-002): after ring-tail + a hole, the
/// returned tail contains only source-string bytes — no spliced marker, no
/// zero-fill.
#[test]
fn test_overflow_no_marker_bytes_in_content() {
    const CAP: usize = 200;
    let src = src_bytes(600); // src_bytes is never zero
    let mut buf = TranscriptBuffer::new(CAP);
    buf.apply_delta(0, &src[0..150]);
    buf.apply_delta(300, &src[300..450]); // ring-tails head, leaves hole (250,300)
    buf.apply_delta(500, &src[500..600]); // ring-tails again, hole (450,500)
    let tail = buf.contiguous_tail(CAP).expect("non-empty");
    assert_eq!(tail, &src[500..600], "exactly the post-hole source bytes");
    assert!(!tail.contains(&0), "no zero-fill or marker byte in content");
    assert!(
        buf.elided_bytes() > 0,
        "elision happened — recorded as metadata"
    );
}

/// `high_water` is monotonic across overflow, including when the clipping
/// delta itself carries the new maximum.
#[test]
fn test_high_water_monotonic_across_overflow() {
    const CAP: usize = 100;
    let src = src_bytes(256);
    let mut buf = TranscriptBuffer::new(CAP);
    buf.apply_delta(0, &src[0..100]);
    assert_eq!(buf.high_water(), 100);
    buf.apply_delta(80, &src[80..180]); // crosses cap: base advances to 80
    assert_eq!(buf.high_water(), 180);
    assert_eq!(buf.base_offset_for_test(), 80);
    // The clipping delta itself carries the new maximum: offset 60 < base,
    // end 190 > prior high_water.
    buf.apply_delta(60, &src[60..190]);
    assert_eq!(
        buf.high_water(),
        190,
        "clipped delta still raises high_water"
    );
    // Below-floor delta: high_water unchanged (its end is below the max).
    buf.apply_delta(0, &src[0..10]);
    assert_eq!(buf.high_water(), 190, "monotonic — never reduced");
}

/// Hand-computable fixture: elided = clipped-below-base + ring-dropped
/// *received* bytes; hole zero-fill is never double-counted when a hole drops
/// below base (R-03.4).
#[test]
fn test_elided_bytes_accounting_exact() {
    // Partial base advance across a straddling hole.
    let src = src_bytes(256);
    let mut buf = TranscriptBuffer::new(100);
    buf.apply_delta(0, &src[0..40]);
    buf.apply_delta(60, &src[60..100]); // hole (40,60); span [0,100)
    assert_eq!(buf.elided_bytes(), 0);
    // end = 160 → required_base = 60: drops [0,60) = 60 span bytes, of which
    // 20 are the hole's zero-fill (never received) → elided += 40, not 60.
    buf.apply_delta(110, &src[110..160]);
    assert_eq!(buf.elided_bytes(), 40, "hole bytes not counted as elided");
    assert_eq!(buf.base_offset_for_test(), 60);

    // Whole-span drop with a hole inside, then a below-base clip.
    let mut buf2 = TranscriptBuffer::new(200);
    buf2.apply_delta(0, &src[0..50]);
    buf2.apply_delta(100, &src[100..150]); // hole (50,100); span [0,150)
    buf2.apply_delta(300, &[7u8; 100]); // required_base 200 >= span_end 150
    assert_eq!(
        buf2.elided_bytes(),
        100,
        "whole-span drop counts received 100, not span 150"
    );
    buf2.apply_delta(0, &src[0..30]); // entirely below floor
    assert_eq!(
        buf2.elided_bytes(),
        130,
        "below-base clip adds full delta len"
    );
}

/// Cap exactly equal to one delta's size, and off-by-one at the cap boundary.
#[test]
fn test_cap_exactly_equal_to_delta_size() {
    let src = src_bytes(200);
    // Exact fit: no elision, base stays 0.
    let mut exact = TranscriptBuffer::new(100);
    exact.apply_delta(0, &src[0..100]);
    assert_eq!(exact.len(), 100);
    assert_eq!(exact.elided_bytes(), 0);
    assert_eq!(exact.base_offset_for_test(), 0);
    assert_eq!(exact.contiguous_tail(100).as_deref(), Some(&src[0..100]));
    // Off-by-one: cap one byte short — oldest byte ring-tailed.
    let mut short = TranscriptBuffer::new(99);
    short.apply_delta(0, &src[0..100]);
    assert_eq!(short.len(), 99);
    assert_eq!(short.elided_bytes(), 1);
    assert_eq!(short.base_offset_for_test(), 1);
    assert_eq!(short.contiguous_tail(99).as_deref(), Some(&src[1..100]));
    // Delta landing exactly at the cap boundary: full replacement.
    let mut buf = TranscriptBuffer::new(100);
    buf.apply_delta(0, &src[0..100]);
    buf.apply_delta(100, &src[100..200]);
    assert_eq!(buf.len(), 100);
    assert_eq!(buf.base_offset_for_test(), 100);
    assert_eq!(buf.elided_bytes(), 100);
    assert_eq!(buf.contiguous_tail(100).as_deref(), Some(&src[100..200]));
}

#[test]
fn test_contiguous_tail_window_larger_than_len() {
    let src = src_bytes(50);
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(0, &src);
    assert_eq!(buf.contiguous_tail(1000).as_deref(), Some(&src[..]));
}

#[test]
fn test_contiguous_tail_window_zero() {
    let src = src_bytes(50);
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(0, &src);
    assert_eq!(buf.contiguous_tail(0), None, "window 0 pinned as None");
}

#[test]
fn test_contiguous_tail_window_on_hole_boundary() {
    let src = src_bytes(300);
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(0, &src[0..100]);
    buf.apply_delta(150, &src[150..300]); // hole (100,150); post-hole avail = 150
    // Window exactly equal to the post-hole run.
    assert_eq!(buf.contiguous_tail(150).as_deref(), Some(&src[150..300]));
    // One past the boundary: still truncated at the hole — never crosses.
    assert_eq!(buf.contiguous_tail(151).as_deref(), Some(&src[150..300]));
    // Sub-window inside the post-hole run.
    assert_eq!(buf.contiguous_tail(100).as_deref(), Some(&src[200..300]));
}

// --------------------------------------- §4 hole-metadata bound (R-15) --

/// Drive 64 disjoint holes, then apply the 65th-hole-creating delta:
/// collapse-to-newest, abandoned span counted in `elided_bytes`, no panic,
/// hole count bounded.
#[test]
fn test_hole_collapse_at_cap() {
    let src = src_bytes(2048);
    let mut buf = TranscriptBuffer::new(4096);
    // 65 chunks of 10 bytes at stride 20 → 64 inter-chunk holes.
    for i in 0..65u64 {
        let offset = i * 20;
        buf.apply_delta(offset, &src[offset as usize..offset as usize + 10]);
    }
    assert_eq!(buf.holes_for_test().len(), 64, "at the metadata cap");
    assert_eq!(buf.elided_bytes(), 0);
    // 65th hole: span_end = 64*20 + 10 = 1290; write at 1300 pushes (1290,1300)
    // → collapse to the newest contiguous segment [1300, 1310).
    buf.apply_delta(1300, &src[1300..1310]);
    assert!(buf.holes_for_test().is_empty(), "collapsed to zero holes");
    assert_eq!(buf.base_offset_for_test(), 1300);
    assert_eq!(buf.len(), 10);
    // Abandoned span: 65 received chunks * 10 bytes = 650; hole zero-fill not counted.
    assert_eq!(buf.elided_bytes(), 650, "received bytes only");
    assert_eq!(buf.contiguous_tail(100).as_deref(), Some(&src[1300..1310]));
    assert_eq!(buf.high_water(), 1310);
}

/// Merges after a collapse land correctly; `contiguous_tail` serves the
/// newest segment; pre-collapse offsets are defined no-ops.
#[test]
fn test_post_collapse_merge_and_tail_correct() {
    let src = src_bytes(2048);
    let mut buf = TranscriptBuffer::new(4096);
    for i in 0..65u64 {
        let offset = i * 20;
        buf.apply_delta(offset, &src[offset as usize..offset as usize + 10]);
    }
    buf.apply_delta(1300, &src[1300..1310]); // triggers collapse, base = 1300
    let elided_after_collapse = buf.elided_bytes();
    // Contiguous extension merges cleanly.
    buf.apply_delta(1310, &src[1310..1400]);
    assert!(buf.holes_for_test().is_empty());
    assert_eq!(buf.contiguous_tail(100).as_deref(), Some(&src[1300..1400]));
    // A late delta into the abandoned (pre-collapse) region is clipped + counted.
    buf.apply_delta(500, &src[500..540]);
    assert_eq!(buf.len(), 100, "content unchanged");
    assert_eq!(buf.elided_bytes(), elided_after_collapse + 40);
    assert_eq!(buf.contiguous_tail(100).as_deref(), Some(&src[1300..1400]));
}

// ----------------------- §5 snapshot() under overflow / poison (crt-052 C2) --
// R-12 (logical base_offset), R-09 (cap boundary), R-16 (poison recovery).

/// Under ring-tail overflow the snapshot's `base_offset` advances and
/// `elided_bytes > 0` (R-12 feeds C3's logical-offset arithmetic). Variance 1:
/// tail-window equivalence only — do NOT assert full content.
#[test]
fn test_snapshot_base_offset_advances_under_overflow() {
    const CAP: usize = 256;
    let src = src_bytes(1000);
    // Ten contiguous 100-byte chunks covering [0,1000) — 3.9x the cap.
    let deltas: Vec<(u64, usize)> = (0..10).map(|i| (i as u64 * 100, 100)).collect();
    let mut buf = TranscriptBuffer::new(CAP);
    apply_all(&mut buf, &src, &deltas);
    let snap = buf.snapshot();
    assert!(snap.base_offset > 0, "ring-tail advanced the logical floor");
    assert!(snap.elided_bytes > 0, "head content elided");
    assert_eq!(
        snap.base_offset,
        1000 - CAP as u64,
        "logical floor = end - cap"
    );
    assert_eq!(snap.bytes.len(), CAP, "newest cap window retained");
}

/// `high_water` survives clipping (monotone, tracks sent-not-retained).
#[test]
fn test_snapshot_high_water_survives_clipping() {
    const CAP: usize = 100;
    let src = src_bytes(256);
    let mut buf = TranscriptBuffer::new(CAP);
    buf.apply_delta(0, &src[0..100]);
    buf.apply_delta(80, &src[80..180]); // clips, base advances
    let snap = buf.snapshot();
    assert_eq!(
        snap.high_water, 180,
        "high_water = max(offset+len) ever sent"
    );
    assert!(snap.base_offset > 0, "clipping occurred");
}

/// Holes surface in the snapshot as a sorted, disjoint, in-span `Vec<HoleInfo>`.
#[test]
fn test_snapshot_holes_reported() {
    let src = src_bytes(500);
    let mut buf = TranscriptBuffer::new(4096);
    apply_all(&mut buf, &src, &[(0, 100), (200, 100), (400, 100)]);
    let snap = buf.snapshot();
    assert_eq!(
        snap.holes,
        vec![
            HoleInfo {
                start: 100,
                end: 200
            },
            HoleInfo {
                start: 300,
                end: 400
            },
        ]
    );
    // Disjoint, sorted ascending, strictly inside the span, bounded.
    assert!(snap.holes.len() <= 64, "bounded by MAX_HOLE_RANGES");
    for w in snap.holes.windows(2) {
        assert!(w[0].end <= w[1].start, "sorted + disjoint");
    }
    let span_end = snap.base_offset + buf.len() as u64;
    for h in &snap.holes {
        assert!(h.start >= snap.base_offset && h.end <= span_end, "in span");
        assert!(h.start < h.end, "non-empty hole");
    }
}

/// At exactly the cap, then one byte over: the ring-tail-just-engaged transition
/// surfaces `base_offset` advance / `elided_bytes > 0` (SR-08 calibration edge).
#[test]
fn test_snapshot_at_exactly_cap_boundary() {
    const CAP: usize = 256;
    let src = src_bytes(CAP + 1);
    // Exactly at cap: no elision, base stays 0.
    let mut at_cap = TranscriptBuffer::new(CAP);
    at_cap.apply_delta(0, &src[0..CAP]);
    let snap = at_cap.snapshot();
    assert_eq!(snap.base_offset, 0, "at cap: no advance");
    assert_eq!(snap.elided_bytes, 0, "at cap: nothing elided");
    assert_eq!(snap.bytes.len(), CAP);
    // One byte over: ring-tail engages.
    let mut over = TranscriptBuffer::new(CAP);
    over.apply_delta(0, &src[0..CAP + 1]);
    let snap = over.snapshot();
    assert_eq!(snap.base_offset, 1, "one over: floor advanced by one");
    assert_eq!(snap.elided_bytes, 1, "one byte elided");
    assert_eq!(snap.bytes.len(), CAP);
}

/// 4 MiB byte copy completes well within the latency class (AC-12); the copy is
/// bounded by the 4 MiB cap. No parse, no marker match in the body (AC-01).
#[test]
fn test_snapshot_4mib_copy_fast() {
    const CAP: usize = 4 * 1024 * 1024;
    let src = src_bytes(CAP);
    let mut buf = TranscriptBuffer::new(CAP);
    buf.apply_delta(0, &src);
    let started = std::time::Instant::now();
    let snap = buf.snapshot();
    let elapsed = started.elapsed();
    assert_eq!(snap.bytes.len(), CAP, "full 4 MiB span copied");
    assert!(
        elapsed < std::time::Duration::from_millis(50),
        "byte copy within latency class, took {elapsed:?}"
    );
}

/// Poison recovery (R-16, #4764): the seam takes the buffer lock with
/// `unwrap_or_else(|p| p.into_inner())`, treats the recovered buffer as empty,
/// and calls `clear_poison()` so recovery happens exactly once (#4748). The
/// snapshot of the treat-as-empty buffer surfaces as empty/lossy — the loss is
/// visible downstream, not silently absent. This mirrors C1's lock acquisition.
#[test]
fn test_snapshot_poisoned_lock_treats_as_empty() {
    use std::sync::{Arc, Mutex};

    let src = src_bytes(300);
    let mut seeded = TranscriptBuffer::new(4096);
    seeded.apply_delta(0, &src);
    let lock = Arc::new(Mutex::new(seeded));

    // Poison the mutex by panicking while the guard is held.
    let poison_lock = Arc::clone(&lock);
    let _ = std::thread::spawn(move || {
        let _guard = poison_lock.lock().expect("first lock");
        panic!("poison the transcript mutex");
    })
    .join();
    assert!(lock.is_poisoned(), "mutex is poisoned");

    // Seam-style recovery: into_inner + treat-as-empty + clear_poison.
    let snapshot = {
        let mut guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        // Treat-as-empty: drop content but keep truthful metadata where readable.
        let _purged = guard.clear();
        let snap = guard.snapshot();
        lock.clear_poison();
        snap
    };
    assert!(
        snapshot.bytes.is_empty(),
        "treat-as-empty: no content surfaced"
    );

    // Recovery happened exactly once: a subsequent write accumulates (#4748 —
    // without clear_poison every later lock would re-clear and lose the write).
    assert!(!lock.is_poisoned(), "poison cleared");
    {
        let mut guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        let hw = guard.high_water();
        guard.apply_delta(hw, &src[0..50]);
    }
    let after = {
        let guard = lock.lock().unwrap_or_else(|p| p.into_inner());
        guard.snapshot()
    };
    assert_eq!(
        after.bytes.len(),
        50,
        "post-recovery write survived (#4748)"
    );
}

/// Pathological sparse stream: alternating far offsets for ~10k deltas stays
/// memory-bounded (len <= cap, holes <= 64) and completes in sane wall time.
/// The bounded-metadata property is the assertion; the 64 constant is tunable.
#[test]
fn test_pathological_sparse_stream_bounded() {
    const CAP: usize = 64 * 1024;
    let payload = [9u8; 16];
    let mut buf = TranscriptBuffer::new(CAP);
    let started = std::time::Instant::now();
    for i in 0..10_000u64 {
        // Alternate between a far-forward stride and a jittered lower offset
        // so writes keep creating, filling, and splitting holes.
        let offset = if i % 2 == 0 {
            i * 1_000
        } else {
            i * 1_000 - 750 + (i % 13) * 17
        };
        buf.apply_delta(offset, &payload);
        assert!(buf.len() <= CAP, "I1: len <= max_bytes");
        assert!(
            buf.holes_for_test().len() <= MAX_HOLE_RANGES,
            "I2: bounded holes"
        );
    }
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "bounded metadata keeps the sparse stream cheap"
    );
}
