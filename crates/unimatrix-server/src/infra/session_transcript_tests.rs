//! Tests for `TranscriptBuffer` (vnc-025): merge correctness (R-01, AC-02 §1),
//! arithmetic soundness (R-02, NFR-09 §2), content opacity (R-05.1, AC-12 §5),
//! and the `session_key` seam (ADR-007). Overflow/ring-tail (R-03 §3) and
//! hole-metadata-bound (R-15 §4) cases live in
//! `session_transcript_tests_overflow.rs` (500-line file cap).
//!
//! Shared harness: delta sets are `(offset, len)` pairs sliced from a source
//! byte string at their true offsets; expected content is derived
//! programmatically from the covered-range union (#2984), never hand-copied.

use super::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

#[path = "session_transcript_tests_overflow.rs"]
mod overflow;

#[path = "session_transcript_tests_snapshot.rs"]
mod snapshot;

// ---------------------------------------------------------------- harness --

/// Deterministic source bytes; never zero. Zero is the hole fill, so any zero
/// byte appearing in a returned tail is a leak of unwritten fill (FR-19).
fn src_bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i % 251 + 1) as u8).collect()
}

/// Apply `(offset, len)` deltas sliced from `src` at their true offsets.
fn apply_all(buf: &mut TranscriptBuffer, src: &[u8], deltas: &[(u64, usize)]) {
    for &(offset, len) in deltas {
        let start = offset as usize;
        buf.apply_delta(offset, &src[start..start + len]);
    }
}

