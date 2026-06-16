//! crt-055 Component 6 — Activity-fold landing (read-before-purge).
//!
//! A self-contained helper invoked from the `context_cycle_review` review
//! pipeline (Component 9, `tools.rs`) STRICTLY BEFORE `purge_cycle_transcripts`.
//! It reads crt-054's per-session `ActivitySnapshot` fold for the cycle, sums
//! across the cycle's held/registered sessions, width-converts the producer
//! `u64`/`u32` counters into the `i64` persist widths (checked/saturating —
//! never wraps, never panics), and builds the forward-compatible
//! `signal_class_counts_json` map. (ADR-007 #5042, ADR-008 #5043, ADR-003 #5046.)
//!
//! **Read-before-purge (ADR-007 / Constraint 4 / R-03):** the
//! `activity_snapshots_for_feature` read MUST precede `purge_cycle_transcripts`.
//! This module performs the READ only; Component 9 places the call ahead of the
//! purge. crt-052's hold purge zeroes/drops the buffers — reading after it
//! silently zeroes every transcript column. The ordering is the load-bearing
//! contract; the inversion is asserted at the integration layer (Component 9).
//!
//! **Width conversion (R-09 / AC-14):** every producer counter (`u64` bytes,
//! `u32` deltas/class-counts) is summed in producer widths with saturating
//! arithmetic, then converted to `i64` via [`u64_to_i64_saturating`]. The
//! overflow path is practically impossible (it requires `> i64::MAX` summed
//! bytes) but it saturates-and-warns rather than wrapping or panicking.
//!
//! **Fixed catalog indices (ADR-008 #5043):** `class_counts[0] = error`,
//! `class_counts[1] = refusal`, by FIXED index. A producer reorder corrupts
//! every column with no type error — the index contract is pinned by test.
//!
//! **Structural leak gate (NFR-01 / R-11):** this fold is content-free — only
//! integer counters and a `class_name → count` map. No transcript bytes touch
//! the persist path; [`ActivitySnapshot`] is scalars-only by construction.

// Wave-3 staging: this module is the read-before-purge LANDING. Its public
// surface (`land_activity_fold` / `FoldLanding`) is wired into the review
// pipeline by Component 9 (`tools.rs`), a LATER edit in the same wave. Until that
// call site lands, these items have no in-tree caller outside the unit tests, so
// the dead-code lint is suppressed module-wide. Remove this `allow` when
// Component 9 places the `land_activity_fold(...)` call STRICTLY BEFORE
// `purge_cycle_transcripts` (ADR-007 / R-03).
#![allow(dead_code)]

use crate::infra::session::SessionRegistry;
use crate::infra::transcript_activity::{ActivitySnapshot, MAX_SIGNAL_CLASSES};

/// The width-converted, summed transcript fold for one cycle.
///
/// Produced by [`land_activity_fold`] BEFORE purge; consumed by Component 9,
/// which wires the `i64`/`String` fields onto the `CycleReviewRecord` (persist)
/// and the rendered `transcript_error_count`/`transcript_refusal_count` plus the
/// `transcript_fold_available` flag onto the presentation layer (Component 7).
///
/// Every metric field is `i64` (the persist column width) or `String` (the JSON
/// map). No byte-bearing field — the leak gate stays intact by construction
/// (NFR-01). `derive(Debug)` is safe: scalars + a count-map JSON string only.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct FoldLanding {
    /// `true` when ≥1 declared session contributed a fold (the collector returned
    /// at least one snapshot). Drives `transcript_fold_available` (Component 7):
    /// an undeclared-only cycle surfaces `unavailable`, NEVER a measured `0`
    /// (R-04). A present session with a genuinely zero buffer counts as available.
    pub(crate) available: bool,
    /// Σ `ActivitySnapshot.bytes_total` across the cycle's sessions.
    pub(crate) transcript_bytes_total: i64,
    /// Σ `ActivitySnapshot.delta_count` across the cycle's sessions.
    pub(crate) transcript_delta_count: i64,
    /// Σ `class_counts[0]` (error, ADR-008 fixed index).
    pub(crate) transcript_error_count: i64,
    /// Σ `class_counts[1]` (refusal, ADR-008 fixed index).
    pub(crate) transcript_refusal_count: i64,
    /// Full `class_name → summed count` map, serialized via serde_json (never
    /// string concatenation). Forward-compatible: classes beyond error/refusal
    /// land here with no new column or migration (NFR-06). Empty catalog → `"{}"`.
    pub(crate) signal_class_counts_json: String,
}

/// Read each held/registered session's `ActivitySnapshot` for `feature_cycle`,
/// sum across the cycle's sessions, width-convert to `i64`, and build the
/// signal-class JSON map.
///
/// `class_names` is the enabled `[transcript_signals]` catalog in CONFIG ORDER
/// (index == `class_counts` index, ADR-008). Component 9 supplies it from the
/// startup-validated config; an empty slice yields `signal_class_counts_json ==
/// "{}"` while the fixed `error`/`refusal` columns still land by index.
///
/// **MUST be called STRICTLY BEFORE `purge_cycle_transcripts`** (ADR-007 / R-03):
/// the collector reads live buffers; the purge zeroes/drops them. Infallible —
/// the collector degrades a poisoned lock to an empty buffer (#4764), never
/// panics, never drops a session.
pub(crate) fn land_activity_fold(
    registry: &SessionRegistry,
    feature_cycle: &str,
    class_names: &[String],
) -> FoldLanding {
    // crt-054's collector (infra/session.rs). Registered ∪ held, deduped by Arc,
    // filtered to `feature_cycle`. UNDECLARED sessions (`feature == None`) are
    // ABSENT — never a fabricated zero. A declared session with a zero buffer
    // appears with a zero snapshot (a measured zero, distinct from absence).
    let snaps = registry.activity_snapshots_for_feature(feature_cycle);
    fold_snapshots(&snaps, class_names)
}

