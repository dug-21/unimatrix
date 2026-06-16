//! crt-055 Component 6 — Activity-fold landing unit tests.
//!
//! Test plan: `product/features/crt-055/test-plan/activity_fold_landing.md`.
//!
//! Covers the UNIT-testable contracts:
//! - AC-07: each column equals the SUMMED snapshot field; class counts by FIXED
//!   index (`[0]=error`, `[1]=refusal`); `signal_class_counts_json` is the full
//!   `class_name → count` map serialized via serde (never concatenation) and
//!   round-trips; forward-compatible for classes beyond error/refusal (NFR-06).
//! - AC-14 / R-09: near-`u64::MAX` / large-`u32` folds saturate to `i64`, NEVER
//!   wrap. (The near-`u64::MAX` case is unreachable through the real fold path, so
//!   it is driven through the pure `fold_snapshots` split.)
//! - AC-19 / R-11: the consumed surface is counters-only; no content reaches the
//!   persist path; `signal_class_counts_json` is a count map, not content bytes.
//! - Read-before-purge availability semantics (R-04): an undeclared-only / empty
//!   cycle yields `available == false` (→ Component 7 "unavailable", never `0`).
//!
//! The read-before-purge ORDERING + inversion (AC-08) and the held-route
//! silent-zero harness guard (AC-09) require the real pipeline call order and are
//! Stage 3c / Component 9 integration tests — see the test plan's Integration
//! section. This file asserts everything that is unit-testable in isolation.

use super::{FoldLanding, build_signal_json, fold_snapshots, land_activity_fold};
use crate::infra::session::SessionRegistry;
use crate::infra::transcript_activity::{ActivitySnapshot, MAX_SIGNAL_CLASSES};

/// Build an `ActivitySnapshot` with the given counters; remaining class slots 0.
fn snap(bytes_total: u64, delta_count: u32, classes: &[(usize, u32)]) -> ActivitySnapshot {
    let mut class_counts = [0u32; MAX_SIGNAL_CLASSES];
    for &(idx, n) in classes {
        class_counts[idx] = n;
    }
    ActivitySnapshot {
        bytes_total,
        delta_count,
        class_counts,
    }
}

/// The v1 catalog names in config order (ADR-008): index 0 = error, 1 = refusal.
fn v1_class_names() -> Vec<String> {
    vec!["error".to_string(), "refusal".to_string()]
}

// =============================================================================
// Fold → columns (AC-07)
// =============================================================================

#[test]
fn test_fold_lands_bytes_and_delta() {
    let snaps = vec![("S".to_string(), snap(1234, 7, &[]))];
    let landing = fold_snapshots(&snaps, &v1_class_names());

    assert_eq!(landing.transcript_bytes_total, 1234);
    assert_eq!(landing.transcript_delta_count, 7);
    assert!(landing.available);
}

#[test]
fn test_fold_lands_class_counts_by_pinned_index() {
    // class_counts[0] = error = 5, [1] = refusal = 3 (ADR-008 fixed indices).
    let snaps = vec![("S".to_string(), snap(100, 2, &[(0, 5), (1, 3)]))];
    let landing = fold_snapshots(&snaps, &v1_class_names());

    assert_eq!(
        landing.transcript_error_count, 5,
        "error == class_counts[0]"
    );
    assert_eq!(
        landing.transcript_refusal_count, 3,
        "refusal == class_counts[1]"
    );
    // R-12: a producer reorder (error/refusal swapped) would surface here — the
    // columns read by FIXED index, so swapping the source counts swaps the cols.
    let swapped = vec![("S".to_string(), snap(100, 2, &[(0, 3), (1, 5)]))];
    let swapped_landing = fold_snapshots(&swapped, &v1_class_names());
    assert_ne!(
        swapped_landing.transcript_error_count, landing.transcript_error_count,
        "fixed-index read makes a catalog reorder observable (R-12)"
    );
}

#[test]
fn test_fold_sums_across_held_sessions() {
    // Two sessions in one cycle → columns equal the SUM, not a single session.
    let snaps = vec![
        ("S1".to_string(), snap(1000, 4, &[(0, 2), (1, 1)])),
        ("S2".to_string(), snap(2000, 6, &[(0, 3), (1, 4)])),
    ];
    let landing = fold_snapshots(&snaps, &v1_class_names());

    assert_eq!(landing.transcript_bytes_total, 3000);
    assert_eq!(landing.transcript_delta_count, 10);
    assert_eq!(landing.transcript_error_count, 5);
    assert_eq!(landing.transcript_refusal_count, 5);
}

