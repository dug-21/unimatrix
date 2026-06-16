//! `snapshot()` primitive + `TranscriptSnapshot` / `HoleInfo` tests (crt-052 C2,
//! ADR-002). Below-cap correctness and content-opacity (R-19) cases; overflow /
//! ring-tail / poison-recovery snapshot cases live in
//! `session_transcript_tests_overflow.rs`. Split from
//! `session_transcript_tests.rs` (500-line file cap); shares its harness via
//! `use super::*`.

use super::*;

// ------------------------------- §6 snapshot() primitive (crt-052 C2) --
// ADR-002 / R-06, R-12, R-19.

#[test]
fn test_snapshot_returns_contiguous_span_no_holes_crossed() {
    // Covering delta set, no overflow: the snapshot span equals the full
    // contiguous content and crosses no hole / contains no zero-fill (FR-19).
    let src = src_bytes(1000);
    let mut buf = TranscriptBuffer::new(DEFAULT_TRANSCRIPT_BUFFER_MAX_BYTES, test_scanner());
    apply_all(&mut buf, &src, &covering_delta_set());
    assert!(buf.holes_for_test().is_empty(), "fixture covers [0,1000)");
    let snap = buf.snapshot();
    assert_eq!(
        snap.bytes.as_slice(),
        buf.contiguous_tail(buf.len()).as_deref().unwrap(),
        "snapshot span matches the contiguous reader"
    );
    assert_eq!(snap.bytes.as_slice(), &src[..], "whole written content");
    assert!(
        !snap.bytes.contains(&0u8),
        "no zero-fill byte (src_bytes never emits 0, FR-19)"
    );
}

#[test]
fn test_snapshot_metadata_matches_buffer_state() {
    // Metadata is read straight from the buffer counters — never recomputed.
    let src = src_bytes(300);
    let buf = buf_with_one_hole(&src); // span [0,100) ∪ [200,300), hole (100,200)
    let snap = buf.snapshot();
    assert_eq!(snap.base_offset, buf.base_offset_for_test());
    assert_eq!(snap.high_water, buf.high_water());
    assert_eq!(snap.elided_bytes, buf.elided_bytes());
    let expected_holes: Vec<HoleInfo> = buf
        .holes_for_test()
        .iter()
        .map(|&(start, end)| HoleInfo { start, end })
        .collect();
    assert_eq!(snap.holes, expected_holes, "holes mapped 1:1 from buffer");
}

#[test]
fn test_snapshot_returns_whole_span_not_windowed() {
    // snapshot() is the WHOLE contiguous run, not a clipped tail window.
    let src = src_bytes(300);
    let buf = buf_with_one_hole(&src); // contiguous run is [200,300): 100 bytes
    let snap = buf.snapshot();
    // contiguous_tail with a tiny window clips to the window; snapshot does not.
    assert_eq!(
        buf.contiguous_tail(10).as_deref(),
        Some(&src[290..300]),
        "windowed tail is clipped"
    );
    assert_eq!(
        snap.bytes.len(),
        100,
        "snapshot is the whole contiguous run"
    );
    assert_eq!(snap.bytes.as_slice(), &src[200..300]);
}

#[test]
fn test_snapshot_empty_buffer() {
    let buf = TranscriptBuffer::new(4096, test_scanner());
    let snap = buf.snapshot();
    assert!(snap.bytes.is_empty());
    assert_eq!(snap.base_offset, 0);
    assert_eq!(snap.high_water, 0);
    assert_eq!(snap.elided_bytes, 0);
    assert!(snap.holes.is_empty());
}

#[test]
fn test_snapshot_returns_truncated_tail_bytes() {
    // A "truncated final line" looks to the buffer like ordinary trailing bytes;
    // snapshot returns them intact (no zero-fill, no panic). Parsing is C3's job.
    let src = src_bytes(120);
    let mut buf = TranscriptBuffer::new(4096, test_scanner());
    buf.apply_delta(0, &src[0..120]); // last bytes are a "partial line"
    let snap = buf.snapshot();
    assert_eq!(snap.bytes.as_slice(), &src[..], "trailing bytes intact");
    assert!(!snap.bytes.contains(&0u8), "no zero-fill");
}

#[test]
fn test_snapshot_exposes_all_four_metadata_fields() {
    // AC-V-SEAM: all four metadata fields public + populated so #700 needs zero
    // byte re-read. Use an overflowed-with-hole buffer so every field is non-trivial.
    let src = src_bytes(600);
    let mut buf = TranscriptBuffer::new(200, test_scanner());
    buf.apply_delta(0, &src[0..150]);
    buf.apply_delta(300, &src[300..450]); // ring-tail + hole
    let snap = buf.snapshot();
    // Field access proves visibility; values prove population.
    let _ = (
        &snap.base_offset,
        &snap.high_water,
        &snap.elided_bytes,
        &snap.holes,
    );
    assert!(snap.base_offset > 0, "base_offset populated under overflow");
    assert!(snap.high_water >= 450, "high_water populated");
    assert!(
        snap.elided_bytes > 0,
        "elided_bytes populated under overflow"
    );
}

