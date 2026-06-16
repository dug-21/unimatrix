//! `compaction_reread` + `compaction_count` reckoning (crt-055 Component 5, Wave 3).
//!
//! Two distinct numbers (ADR-005 #5047, never collapsed):
//!   - **`compaction_count`** — COUNT of `compaction_events` rows attributed to the
//!     cycle's DECLARED sessions. Sourced from the store accessor
//!     `compaction_count_for_sessions`; undeclared / evicted sessions (#4140) are not in
//!     the declared list, so their rows never mis-attribute (R-05). This reckoning is the
//!     store COUNT itself — no derivation here.
//!   - **`compaction_reread_count`** — within-session PostToolUse reads that re-touch a
//!     pre-boundary file AFTER the session's EARLIEST compaction boundary
//!     (`MIN(compacted_at)`, ADR-006), each distinct file counted once per session.
//!
//! ## The binding gate (ADR-006 #5048 — CRITICAL, R-08)
//! ```text
//! counts as a reread  IFF  (read.ts_millis ÷ 1000) > compacted_at      // FLOOR, STRICT >
//! ```
//! `compacted_at` is Unix SECONDS (producer contract, untouched). The PostToolUse read
//! `ts` is epoch MILLIS ([`ObservationRecord::ts`]) and is normalized to seconds by
//! integer-floor division (`ts / 1000`, the `session_metrics.rs:115` convention) BEFORE
//! the comparison — the READ side only, never the boundary. The comparison is STRICT `>`:
//! a read whose floored second equals the boundary does NOT count. A millis value entering
//! UNnormalized would be ~1000× the seconds boundary and make every read pass — the floor
//! is exactly what prevents that silent gate-break.
//!
//! The actual file-set-intersection is delegated to the ONE shared primitive
//! [`crate::reload_overlap::overlap_count`] via [`ReloadWindow::PostCompaction`]; this
//! module owns only the per-session `MIN(compacted_at)` boundary selection and drives the
//! primitive once per session with that session's own boundary.

use std::collections::HashMap;

use crate::reload_overlap::{ReloadWindow, overlap_count};
use crate::types::ObservationRecord;