#[test]
fn test_signal_class_counts_json_matches_catalog() {
    let snaps = vec![("S".to_string(), snap(10, 1, &[(0, 5), (1, 3)]))];
    let landing = fold_snapshots(&snaps, &v1_class_names());

    // Serialized via serde — round-trips to the same map.
    let parsed: serde_json::Value =
        serde_json::from_str(&landing.signal_class_counts_json).expect("valid JSON");
    assert_eq!(parsed["error"], serde_json::json!(5));
    assert_eq!(parsed["refusal"], serde_json::json!(3));
    let obj = parsed.as_object().expect("object");
    assert_eq!(obj.len(), 2, "only the enabled classes appear");
}

#[test]
fn test_signal_json_forward_compatible_beyond_error_refusal() {
    // NFR-06: a class added beyond error/refusal lands in the JSON map with no
    // new column. Index 2 = "timeout" (a hypothetical domain extension).
    let names = vec![
        "error".to_string(),
        "refusal".to_string(),
        "timeout".to_string(),
    ];
    let snaps = vec![("S".to_string(), snap(10, 1, &[(0, 1), (1, 2), (2, 9)]))];
    let landing = fold_snapshots(&snaps, &names);

    let parsed: serde_json::Value =
        serde_json::from_str(&landing.signal_class_counts_json).expect("valid JSON");
    assert_eq!(parsed["timeout"], serde_json::json!(9));
    // The dedicated columns still track error/refusal by fixed index.
    assert_eq!(landing.transcript_error_count, 1);
    assert_eq!(landing.transcript_refusal_count, 2);
}

#[test]
fn test_signal_json_empty_catalog_is_empty_object() {
    let snaps = vec![("S".to_string(), snap(10, 1, &[]))];
    let landing = fold_snapshots(&snaps, &[]);
    assert_eq!(landing.signal_class_counts_json, "{}");
}