#[test]
fn test_700_reuse_parses_snapshot_bytes_without_contiguous_tail() {
    // #700-shaped marker recovery: run an OWN pattern over snapshot.bytes using
    // only base_offset, with no contiguous_tail / other buffer accessor call.
    let mut src = src_bytes(400);
    // Embed a recognizable marker the recovery caller scans for.
    let marker = b"# DECISION:";
    src[200..200 + marker.len()].copy_from_slice(marker);
    let mut buf = TranscriptBuffer::new(4096, test_scanner());
    buf.apply_delta(0, &src);
    let snap = buf.snapshot();

    // Caller's own pattern pass — purely over the owned bytes + base_offset.
    let in_span = snap
        .bytes
        .windows(marker.len())
        .position(|w| w == marker)
        .expect("marker present in snapshot bytes");
    let logical_offset = snap.base_offset + in_span as u64;
    assert_eq!(
        logical_offset, 200,
        "logical offset = base_offset + in-span"
    );
}

// ------------------------------- §7 content opacity: snapshot Debug (R-19) --

#[test]
fn test_snapshot_debug_metadata_only() {
    let sentinel = b"SENTINEL-SNAPSHOT-SECRET";
    let mut buf = TranscriptBuffer::new(4096, test_scanner());
    buf.apply_delta(0, sentinel);
    let snap = buf.snapshot();
    let dbg = format!("{snap:?}");
    for field in ["len", "base_offset", "high_water", "holes", "elided_bytes"] {
        assert!(
            dbg.contains(field),
            "Debug must carry metadata field {field}"
        );
    }
    assert!(
        !dbg.contains("SENTINEL"),
        "Debug must never carry content bytes"
    );
    // Also assert the byte values themselves are absent (numbers, not ASCII).
    assert!(
        !dbg.contains(&format!("{}", sentinel[0])),
        "no raw byte value leaked"
    );
}

#[test]
fn test_holeinfo_debug_safe() {
    let h = HoleInfo {
        start: 100,
        end: 200,
    };
    let dbg = format!("{h:?}");
    assert!(dbg.contains("start") && dbg.contains("100"));
    assert!(dbg.contains("end") && dbg.contains("200"));
}

// ----------------------------- crt-054 ActivitySnapshot read surface (C4) --
// Shape / content-opacity (AC-08). Read-before-purge (AC-07) is Stage 3c.

use crate::infra::transcript_activity::{ActivitySnapshot, MAX_SIGNAL_CLASSES};

#[test]
fn test_activity_snapshot_is_copy() {
    // Copy: re-using `snap` after a by-value copy compiles only if Copy.
    let snap = ActivitySnapshot::empty();
    let _copy = snap;
    let _again = snap; // would not compile if ActivitySnapshot were not Copy
    assert_eq!(snap.bytes_total, 0);
}

#[test]
fn test_activity_snapshot_shape_matches_contract() {
    // Field set/widths/order exactly { bytes_total: u64, delta_count: u32,
    // class_counts: [u32; MAX_SIGNAL_CLASSES] } — crt-055 Surface B contract.
    let snap = ActivitySnapshot {
        bytes_total: 1u64,
        delta_count: 2u32,
        class_counts: [0u32; MAX_SIGNAL_CLASSES],
    };
    let _b: u64 = snap.bytes_total;
    let _d: u32 = snap.delta_count;
    let _c: [u32; MAX_SIGNAL_CLASSES] = snap.class_counts;
    assert_eq!(MAX_SIGNAL_CLASSES, 16, "pinned == 16 (AC-11)");
}

#[test]
fn test_activity_snapshot_debug_is_metadata_only() {
    // Debug prints only the scalar counters — no transcript bytes can appear
    // (the struct has no byte-bearing field).
    let snap = ActivitySnapshot {
        bytes_total: 42,
        delta_count: 3,
        class_counts: [0; MAX_SIGNAL_CLASSES],
    };
    let dbg = format!("{snap:?}");
    assert!(dbg.contains("bytes_total") && dbg.contains("42"));
    assert!(dbg.contains("delta_count") && dbg.contains("3"));
    assert!(dbg.contains("class_counts"));
}

#[test]
fn test_buffer_activity_snapshot_reflects_folds() {
    // The buffer accessor returns the folded totals (AC-05/AC-07 read side).
    let mut buf = TranscriptBuffer::new(4096, test_scanner());
    buf.apply_delta(0, b"abcdef"); // 6 bytes
    let snap = buf.activity_snapshot();
    assert_eq!(snap.bytes_total, 6);
    assert_eq!(snap.delta_count, 1);
}