/// Programmatic covered-range union of a delta set (#2984): sorted, merged.
fn covered_union(deltas: &[(u64, usize)]) -> Vec<(u64, u64)> {
    let mut ranges: Vec<(u64, u64)> = deltas.iter().map(|&(o, l)| (o, o + l as u64)).collect();
    ranges.sort_unstable();
    let mut merged: Vec<(u64, u64)> = Vec::new();
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// Fixed covering set over `[0, 1000)`: exact duplicate (`(0,300)` twice),
/// partial overlaps, out-of-order coverage (R-01.1).
fn covering_delta_set() -> Vec<(u64, usize)> {
    vec![
        (0, 300),
        (250, 200),
        (450, 150),
        (600, 200),
        (800, 200),
        (100, 100),
        (0, 300),
        (700, 150),
    ]
}

/// Span `[0,100) ∪ [200,300)` with one hole `(100,200)`.
fn buf_with_one_hole(src: &[u8]) -> TranscriptBuffer {
    let mut buf = TranscriptBuffer::new(4096);
    apply_all(&mut buf, src, &[(0, 100), (200, 100)]);
    assert_eq!(buf.holes_for_test(), &[(100, 200)]);
    buf
}

// ------------------------------------------- §1 merge correctness (R-01) --

#[test]
fn test_apply_delta_permutation_convergence_below_cap() {
    let src = src_bytes(1000);
    let base_set = covering_delta_set();
    let union = covered_union(&base_set);
    assert_eq!(union, vec![(0, 1000)], "fixture must fully cover [0,1000)");
    let expected_hw = union[0].1;
    let mut rng = StdRng::seed_from_u64(0x025);
    for _ in 0..32 {
        let mut deltas = base_set.clone();
        deltas.shuffle(&mut rng);
        let mut buf = TranscriptBuffer::new(DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES);
        apply_all(&mut buf, &src, &deltas);
        assert_eq!(buf.len(), 1000, "identical len across orders");
        assert_eq!(buf.high_water(), expected_hw, "identical high_water");
        assert_eq!(buf.elided_bytes(), 0, "below cap: nothing elided");
        assert!(
            buf.holes_for_test().is_empty(),
            "covering set leaves no hole"
        );
        assert_eq!(
            buf.contiguous_tail(buf.len()).as_deref(),
            Some(&src[..]),
            "identical full content across orders"
        );
    }
}

#[test]
fn test_apply_delta_fills_hole_exactly() {
    let src = src_bytes(300);
    let mut buf = buf_with_one_hole(&src);
    buf.apply_delta(100, &src[100..200]);
    assert!(buf.holes_for_test().is_empty());
    assert_eq!(buf.contiguous_tail(300).as_deref(), Some(&src[..]));
}

#[test]
fn test_apply_delta_shrinks_hole_from_start() {
    let src = src_bytes(300);
    let mut buf = buf_with_one_hole(&src);
    buf.apply_delta(100, &src[100..140]);
    assert_eq!(buf.holes_for_test(), &[(140, 200)]);
    assert_eq!(buf.contiguous_tail(300).as_deref(), Some(&src[200..300]));
    buf.apply_delta(140, &src[140..200]); // written range becomes readable
    assert!(buf.holes_for_test().is_empty());
    assert_eq!(buf.contiguous_tail(300).as_deref(), Some(&src[..]));
}

#[test]
fn test_apply_delta_shrinks_hole_from_end() {
    let src = src_bytes(300);
    let mut buf = buf_with_one_hole(&src);
    buf.apply_delta(160, &src[160..200]);
    assert_eq!(buf.holes_for_test(), &[(100, 160)]);
    // tail boundary moved back to the shrunken hole's end
    assert_eq!(buf.contiguous_tail(300).as_deref(), Some(&src[160..300]));
}

#[test]
fn test_apply_delta_splits_hole_in_two() {
    let src = src_bytes(300);
    let mut buf = buf_with_one_hole(&src);
    buf.apply_delta(130, &src[130..170]);
    assert_eq!(buf.holes_for_test(), &[(100, 130), (170, 200)]);
    assert_eq!(buf.contiguous_tail(300).as_deref(), Some(&src[200..300]));
    buf.apply_delta(100, &src[100..130]);
    buf.apply_delta(170, &src[170..200]);
    assert!(buf.holes_for_test().is_empty());
    assert_eq!(buf.contiguous_tail(300).as_deref(), Some(&src[..]));
}

#[test]
fn test_apply_delta_spans_multiple_holes() {
    let src = src_bytes(500);
    let mut buf = TranscriptBuffer::new(4096);
    apply_all(&mut buf, &src, &[(0, 100), (200, 100), (400, 100)]);
    assert_eq!(buf.holes_for_test(), &[(100, 200), (300, 400)]);
    // one write spanning both holes: shrinks the first from the end, removes the second
    buf.apply_delta(150, &src[150..450]);
    assert_eq!(buf.holes_for_test(), &[(100, 150)]);
    assert_eq!(buf.contiguous_tail(500).as_deref(), Some(&src[150..500]));
}

#[test]
fn test_apply_delta_below_base_after_ring_tail_is_noop() {
    let src = src_bytes(200);
    let mut buf = TranscriptBuffer::new(100);
    buf.apply_delta(0, &src[0..100]);
    buf.apply_delta(100, &src[100..200]); // ring-tail: base advances to 100
    assert_eq!(buf.base_offset_for_test(), 100);
    assert_eq!(buf.elided_bytes(), 100);
    buf.apply_delta(0, &src[0..50]); // entirely below floor
    assert_eq!(buf.len(), 100, "content unchanged");
    assert_eq!(buf.contiguous_tail(100).as_deref(), Some(&src[100..200]));
    assert_eq!(buf.elided_bytes(), 150, "clipped bytes counted");
    assert_eq!(buf.high_water(), 200, "high_water still updated (FR-02)");
}

#[test]
fn test_apply_delta_beyond_span_creates_hole_tail_never_crosses() {
    let src = src_bytes(200);
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(0, &src[0..100]);
    buf.apply_delta(150, &src[150..200]); // gap [100,150)
    assert_eq!(buf.holes_for_test(), &[(100, 150)]);
    let tail = buf.contiguous_tail(200).expect("non-empty");
    assert_eq!(tail, &src[150..200], "only post-hole bytes");
    assert!(
        !tail.contains(&0),
        "no zero-fill byte in any returned tail (FR-19)"
    );
}

#[test]
fn test_apply_delta_zero_length_bytes_noop_high_water_defined() {
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(500, &[]);
    assert_eq!(buf.high_water(), 500, "only high_water moves");
    assert_eq!(buf.len(), 0);
    assert_eq!(buf.elided_bytes(), 0);
    assert_eq!(buf.base_offset_for_test(), 0);
    assert_eq!(buf.contiguous_tail(100), None);
}

#[test]
fn test_apply_delta_offset_zero_empty_buffer_then_exact_duplicate() {
    let src = src_bytes(100);
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(0, &src);
    buf.apply_delta(0, &src); // exact duplicate: in-place rewrite
    assert_eq!(buf.len(), 100);
    assert_eq!(buf.high_water(), 100);
    assert_eq!(buf.elided_bytes(), 0);
    assert!(buf.holes_for_test().is_empty());
    assert_eq!(buf.contiguous_tail(100).as_deref(), Some(&src[..]));
}

#[test]
fn test_apply_delta_invalid_utf8_bytes_accepted() {
    // API is `&[u8]`; crt-052 reads raw bytes — invalid UTF-8 must round-trip.
    let bytes: Vec<u8> = vec![0xFF, 0xFE, 0xC0, 0x80, 0x9F, 0xF5, 0x01];
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(0, &bytes);
    assert_eq!(
        buf.contiguous_tail(bytes.len()).as_deref(),
        Some(&bytes[..])
    );
}

// --------------------------------- §2 arithmetic soundness (R-02, NFR-09) --

#[test]
fn test_apply_delta_near_u64_max_drops_whole() {
    let src = src_bytes(100);
    let mut buf = TranscriptBuffer::new(64 * 1024);
    buf.apply_delta(0, &src);
    buf.apply_delta(u64::MAX - 10, &[0u8; 100]); // end overflows u64
    // ADR-008 drop-whole: NO state change at all — do NOT assert partial clip.
    assert_eq!(buf.len(), 100);
    assert_eq!(buf.high_water(), 100, "not even high_water");
    assert_eq!(buf.elided_bytes(), 0);
    assert_eq!(buf.base_offset_for_test(), 0);
    assert!(buf.holes_for_test().is_empty());
}

#[test]
fn test_apply_delta_far_offset_jump_allocation_bounded() {
    let src = src_bytes(600);
    let cap = 1000usize;
    let mut buf = TranscriptBuffer::new(cap);
    buf.apply_delta(0, &src[0..500]);
    let far = 1u64 << 40;
    buf.apply_delta(far, &src[500..600]);
    let end = far + 100;
    assert!(buf.len() <= cap, "allocation stays <= cap");
    assert_eq!(
        buf.base_offset_for_test(),
        end - cap as u64,
        "base advances"
    );
    assert_eq!(
        buf.elided_bytes(),
        500,
        "prior received bytes counted as elided"
    );
    assert_eq!(buf.high_water(), end);
    assert_eq!(buf.contiguous_tail(cap).as_deref(), Some(&src[500..600]));
}

#[test]
fn test_apply_delta_one_mib_into_4mib_cap() {
    const MIB: usize = 1 << 20;
    let src = src_bytes(MIB);
    let mut buf = TranscriptBuffer::new(DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES);
    buf.apply_delta(0, &src); // frame-ceiling delta (FR-05)
    assert_eq!(buf.len(), MIB);
    assert_eq!(buf.elided_bytes(), 0);
    assert_eq!(buf.contiguous_tail(MIB).as_deref(), Some(&src[..]));
}

#[test]
fn test_apply_delta_one_mib_into_64kib_cap() {
    const MIB: usize = 1 << 20;
    const CAP: usize = 64 * 1024;
    let src = src_bytes(MIB);
    let mut buf = TranscriptBuffer::new(CAP);
    buf.apply_delta(0, &src); // ring-tails: only the newest CAP bytes retained
    assert_eq!(buf.len(), CAP);
    assert_eq!(buf.elided_bytes(), (MIB - CAP) as u64);
    assert_eq!(buf.base_offset_for_test(), (MIB - CAP) as u64);
    assert_eq!(buf.contiguous_tail(CAP).as_deref(), Some(&src[MIB - CAP..]));
}

/// Named NFR-09 verification: randomized (offset, len) incl. near-`u64::MAX`
/// band; assert only no panic, len <= cap, high_water monotonic.
#[test]
fn test_apply_delta_fuzz_no_panic() {
    const MIB: usize = 1 << 20;
    let src = src_bytes(MIB);
    for &cap in &[64 * 1024usize, 4 * 1024 * 1024] {
        let mut rng = StdRng::seed_from_u64(0xA55_069 ^ cap as u64);
        let mut buf = TranscriptBuffer::new(cap);
        let mut prev_hw = 0u64;
        for i in 0..10_000u32 {
            let len = rng.random_range(0..=MIB);
            let offset = match i % 3 {
                0 => rng.random_range(0..(64u64 << 20)), // realistic band
                1 => rng.random::<u64>(),                // full range
                _ => u64::MAX - rng.random_range(0..(2u64 << 20)), // near-MAX band
            };
            buf.apply_delta(offset, &src[..len]);
            assert!(buf.len() <= cap, "I1: len <= max_bytes");
            assert!(buf.high_water() >= prev_hw, "I3: high_water monotonic");
            prev_hw = buf.high_water();
            assert!(
                buf.holes_for_test().len() <= MAX_HOLE_RANGES,
                "I2: bounded holes"
            );
            if i % 64 == 0 {
                let _ = buf.contiguous_tail(12_000); // reader never panics either
            }
        }
    }
}

// ----------------------------------- §5 content opacity (R-05.1, AC-12) --

#[test]
fn test_debug_output_contains_no_payload_bytes() {
    let sentinel = b"SENTINEL-TRANSCRIPT-SECRET";
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(0, sentinel);
    let dbg = format!("{buf:?}");
    for field in [
        "len:",
        "base_offset:",
        "high_water:",
        "holes:",
        "elided_bytes:",
    ] {
        assert!(
            dbg.contains(field),
            "Debug must carry metadata field {field}"
        );
    }
    assert!(
        !dbg.contains("SENTINEL"),
        "Debug must never carry content bytes"
    );
}

#[test]
fn test_clear_returns_bytes_purged() {
    let src = src_bytes(300);
    let mut buf = buf_with_one_hole(&src); // span 300 (incl. hole zero-fill)
    assert_eq!(buf.clear(), 300, "returns prior span length");
    assert_eq!(buf.len(), 0);
    assert!(buf.is_empty());
    assert_eq!(buf.high_water(), 300, "high_water unchanged (I3)");
    assert_eq!(buf.elided_bytes(), 0, "clear is a purge, not an elision");
    assert_eq!(
        buf.base_offset_for_test(),
        300,
        "base = high_water (pinned, R-10)"
    );
    assert!(buf.holes_for_test().is_empty());
    // Resumed stream at offsets >= high_water continues cleanly (no giant hole).
    buf.apply_delta(300, &src[0..50]);
    assert!(buf.holes_for_test().is_empty());
    assert_eq!(buf.contiguous_tail(50).as_deref(), Some(&src[0..50]));
    // Deltas below high_water after a clear are defined no-ops (clipped + counted).
    buf.apply_delta(0, &src[0..10]);
    assert_eq!(buf.len(), 50);
    assert_eq!(buf.elided_bytes(), 10);
}

/// R-10.2/.3 (cycle-review-purge §3, crt-052 inherits): after a cycle-review
/// clear, the client keeps streaming at high FILE offsets (it doesn't know the
/// server cleared). A resume far beyond `high_water` opens a large gap; the
/// pinned behavior is hole + ring-tail collapse to the newest bytes — the final
/// `contiguous_tail` serves exactly the newest contiguous bytes, no panic, no
/// zero-fill leaking into the tail (FR-19).
#[test]
fn test_post_clear_resumed_stream_serves_tail() {
    let src = src_bytes(60_000);
    let mut buf = TranscriptBuffer::new(4096);
    buf.apply_delta(0, &src[0..300]);
    assert_eq!(buf.clear(), 300);
    assert_eq!(buf.high_water(), 300);

    // Resume far beyond high_water: gap of ~49.7 KB, then 50 content bytes.
    buf.apply_delta(50_000, &src[50_000..50_050]);
    assert_eq!(buf.high_water(), 50_050, "high_water is monotonic");

    // Tail never crosses the gap: exactly the newest contiguous bytes.
    let tail = buf.contiguous_tail(4096).expect("tail must be served");
    assert_eq!(tail, &src[50_000..50_050], "newest contiguous bytes only");
    assert!(
        !tail.contains(&0u8),
        "no zero-fill (gap filler) in the tail"
    );

    // The stream continues contiguously from the resume point and extends the tail.
    buf.apply_delta(50_050, &src[50_050..50_150]);
    let tail = buf.contiguous_tail(4096).expect("tail must be served");
    assert_eq!(
        tail,
        &src[50_000..50_150],
        "contiguous continuation extends the tail"
    );
}

// ------------------------------------------ session_key seam (ADR-007) --

#[test]
fn test_session_key_oss_returns_session_id_unchanged() {
    assert_eq!(session_key("default", "", "sess-abc"), "sess-abc");
    assert_eq!(session_key("acme", "proj-x", "sess-abc"), "sess-abc");
    assert_eq!(session_key("", "", "http-sess-9"), "http-sess-9");
}
