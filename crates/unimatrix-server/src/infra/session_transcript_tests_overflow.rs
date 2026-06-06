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
