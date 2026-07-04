//! Unit tests for `distill_scope` (crt-057). Split out per the crate's
//! `_tests.rs` convention to keep the production module under the 500-line limit.

use super::*;
use unimatrix_observe::{
    EvidenceRecord, FamilyHint, HotspotCategory, HotspotFinding, SessionLossInfo, Severity,
};

fn cand(session: &str, offset: u64, ts: Option<&str>, text: &str) -> TranscriptCandidate {
    TranscriptCandidate {
        session_id: session.to_string(),
        byte_offset: offset,
        ts: ts.map(|s| s.to_string()),
        family_hints: vec![FamilyHint::Decision],
        text: text.to_string(),
    }
}

fn finding_with_evidence_ts(ts_values: &[u64]) -> HotspotFinding {
    HotspotFinding {
        category: HotspotCategory::Friction,
        severity: Severity::Warning,
        rule_name: "test_rule".to_string(),
        claim: "test claim".to_string(),
        measured: 1.0,
        threshold: 0.5,
        evidence: ts_values
            .iter()
            .map(|&ts| EvidenceRecord {
                description: "ev".to_string(),
                ts,
                tool: None,
                detail: "d".to_string(),
            })
            .collect(),
    }
}

// ── anchor resolution (R-09: finding id → evidence-ts span) ──────────────

#[test]
fn test_resolve_anchor_bounds_spans_evidence_ts() {
    // F-02 selects hotspots[1]; its evidence ts span is [min, max].
    let hotspots = vec![
        finding_with_evidence_ts(&[1000]),
        finding_with_evidence_ts(&[5000, 9000, 7000]),
    ];
    let rb = resolve_anchor_bounds("F-02", &hotspots).expect("F-02 resolves");
    assert_eq!(rb.kind, BoundsKind::Anchor);
    assert_eq!(rb.lo_epoch_ms, 5000);
    assert_eq!(rb.hi_epoch_ms, 9000);
    // Accepts bare/short forms too.
    assert_eq!(
        resolve_anchor_bounds("2", &hotspots).unwrap().lo_epoch_ms,
        5000
    );
    assert_eq!(
        resolve_anchor_bounds("F-2", &hotspots).unwrap().hi_epoch_ms,
        9000
    );
}

#[test]
fn test_resolve_anchor_bounds_unknown_or_empty_is_none() {
    let hotspots = vec![finding_with_evidence_ts(&[1000])];
    assert!(
        resolve_anchor_bounds("F-99", &hotspots).is_none(),
        "out of range → None"
    );
    assert!(
        resolve_anchor_bounds("F-00", &hotspots).is_none(),
        "F-00 invalid (1-based) → None"
    );
    assert!(
        resolve_anchor_bounds("not-an-id", &hotspots).is_none(),
        "unparseable → None"
    );
    // Finding with no evidence → unresolvable.
    let no_ev = vec![finding_with_evidence_ts(&[])];
    assert!(
        resolve_anchor_bounds("F-01", &no_ev).is_none(),
        "no evidence → None"
    );
    assert!(
        resolve_anchor_bounds("F-01", &[]).is_none(),
        "empty hotspots → None"
    );
}

// ── clock normalization (R-05, explicit fixed offsets, never now_ts()) ──

#[test]
fn test_parse_iso8601_basic_utc_to_epoch_ms() {
    // 2026-06-08T10:00:00Z — cross-checked against a fixed known epoch.
    let ms = parse_iso8601_to_epoch_ms("2026-06-08T10:00:00Z").expect("parse");
    assert_eq!(ms, 1_780_912_800_000);
    // Relative sanity: +1 day and +1 hour advance by the exact deltas.
    let next_day = parse_iso8601_to_epoch_ms("2026-06-09T10:00:00Z").expect("parse");
    assert_eq!(next_day - ms, 86_400_000);
    let plus_hour = parse_iso8601_to_epoch_ms("2026-06-08T11:00:00Z").expect("parse");
    assert_eq!(plus_hour - ms, 3_600_000);
}

#[test]
fn test_parse_iso8601_fractional_and_offset() {
    let z = parse_iso8601_to_epoch_ms("2026-06-08T10:00:00.500Z").expect("frac");
    let base = parse_iso8601_to_epoch_ms("2026-06-08T10:00:00Z").expect("base");
    assert_eq!(z - base, 500, "fractional seconds add milliseconds");
    // +02:00 offset means the same wall clock is 2h EARLIER in UTC.
    let plus = parse_iso8601_to_epoch_ms("2026-06-08T10:00:00+02:00").expect("tz");
    assert_eq!(
        base - plus,
        2 * 3600 * 1000,
        "offset subtracts to reach UTC"
    );
}