/// Reckon `compaction_reread_count` for a cycle.
///
/// For each declared session that has a compaction boundary, drive the shared
/// post-compaction overlap window with that session's EARLIEST boundary
/// (`MIN(compacted_at)`, ADR-006) and sum the per-session overlaps. Each distinct
/// pre-boundary file re-read after the boundary is counted once per session (the
/// primitive's `already_counted` set guarantees this).
///
/// `boundaries` maps `session_id → MIN(compacted_at)` (Unix seconds), as resolved by the
/// caller from the store accessor `min_compacted_at` over the cycle's declared sessions.
/// A session absent from the map (or `None` boundary, filtered out before this call) has
/// no gate and contributes `0` — never a fabricated count. The boundary stays SECONDS;
/// the primitive normalizes the read `ts` (`ts / 1000`) internally.
///
/// `records` is the cycle's full observation set; this function partitions it per session
/// so each session is gated only against its own boundary (no cross-session bleed — the
/// post-compaction window is within-session by construction).
pub fn reckon_compaction_reread(
    records: &[ObservationRecord],
    boundaries: &HashMap<String, i64>,
) -> i64 {
    let mut total: u64 = 0;
    // Drive the shared primitive once per declared session with a boundary, passing ONLY
    // that session's records so the within-session window cannot see another session's
    // reads. `summaries` is unused by the PostCompaction window, so an empty slice is fine.
    for (session_id, &boundary_secs) in boundaries {
        let session_records: Vec<ObservationRecord> = records
            .iter()
            .filter(|r| &r.session_id == session_id)
            .cloned()
            .collect();
        let counts = overlap_count(
            &session_records,
            ReloadWindow::PostCompaction { boundary_secs },
            &[],
        );
        total = total.saturating_add(counts.overlap);
    }
    total as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read(session_id: &str, ts_millis: u64, file_path: &str) -> ObservationRecord {
        ObservationRecord {
            ts: ts_millis,
            event_type: "PostToolUse".to_string(),
            source_domain: "claude-code".to_string(),
            session_id: session_id.to_string(),
            tool: Some("Read".to_string()),
            input: Some(json!({ "file_path": file_path })),
            response_size: None,
            response_snippet: None,
        }
    }

    fn boundary_map(pairs: &[(&str, i64)]) -> HashMap<String, i64> {
        pairs.iter().map(|(s, b)| (s.to_string(), *b)).collect()
    }

    // ---- AC-22 canonical gate: floor + STRICT > (expected count = 1) ----

    #[test]
    fn test_gate_canonical_floor_strict_after_counts_one() {
        // compacted_at = T (seconds). ONE distinct pre-boundary file re-read three times
        // after compaction at: exact boundary, -500ms, +1s. Only +1s clears floor+strict>.
        let t: i64 = 1_000; // seconds
        let t_millis = (t as u64) * 1000;
        let records = vec![
            // prior read at/before the boundary establishes the file in the prior set.
            read("s1", t_millis - 2000, "/a.rs"), // T-2s, clearly prior
            // exact boundary: ts = T*1000 → floor T → T > T false → NOT counted
            read("s1", t_millis, "/a.rs"),
            // -500ms: ts = T*1000-500 → floor T-1 → T-1 > T false → NOT counted (floor guard)
            read("s1", t_millis - 500, "/a.rs"),
            // +1s: ts = T*1000+1000 → floor T+1 → T+1 > T true → COUNTS
            read("s1", t_millis + 1000, "/a.rs"),
        ];
        let boundaries = boundary_map(&[("s1", t)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 1);
    }

    #[test]
    fn test_gate_normalizes_read_ts_millis_to_seconds() {
        // ts_millis = T*1000 + 999 floors to T (NOT counted, strict >);
        // (T+1)*1000 floors to T+1 (counted). Distinguishes floor from rounding.
        let t: i64 = 50;
        let records = vec![
            read("s1", (t as u64) * 1000 - 5000, "/f.rs"), // prior
            read("s1", (t as u64) * 1000 + 999, "/f.rs"),  // floor T → not counted
            read("s1", ((t + 1) as u64) * 1000, "/f.rs"),  // floor T+1 → counted
        ];
        let boundaries = boundary_map(&[("s1", t)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 1);
    }

    #[test]
    fn test_gate_unnormalized_millis_would_overcount_floor_prevents() {
        // Load-bearing: a -500ms read (ts = T*1000-500) compared RAW against seconds T
        // would be T*1000-500 > T = true (wrong, ~1000x). The ÷1000 floor (→ T-1) makes it
        // NOT count. This asserts the normalization is present.
        let t: i64 = 1_000;
        let records = vec![
            read("s1", (t as u64) * 1000 - 3000, "/a.rs"), // prior
            read("s1", (t as u64) * 1000 - 500, "/a.rs"),  // -500ms → floor T-1 → not counted
        ];
        let boundaries = boundary_map(&[("s1", t)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 0);
    }

    #[test]
    fn test_gate_strictly_after_equal_not_counted() {
        // read_ts_secs == compacted_at → NOT counted (strict >, not >=).
        let t: i64 = 200;
        let records = vec![
            read("s1", (t as u64) * 1000 - 1000, "/a.rs"), // prior
            read("s1", (t as u64) * 1000, "/a.rs"),        // exactly T → not counted
        ];
        let boundaries = boundary_map(&[("s1", t)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 0);
    }

    #[test]
    fn test_gate_before_boundary_not_counted() {
        // A read entirely before the boundary is prior context, not a reread.
        let t: i64 = 200;
        let records = vec![
            read("s1", (t as u64) * 1000 - 5000, "/a.rs"),
            read("s1", (t as u64) * 1000 - 1000, "/a.rs"),
        ];
        let boundaries = boundary_map(&[("s1", t)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 0);
    }

    // ---- AC-12: boundary selection / each read once ----

    #[test]
    fn test_reread_counted_at_most_once_per_session() {
        // A file re-read MANY times after the boundary counts ONCE per session.
        let t: i64 = 100;
        let records = vec![
            read("s1", (t as u64) * 1000 - 5000, "/a.rs"), // prior
            read("s1", (t as u64) * 1000 + 2000, "/a.rs"), // reread
            read("s1", (t as u64) * 1000 + 3000, "/a.rs"), // reread again
            read("s1", (t as u64) * 1000 + 4000, "/a.rs"), // reread again
        ];
        let boundaries = boundary_map(&[("s1", t)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 1);
    }

    #[test]
    fn test_multi_compaction_gates_on_earliest_min() {
        // Multi-compaction session: caller passes MIN(compacted_at) as the boundary.
        // A read after the EARLIEST boundary (but it would be before a later one) counts.
        let min_boundary: i64 = 100; // MIN of {100, 500}
        let records = vec![
            read("s1", 90_000, "/a.rs"),  // 90s, prior to MIN 100s
            read("s1", 300_000, "/a.rs"), // 300s, after MIN 100s → counts (would miss if gated on 500)
        ];
        let boundaries = boundary_map(&[("s1", min_boundary)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 1);
    }

    #[test]
    fn test_per_session_boundaries_no_cross_session_bleed() {
        // Each session gated on its OWN boundary; reads do not bleed across sessions.
        // s1 re-reads after its boundary (counts); s2's file is distinct + only read once.
        let records = vec![
            read("s1", 10_000, "/a.rs"), // prior (10s, before s1 boundary 50s)
            read("s1", 60_000, "/a.rs"), // 60s > 50s → counts
            read("s2", 70_000, "/b.rs"), // single read, no prior → no reread
        ];
        let boundaries = boundary_map(&[("s1", 50), ("s2", 50)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 1);
    }

    #[test]
    fn test_session_without_boundary_contributes_zero() {
        // A session absent from the boundary map (no MIN boundary) is never gated → 0.
        let records = vec![
            read("s1", 10_000, "/a.rs"),
            read("s1", 60_000, "/a.rs"), // would be a reread IF gated, but no boundary
        ];
        let boundaries: HashMap<String, i64> = HashMap::new();
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 0);
    }

    #[test]
    fn test_compacts_but_never_rereads_is_zero() {
        // Session compacts but never re-reads a pre-boundary file → measured 0
        // (compaction_count > 0 lives in the store accessor, distinct from this).
        let t: i64 = 100;
        let records = vec![
            read("s1", (t as u64) * 1000 - 1000, "/a.rs"), // prior
            read("s1", (t as u64) * 1000 + 5000, "/b.rs"), // post, but a DIFFERENT file
        ];
        let boundaries = boundary_map(&[("s1", t)]);
        assert_eq!(reckon_compaction_reread(&records, &boundaries), 0);
    }
}
