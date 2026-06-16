//! Tests for the crt-054 transcript-activity foundation (Surface B types):
//! `ActivityCounters` fold arithmetic (AC-05), the `SignatureScanner` single
//! shared scan / multi-class behavior (AC-09), content-opacity of the
//! `ActivitySnapshot` read surface (AC-08), the pinned `MAX_SIGNAL_CLASSES`
//! constant (AC-11), and cast-free width handling (AC-14).
//!
//! Scope: this module owns the SCAN behavior + the fold arithmetic. Catalog
//! default-set correctness and `validate()` rejection live in the config
//! component; here the scanner is built from already-valid patterns (plus one
//! explicit invalid-regex compile case pairing the config plan's loud reject).

use super::*;

fn error_refusal_scanner() -> SignatureScanner {
    // v1 catalog: index 0 = error, index 1 = refusal.
    SignatureScanner::compile(&["err.*".to_string(), "refus.*".to_string()])
        .expect("v1 catalog patterns compile")
}

// ---- MAX_SIGNAL_CLASSES (AC-11) ----

#[test]
fn test_max_signal_classes_is_exactly_16() {
    assert_eq!(
        MAX_SIGNAL_CLASSES, 16,
        "MAX_SIGNAL_CLASSES is pinned at 16 (AC-11)"
    );
}

// ---- ActivityCounters fold arithmetic (AC-05) ----

#[test]
fn test_fold_bytes_total_sums_delta_lengths() {
    let scanner = SignatureScanner::empty();
    let mut counters = ActivityCounters::new();
    counters.fold(b"abc", &scanner);
    counters.fold(b"de", &scanner);
    let snap = counters.snapshot();
    assert_eq!(snap.bytes_total, 5);
    assert_eq!(snap.delta_count, 2);
}

#[test]
fn test_fold_delta_count_increments_per_call() {
    let scanner = SignatureScanner::empty();
    let mut counters = ActivityCounters::new();
    for _ in 0..7 {
        counters.fold(b"payload bytes here", &scanner);
    }
    // One increment per delta, never per byte or per match.
    assert_eq!(counters.snapshot().delta_count, 7);
}

#[test]
fn test_fold_empty_delta_counts_no_bytes() {
    let scanner = error_refusal_scanner();
    let mut counters = ActivityCounters::new();
    counters.fold(b"", &scanner);
    let snap = counters.snapshot();
    assert_eq!(snap.delta_count, 1, "empty delta still counts as a delta");
    assert_eq!(snap.bytes_total, 0, "empty delta adds no bytes");
    assert_eq!(
        snap.class_counts, [0; MAX_SIGNAL_CLASSES],
        "no spurious class match"
    );
}

#[test]
fn test_new_yields_all_zero() {
    let snap = ActivityCounters::new().snapshot();
    assert_eq!(snap.bytes_total, 0);
    assert_eq!(snap.delta_count, 0);
    assert_eq!(snap.class_counts, [0; MAX_SIGNAL_CLASSES]);
}

// ---- SignatureScanner: single shared scan, multi-class (AC-09) ----

#[test]
fn test_compile_error_refusal_class_count_is_two() {
    let scanner = error_refusal_scanner();
    assert_eq!(scanner.class_count(), 2);
}

#[test]
fn test_scan_over_delta_matching_both_yields_zero_and_one() {
    let scanner = error_refusal_scanner();
    let matched: Vec<usize> = scanner
        .scan(b"error: the model refused the request")
        .collect();
    assert_eq!(
        matched,
        vec![0, 1],
        "single scan yields both class indices (AC-09)"
    );
}

#[test]
fn test_scanner_single_scan_increments_matched_classes() {
    let scanner = error_refusal_scanner();
    let mut counters = ActivityCounters::new();
    counters.fold(b"error and refusal in one delta", &scanner);
    let snap = counters.snapshot();
    assert!(snap.class_counts[0] >= 1, "error class incremented");
    assert!(snap.class_counts[1] >= 1, "refusal class incremented");
}

#[test]
fn test_scanner_match_count_is_per_delta_not_per_occurrence() {
    let scanner = error_refusal_scanner();
    let mut counters = ActivityCounters::new();
    // Two occurrences of the error signature in one delta -> +1, not +2.
    counters.fold(b"error then error again", &scanner);
    assert_eq!(counters.snapshot().class_counts[0], 1);
}

#[test]
fn test_scanner_no_match_leaves_class_counts_zero() {
    let scanner = error_refusal_scanner();
    let mut counters = ActivityCounters::new();
    counters.fold(b"a totally benign payload", &scanner);
    let snap = counters.snapshot();
    assert_eq!(snap.class_counts, [0; MAX_SIGNAL_CLASSES]);
    assert_eq!(snap.delta_count, 1, "delta still advances on no match");
    assert_eq!(snap.bytes_total, 24);
}