#[test]
fn test_candidate_epoch_ms_none_and_malformed() {
    assert_eq!(candidate_epoch_ms(&None), None);
    assert_eq!(
        candidate_epoch_ms(&Some("not-a-timestamp".to_string())),
        None
    );
    assert!(candidate_epoch_ms(&Some("2026-06-08T10:00:00Z".to_string())).is_some());
}

#[test]
fn test_epoch_boundary_triple_inside_on_outside() {
    // Window [lo, hi] padded by millis: just-inside / on / just-outside.
    let bounds = (1_000_000u64, 2_000_000u64);
    let millis = 120_000u64; // ±2 min default
    // hi + millis = 2_120_000 is the boundary.
    assert!(
        ts_within_window(bounds, millis, 2_119_999),
        "just-inside → in"
    );
    assert!(
        ts_within_window(bounds, millis, 2_120_000),
        "on boundary → in"
    );
    assert!(
        !ts_within_window(bounds, millis, 2_120_001),
        "just-outside → out"
    );
    // low side saturating.
    assert!(
        ts_within_window(bounds, millis, 880_000),
        "lo − millis on boundary → in"
    );
    assert!(
        !ts_within_window(bounds, millis, 879_999),
        "below lo − millis → out"
    );
}

#[test]
fn test_skewed_plane_b_ts_resolved_via_window_not_exact() {
    // Anchor at Plane-A T; candidate JSONL ts skewed +90s (< ±120s window).
    let anchor = parse_iso8601_to_epoch_ms("2026-06-08T10:00:00Z").expect("anchor");
    let skewed = parse_iso8601_to_epoch_ms("2026-06-08T10:01:30Z").expect("skewed");
    let bounds = (anchor, anchor);
    // Exact-match join would MISS it (skewed != anchor); windowed finds it.
    assert_ne!(skewed, anchor, "an exact join would miss the skew");
    assert!(
        ts_within_window(bounds, 120_000, skewed),
        "windowed join selects the skewed candidate"
    );
}

#[test]
fn test_phase_contains_is_self_bounding_no_window() {
    // phase [start, end]; a candidate 1ms past end is OUT even though a window
    // would have padded it — phase ignores window (R-09 sc.4).
    let bounds = (1_000_000u64, 2_000_000u64);
    assert!(phase_contains_ts(bounds, 2_000_000), "on end → in");
    assert!(
        !phase_contains_ts(bounds, 2_000_001),
        "past end → out (no window pad)"
    );
}

#[test]
fn test_block_within_ts_none_byte_fallback() {
    // in-window ts-bearing candidates cover block indices [4, 6]; ±3 blocks.
    assert!(block_within(1, 4, 6, 3), "idx 1 = 4−3 boundary → in");
    assert!(!block_within(0, 4, 6, 3), "idx 0 outside ±3 → out");
    assert!(block_within(9, 4, 6, 3), "idx 9 = 6+3 boundary → in");
    assert!(!block_within(10, 4, 6, 3), "idx 10 outside ±3 → out");
}

// ── regex compilation / validation (R-09 security) ──────────────────────

#[test]
fn test_validate_scope_regex_ok_and_error() {
    let ok = TranscriptScope {
        phase: None,
        anchor: None,
        r#match: Some("decid(e|ed)".to_string()),
        window: None,
    };
    assert!(validate_scope_regex(Some(&ok)).is_ok());

    let bad = TranscriptScope {
        phase: None,
        anchor: None,
        r#match: Some("(".to_string()),
        window: None,
    };
    let err = validate_scope_regex(Some(&bad)).expect_err("invalid regex must error");
    assert_eq!(err.code, crate::error::ERROR_INVALID_PARAMS);

    // Absent scope / absent match → Ok (no-op).
    assert!(validate_scope_regex(None).is_ok());
}

// ── derive_search_status (R-01 matrix — the central coverage) ────────────

fn section_with(
    candidates: Vec<TranscriptCandidate>,
    loss: Vec<SessionLossInfo>,
) -> TranscriptCandidatesSection {
    TranscriptCandidatesSection { candidates, loss }
}

fn loss_row(
    sid: &str,
    elided: u64,
    holes: bool,
    prov: CandidateProvenance,
    dropped: u64,
) -> SessionLossInfo {
    SessionLossInfo {
        session_id: sid.to_string(),
        elided_bytes: elided,
        has_holes: holes,
        provenance: prov,
        dropped_candidates: dropped,
    }
}

fn match_scope() -> TranscriptScope {
    TranscriptScope {
        phase: None,
        anchor: None,
        r#match: Some(".*".to_string()),
        window: None,
    }
}