#[test]
fn test_signal_json_class_name_with_special_chars_is_escaped() {
    // A class_name from config may contain JSON-special chars; serde escapes it
    // safely (never string concatenation). Round-trips to the literal key.
    let names = vec![r#"weird"name\with/specials"#.to_string()];
    let counts = build_signal_json(
        &{
            let mut a = [0u64; MAX_SIGNAL_CLASSES];
            a[0] = 42;
            a
        },
        &names,
    );
    let parsed: serde_json::Value = serde_json::from_str(&counts).expect("escaped JSON parses");
    assert_eq!(parsed[r#"weird"name\with/specials"#], serde_json::json!(42));
}

// =============================================================================
// Width conversion (R-09, AC-14)
// =============================================================================

#[test]
fn test_fold_width_conversion_saturates() {
    // bytes_total near u64::MAX → saturates to i64::MAX, NEVER wraps negative.
    let snaps = vec![("S".to_string(), snap(u64::MAX, u32::MAX, &[(0, u32::MAX)]))];
    let landing = fold_snapshots(&snaps, &v1_class_names());

    assert_eq!(landing.transcript_bytes_total, i64::MAX);
    assert!(
        landing.transcript_bytes_total > 0,
        "saturated, never wrapped to a negative count"
    );
    // u32::MAX widens to u64 cleanly — well within i64 range, no saturation.
    assert_eq!(landing.transcript_delta_count, i64::from(u32::MAX));
    assert_eq!(landing.transcript_error_count, i64::from(u32::MAX));
}

#[test]
fn test_fold_summation_saturates_at_i64_max() {
    // Two near-max byte totals: producer-width saturating_add caps at u64::MAX,
    // then converts to i64::MAX. Proves the sum path never wraps either.
    let snaps = vec![
        ("S1".to_string(), snap(u64::MAX, 1, &[])),
        ("S2".to_string(), snap(u64::MAX, 1, &[])),
    ];
    let landing = fold_snapshots(&snaps, &v1_class_names());
    assert_eq!(landing.transcript_bytes_total, i64::MAX);
    assert_eq!(landing.transcript_delta_count, 2);
}

// =============================================================================
// Availability / silent-zero semantics (R-04)
// =============================================================================

#[test]
fn test_empty_cycle_is_unavailable_not_zero() {
    // No held/declared sessions → available == false → Component 7 renders
    // "unavailable", never a measured 0 (R-04). Columns are 0 but the flag is
    // what distinguishes absence from a measured zero.
    let landing = fold_snapshots(&[], &v1_class_names());
    assert!(!landing.available, "empty fold is UNAVAILABLE");
    assert_eq!(landing.transcript_bytes_total, 0);
    // The catalog names still render (zeroed); the `available` flag — not the
    // JSON shape — distinguishes absence from a measured 0. Empty CATALOG → "{}".
    let parsed: serde_json::Value =
        serde_json::from_str(&landing.signal_class_counts_json).expect("valid JSON");
    assert_eq!(parsed["error"], serde_json::json!(0));
    assert_eq!(parsed["refusal"], serde_json::json!(0));
}

#[test]
fn test_present_session_with_zero_buffer_is_available() {
    // A declared session present with a genuinely zero buffer is a MEASURED zero:
    // available == true (distinct from absence).
    let snaps = vec![("S".to_string(), snap(0, 0, &[]))];
    let landing = fold_snapshots(&snaps, &v1_class_names());
    assert!(
        landing.available,
        "present-but-zero is available (measured 0)"
    );
    assert_eq!(landing.transcript_bytes_total, 0);
}

// =============================================================================
// Leak gate (R-11, AC-19)
// =============================================================================

#[test]
fn test_consumed_surface_is_metadata_only() {
    // The landing carries ONLY integer counters + a count-map JSON string. The
    // JSON is a class_name → count map, never transcript bytes. Assert the JSON
    // contains only the catalog keys mapped to integers (no content payload).
    let snaps = vec![("S".to_string(), snap(9999, 3, &[(0, 1), (1, 2)]))];
    let landing = fold_snapshots(&snaps, &v1_class_names());

    let parsed: serde_json::Value =
        serde_json::from_str(&landing.signal_class_counts_json).expect("valid JSON");
    for (key, val) in parsed.as_object().expect("object") {
        assert!(
            key == "error" || key == "refusal",
            "JSON keys are catalog class names only, not content: {key}"
        );
        assert!(
            val.is_i64() || val.is_u64(),
            "values are integer counts only"
        );
    }
}

// =============================================================================
// Registry-backed end-to-end (land_activity_fold), AC-07 + R-04
// =============================================================================

#[test]
fn test_land_activity_fold_reads_registered_session() {
    // Drive the real collector: a registered session in the cycle with streamed
    // bytes yields a non-empty, available fold. (The default scanner is empty in
    // a bare registry, so class counts are 0 — bytes/deltas are what matter.)
    let registry = SessionRegistry::with_transcript_cap(4 * 1024 * 1024);
    let cycle = "crt-055-fold-test";
    registry.register_session("S", None, Some(cycle.to_string()));
    registry.apply_transcript_delta("S", 0, b"hello world bytes");

    let landing = land_activity_fold(&registry, cycle, &v1_class_names());
    assert!(landing.available, "a declared session contributes a fold");
    assert_eq!(
        landing.transcript_bytes_total,
        b"hello world bytes".len() as i64
    );
    assert!(landing.transcript_delta_count >= 1);
}

#[test]
fn test_land_activity_fold_undeclared_cycle_is_unavailable() {
    // A cycle with NO declared sessions → collector returns empty → unavailable.
    let registry = SessionRegistry::with_transcript_cap(4 * 1024 * 1024);
    registry.register_session("S", None, Some("some-other-cycle".to_string()));
    registry.apply_transcript_delta("S", 0, b"bytes for another cycle");

    let landing = land_activity_fold(&registry, "crt-055-empty", &v1_class_names());
    assert!(
        !landing.available,
        "no declared session → unavailable, not 0"
    );
    assert_eq!(landing.transcript_bytes_total, 0);
}

#[test]
fn test_land_activity_fold_undeclared_session_does_not_zero_valid() {
    // One UNDECLARED session (feature == None) among a valid declared session →
    // the undeclared one is ABSENT from the collector and does not zero the
    // declared session's fold (per-session presence in the sum, R-04).
    let registry = SessionRegistry::with_transcript_cap(4 * 1024 * 1024);
    let cycle = "crt-055-mixed";
    registry.register_session("DECLARED", None, Some(cycle.to_string()));
    registry.apply_transcript_delta("DECLARED", 0, b"declared bytes");
    registry.register_session("UNDECLARED", None, None);
    registry.apply_transcript_delta("UNDECLARED", 0, b"undeclared bytes");

    let landing = land_activity_fold(&registry, cycle, &v1_class_names());
    assert!(landing.available);
    assert_eq!(
        landing.transcript_bytes_total,
        b"declared bytes".len() as i64,
        "the undeclared session does not contribute and does not zero the valid one"
    );
}

#[test]
fn test_fold_landing_default_is_unavailable_empty() {
    // Defensive: the Default landing is a coherent unavailable/empty state.
    let landing = FoldLanding::default();
    assert!(!landing.available);
    assert_eq!(landing.transcript_bytes_total, 0);
    assert_eq!(landing.signal_class_counts_json, "");
}