#[test]
fn test_empty_scanner_scans_to_nothing_but_fold_advances() {
    let scanner = SignatureScanner::empty();
    assert_eq!(scanner.class_count(), 0);
    let matched: Vec<usize> = scanner.scan(b"error refusal").collect();
    assert!(matched.is_empty(), "empty scanner matches nothing");
    let mut counters = ActivityCounters::new();
    counters.fold(b"error refusal", &scanner);
    let snap = counters.snapshot();
    assert_eq!(snap.delta_count, 1);
    assert_eq!(snap.bytes_total, 13);
    assert_eq!(snap.class_counts, [0; MAX_SIGNAL_CLASSES]);
}

#[test]
fn test_scanner_bytes_domain_non_utf8_no_panic() {
    let scanner = error_refusal_scanner();
    let mut counters = ActivityCounters::new();
    // Invalid UTF-8 bytes (0xFF, 0xFE, lone continuation byte) — must scan via
    // the bytes-domain RegexSet without panic and without a validation pass.
    let bytes: &[u8] = &[0xFF, 0xFE, 0x80, b'e', b'r', b'r', 0x00, 0xC0];
    counters.fold(bytes, &scanner);
    let snap = counters.snapshot();
    assert_eq!(snap.delta_count, 1);
    assert_eq!(snap.bytes_total, bytes.len() as u64);
    // "err" appears, so the error class matches even amid non-UTF-8 bytes.
    assert_eq!(snap.class_counts[0], 1);
}

#[test]
fn test_compile_invalid_regex_returns_err() {
    // Unbalanced group is an invalid pattern.
    let result = SignatureScanner::compile(&["valid".to_string(), "(unbalanced".to_string()]);
    match result {
        Err(ScannerError::InvalidRegex(_)) => {}
        other => panic!("expected Err(InvalidRegex), got {other:?}"),
    }
}

// ---- Content-opacity / Copy / no Display (AC-08) ----

#[test]
fn test_counters_and_snapshot_are_copy() {
    fn assert_copy<T: Copy>() {}
    assert_copy::<ActivityCounters>();
    assert_copy::<ActivitySnapshot>();
}

#[test]
fn test_snapshot_debug_is_metadata_only() {
    // Metadata-only Debug: prints counts/scalars, never bytes. Confirm the field
    // labels are present and no content can appear (there is no content field).
    let snap = ActivitySnapshot {
        bytes_total: 42,
        delta_count: 3,
        class_counts: [0; MAX_SIGNAL_CLASSES],
    };
    let rendered = format!("{snap:?}");
    assert!(rendered.contains("ActivitySnapshot"));
    assert!(rendered.contains("bytes_total"));
    assert!(rendered.contains("delta_count"));
    assert!(rendered.contains("class_counts"));
    assert!(rendered.contains("42"));
}

#[test]
fn test_activity_snapshot_empty_is_all_zero() {
    let snap = ActivitySnapshot::empty();
    assert_eq!(snap.bytes_total, 0);
    assert_eq!(snap.delta_count, 0);
    assert_eq!(snap.class_counts, [0; MAX_SIGNAL_CLASSES]);
}

// ---- Width / cast-free (AC-14) ----

#[test]
fn test_bytes_total_holds_large_value_un_narrowed() {
    // bytes_total is full u64 width; snapshot round-trips a near-u64::MAX value
    // without any narrowing toward i64 (the i64 conversion is crt-055's).
    let snap = ActivitySnapshot {
        bytes_total: u64::MAX - 1,
        delta_count: u32::MAX,
        class_counts: [u32::MAX; MAX_SIGNAL_CLASSES],
    };
    assert_eq!(snap.bytes_total, u64::MAX - 1);
    assert_eq!(snap.delta_count, u32::MAX);
    assert_eq!(snap.class_counts[0], u32::MAX);
}

#[test]
fn test_fold_saturates_rather_than_panics() {
    let scanner = SignatureScanner::empty();
    let mut counters = ActivityCounters {
        bytes_total: u64::MAX,
        delta_count: u32::MAX,
        class_counts: [0; MAX_SIGNAL_CLASSES],
    };
    counters.fold(b"more", &scanner);
    let snap = counters.snapshot();
    // Saturating add: stays clamped, no overflow panic in debug builds.
    assert_eq!(snap.bytes_total, u64::MAX);
    assert_eq!(snap.delta_count, u32::MAX);
}