#[test]
fn test_search_complete_false_per_single_loss_condition() {
    // Rows a–d: exactly one loss signal each ⇒ search_complete == false.
    let cases = [
        (
            "a",
            loss_row("a", 42, false, CandidateProvenance::Primary, 0),
        ),
        ("b", loss_row("b", 0, true, CandidateProvenance::Primary, 0)),
        (
            "c",
            loss_row("c", 0, false, CandidateProvenance::Reconstructed, 0),
        ),
        (
            "d",
            loss_row("d", 0, false, CandidateProvenance::Primary, 5),
        ),
    ];
    for (sid, lr) in cases {
        // No-match: session appears only via its loss row.
        let section = section_with(vec![], vec![lr]);
        let (rows, bounds) = derive_search_status(Some(&section), Some(&match_scope()));
        assert!(bounds.is_none());
        assert_eq!(rows.len(), 1, "{sid}: one status row");
        assert_eq!(rows[0].session_id, sid);
        assert_eq!(
            rows[0].matched,
            Some(false),
            "{sid}: no-match under a match scope"
        );
        assert!(
            !rows[0].search_complete,
            "{sid}: lossy ⇒ search_complete false"
        );
    }
}

#[test]
fn test_search_complete_false_on_combined_loss_or_not_and() {
    // Row e: two signals at once ⇒ still false (OR, not AND).
    let section = section_with(
        vec![],
        vec![loss_row(
            "e",
            100,
            true,
            CandidateProvenance::Reconstructed,
            3,
        )],
    );
    let (rows, _) = derive_search_status(Some(&section), Some(&match_scope()));
    assert_eq!(rows.len(), 1);
    assert!(
        !rows[0].search_complete,
        "OR-combination is still incomplete"
    );
    assert_eq!(rows[0].elided_bytes, 100, "elided surfaced");
    assert_eq!(rows[0].provenance, CandidateProvenance::Reconstructed);
}

#[test]
fn test_clean_primary_nomatch_is_trustworthy_negative() {
    // Row f: a clean Primary session WITH a candidate, no loss row ⇒ true.
    let section = section_with(
        vec![cand("f", 0, Some("2026-06-08T10:00:00Z"), "hello")],
        vec![],
    );
    let (rows, _) = derive_search_status(Some(&section), Some(&match_scope()));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].session_id, "f");
    assert!(
        rows[0].search_complete,
        "clean primary ⇒ trustworthy negative"
    );
    assert_eq!(
        rows[0].matched,
        Some(true),
        "candidate present ⇒ matched true"
    );
}

#[test]
fn test_match_never_collapses_to_bare_boolean() {
    // Every returned session in a match result carries a SessionLossInfo OR a
    // candidate; a lossy no-match session is present with its loss row.
    let section = section_with(
        vec![cand("clean", 0, Some("2026-06-08T10:00:00Z"), "x")],
        vec![loss_row("lossy", 9, false, CandidateProvenance::Primary, 0)],
    );
    let (rows, _) = derive_search_status(Some(&section), Some(&match_scope()));
    // Two sessions surfaced: the matched clean one + the lossy no-match one.
    assert_eq!(rows.len(), 2);
    let lossy = rows
        .iter()
        .find(|r| r.session_id == "lossy")
        .expect("lossy present");
    assert_eq!(lossy.matched, Some(false));
    assert!(
        !lossy.search_complete,
        "lossy no-match is INDETERMINATE, never a bare false"
    );
}

#[test]
fn test_loss_row_present_on_match_hit_too() {
    // A positive match over a lossy session STILL surfaces its loss row.
    let section = section_with(
        vec![cand("hit", 0, Some("2026-06-08T10:00:00Z"), "decided")],
        vec![loss_row("hit", 500, false, CandidateProvenance::Primary, 0)],
    );
    let (rows, _) = derive_search_status(Some(&section), Some(&match_scope()));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].matched, Some(true), "matched");
    assert!(
        !rows[0].search_complete,
        "but incomplete — loss carried on the hit"
    );
    assert_eq!(rows[0].elided_bytes, 500);
}

#[test]
fn test_anchor_only_scope_matched_is_none() {
    // No `match` supplied ⇒ `matched` is None (no regex verdict).
    let anchor_scope = TranscriptScope {
        phase: None,
        anchor: Some("F-03".to_string()),
        r#match: None,
        window: None,
    };
    let section = section_with(
        vec![cand("s", 0, Some("2026-06-08T10:00:00Z"), "x")],
        vec![],
    );
    let (rows, _) = derive_search_status(Some(&section), Some(&anchor_scope));
    assert_eq!(
        rows[0].matched, None,
        "anchor-only scope has no regex verdict"
    );
}

#[test]
fn test_derive_search_status_none_inputs() {
    assert_eq!(derive_search_status(None, Some(&match_scope())).0.len(), 0);
    let s = section_with(vec![], vec![]);
    assert_eq!(derive_search_status(Some(&s), None).0.len(), 0);
}