/// Pure fold over an already-read set of `(session_id, ActivitySnapshot)` pairs.
///
/// Split from [`land_activity_fold`] so the summing + width-conversion + JSON
/// contract is unit-testable WITHOUT the registry — in particular the near-
/// `u64::MAX` saturation case (AC-14), which cannot be produced through the real
/// fold path. The split changes no behavior: `land_activity_fold` is exactly
/// `fold_snapshots(&collector_output, class_names)`.
fn fold_snapshots(snaps: &[(String, ActivitySnapshot)], class_names: &[String]) -> FoldLanding {
    // ≥1 declared session contributed a fold → the fold is AVAILABLE. Drives
    // Component 7's `transcript_fold_available` so an undeclared-only cycle
    // renders "unavailable", never a measured `0` (R-04).
    let available = !snaps.is_empty();

    // Sum in producer widths with saturating arithmetic — a pathological stream
    // cannot wrap or panic. Convert to `i64` once, at the persist boundary.
    let mut bytes_total_u64: u64 = 0;
    let mut delta_count_u64: u64 = 0;
    let mut class_sums_u64: [u64; MAX_SIGNAL_CLASSES] = [0; MAX_SIGNAL_CLASSES];

    for (_session_id, snap) in snaps {
        bytes_total_u64 = bytes_total_u64.saturating_add(snap.bytes_total);
        // Widening u32 → u64 (NOT the forbidden narrowing toward i64); summed
        // here, narrowed to i64 once below.
        delta_count_u64 = delta_count_u64.saturating_add(u64::from(snap.delta_count));
        for (sum, &count) in class_sums_u64.iter_mut().zip(snap.class_counts.iter()) {
            *sum = sum.saturating_add(u64::from(count));
        }
    }

    FoldLanding {
        available,
        transcript_bytes_total: u64_to_i64_saturating(bytes_total_u64),
        transcript_delta_count: u64_to_i64_saturating(delta_count_u64),
        // Fixed catalog indices (ADR-008): [0] = error, [1] = refusal.
        transcript_error_count: u64_to_i64_saturating(class_sums_u64[0]),
        transcript_refusal_count: u64_to_i64_saturating(class_sums_u64[1]),
        signal_class_counts_json: build_signal_json(&class_sums_u64, class_names),
    }
}

/// Checked/saturating `u64 → i64` at the persist boundary (R-09 / AC-14).
///
/// `> i64::MAX` saturates to `i64::MAX` and warns; it NEVER wraps (a plain
/// `as i64` would wrap a value past `i64::MAX` into a negative count, silently
/// corrupting a cross-cycle baseline). Practically unreachable — it requires a
/// summed byte total past 9.2 EB — but it is the explicit non-wrapping contract.
fn u64_to_i64_saturating(value: u64) -> i64 {
    if value > i64::MAX as u64 {
        tracing::warn!(
            value,
            "transcript fold counter exceeds i64::MAX; saturating to i64::MAX (R-09)"
        );
        i64::MAX
    } else {
        // In-range: the cast is exact (value <= i64::MAX), never wraps.
        value as i64
    }
}

/// Build `signal_class_counts_json` — the forward-compatible `class_name → count`
/// map (ADR-007 / NFR-06).
///
/// Maps each ENABLED class name (config order) to its summed, width-converted
/// count. Serialized via `serde_json` — NEVER string concatenation — because a
/// `class_name` comes from operator config and serde escapes any JSON-special
/// characters safely. An empty catalog yields `"{}"`. Classes beyond the fixed
/// `error`/`refusal` columns land here with no migration.
///
/// `preserve_order` is enabled on `serde_json` (Cargo.toml), so the map renders
/// in config order — stable across runs for diff-friendly cross-cycle reads.
fn build_signal_json(class_sums_u64: &[u64; MAX_SIGNAL_CLASSES], class_names: &[String]) -> String {
    let mut map = serde_json::Map::new();
    // Only enabled classes (idx < class_count); a name beyond MAX_SIGNAL_CLASSES
    // cannot exist (validate() bounds the enabled count), but guard the index
    // defensively so a future catalog cannot read out of bounds.
    for (idx, name) in class_names.iter().enumerate() {
        if idx >= MAX_SIGNAL_CLASSES {
            break;
        }
        let count = u64_to_i64_saturating(class_sums_u64[idx]);
        map.insert(name.clone(), serde_json::Value::from(count));
    }
    // Infallible in practice (a map of String → i64 always serializes); the
    // `"{}"` fallback honors the "empty/never-fails" contract without unwrap.
    serde_json::to_string(&serde_json::Value::Object(map)).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
#[path = "activity_fold_handler_tests.rs"]
mod tests;
