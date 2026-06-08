//! Unit tests for the C5 reconstruction fallback (crt-052).
//! Split from `reconstruct.rs` to keep each source file under the 500-line
//! limit (Constraint 10 / #693). Included via `#[path]` from `reconstruct.rs`.

use super::*;
use crate::types::ObservationRecord;

fn obs(session_id: &str, ts: u64, event_type: &str) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: event_type.to_string(),
        source_domain: "claude-code".to_string(),
        session_id: session_id.to_string(),
        tool: None,
        input: None,
        response_size: None,
        response_snippet: None,
    }
}

fn obs_full(
    session_id: &str,
    ts: u64,
    tool: &str,
    input: serde_json::Value,
    snippet: &str,
) -> ObservationRecord {
    ObservationRecord {
        ts,
        event_type: "PostToolUse".to_string(),
        source_domain: "claude-code".to_string(),
        session_id: session_id.to_string(),
        tool: Some(tool.to_string()),
        input: Some(input),
        response_size: Some(snippet.len() as u64),
        response_snippet: Some(snippet.to_string()),
    }
}

// ── reconstruction (AC-07) ──────────────────────────────────────────────

#[test]
fn test_reconstruct_builds_from_observations() {
    let records = vec![obs_full(
        "s1",
        1000,
        "Edit",
        serde_json::json!({"file_path": "/tmp/x.rs"}),
        "we decided to use redb",
    )];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert_eq!(out.len(), 1, "one observation -> one candidate");
    let c = &out[0];
    assert_eq!(c.session_id, "s1");
    assert!(c.text.contains("tool: Edit"), "tool folded into text");
    assert!(c.text.contains("/tmp/x.rs"), "input folded into text");
    assert!(
        c.text.contains("we decided to use redb"),
        "snippet folded in"
    );
    assert!(c.ts.is_some(), "observation ts is the ordering key");
    assert!(
        !c.family_hints.is_empty(),
        "family_hints non-empty (C4 invariant)"
    );
    // Reconstructed input has no stream offset (documented).
    assert_eq!(c.byte_offset, 0);
}

#[test]
fn test_reconstruct_distillation_input_only() {
    // Structural / behavioral assertion of AC-07(iii): the function returns
    // owned candidates derived purely from a borrowed `&[ObservationRecord]`.
    // It takes `obs` by shared reference — it CANNOT insert/mutate rows or
    // write a byte buffer (no &mut, no buffer handle, no I/O in scope).
    let before = vec![obs("s1", 10, "PreToolUse"), obs("s1", 20, "PostToolUse")];
    let snapshot_len = before.len();
    let _ = reconstruct_from_observations("s1", &before, 24_000);
    // The input slice is unchanged (no row produced/removed): borrow-only.
    assert_eq!(
        before.len(),
        snapshot_len,
        "no observation row inserted/removed"
    );
}

#[test]
fn test_reconstruct_respects_session_cap() {
    // Three identical blocks. Size one block, then set the cap to admit
    // exactly the first two (keep-earliest) and reject the third.
    let big = "x".repeat(200);
    let records = vec![
        obs_full("s1", 1, "Read", serde_json::json!({}), &big),
        obs_full("s1", 2, "Read", serde_json::json!({}), &big),
        obs_full("s1", 3, "Read", serde_json::json!({}), &big),
    ];
    let one_block_len = compose_reconstructed_text(&records[0]).len();
    // Cap admits two blocks but not three.
    let cap = one_block_len * 2 + 1;
    let out = reconstruct_from_observations("s1", &records, cap);
    assert_eq!(out.len(), 2, "session_cap admits keep-earliest only");
    // Earliest two by ts are kept.
    assert_eq!(out[0].ts, Some(format_ts(1)));
    assert_eq!(out[1].ts, Some(format_ts(2)));
}

#[test]
fn test_reconstruct_cap_smaller_than_first_block_yields_empty() {
    // A single block larger than the cap is dropped whole (no partial block).
    let big = "z".repeat(300);
    let records = vec![obs_full("s1", 1, "Read", serde_json::json!({}), &big)];
    let out = reconstruct_from_observations("s1", &records, 10);
    assert!(out.is_empty(), "block exceeding cap is dropped whole");
}

#[test]
fn test_reconstruct_cap_zero_yields_empty() {
    let records = vec![obs_full("s1", 1, "Read", serde_json::json!({}), "decided")];
    let out = reconstruct_from_observations("s1", &records, 0);
    assert!(out.is_empty(), "zero cap admits nothing");
}

#[test]
fn test_reconstruct_empty_observations() {
    let records: Vec<ObservationRecord> = vec![];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert!(
        out.is_empty(),
        "no observations -> empty Vec (caller emits loss row)"
    );
}

#[test]
fn test_reconstruct_filters_to_session() {
    let records = vec![
        obs_full("s1", 1, "Read", serde_json::json!({}), "decided one"),
        obs_full("s2", 2, "Read", serde_json::json!({}), "decided two"),
        obs_full("s1", 3, "Read", serde_json::json!({}), "decided three"),
    ];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert_eq!(out.len(), 2, "only s1 rows reconstruct");
    assert!(out.iter().all(|c| c.session_id == "s1"));
}

#[test]
fn test_reconstruct_orders_chronologically() {
    let records = vec![
        obs_full("s1", 300, "Read", serde_json::json!({}), "decided c"),
        obs_full("s1", 100, "Read", serde_json::json!({}), "decided a"),
        obs_full("s1", 200, "Read", serde_json::json!({}), "decided b"),
    ];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    let ts: Vec<_> = out.iter().map(|c| c.ts.clone()).collect();
    assert_eq!(
        ts,
        vec![
            Some(format_ts(100)),
            Some(format_ts(200)),
            Some(format_ts(300))
        ],
        "candidates ordered by observation ts"
    );
}

// ── topic_source SOFT preference (AC-07(iv), R-14) ──────────────────────

#[test]
fn test_topic_source_rank_ordering() {
    // The rank table is the load-bearing ordering contract.
    assert!(topic_source_rank(Some("declared")) < topic_source_rank(Some("registry-fill")));
    assert!(topic_source_rank(Some("registry-fill")) < topic_source_rank(Some("extracted")));
    assert!(topic_source_rank(Some("extracted")) < topic_source_rank(Some("vote")));
    assert!(topic_source_rank(Some("vote")) < topic_source_rank(None));
    // Unknown values rank last, never panic, never excluded.
    assert_eq!(topic_source_rank(Some("bogus")), topic_source_rank(None));
}

#[test]
fn test_topic_source_drops_no_observation() {
    // R-14: every feature-matched observation contributes regardless of source.
    let records = vec![
        obs_full("s1", 1, "Read", serde_json::json!({}), "decided a"),
        obs_full("s1", 2, "Read", serde_json::json!({}), "decided b"),
        obs_full("s1", 3, "Read", serde_json::json!({}), "decided c"),
    ];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert_eq!(
        out.len(),
        records.len(),
        "no observation dropped by topic_source"
    );
}

#[test]
fn test_all_vote_observations_still_reconstruct() {
    // SR-06 banned-hard-filter regression: an all-"vote" session still
    // reconstructs (the rank function never filters).
    assert_eq!(
        topic_source_rank(Some("vote")),
        3,
        "vote ranks, not filtered"
    );
    let records = vec![
        obs_full("s1", 1, "Read", serde_json::json!({}), "voted note a"),
        obs_full("s1", 2, "Read", serde_json::json!({}), "voted note b"),
    ];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert_eq!(out.len(), 2, "all-vote session still reconstructs");
}

#[test]
fn test_topic_source_is_stable_sort_key_not_filter() {
    // With equal rank (all None today), the relative input order is preserved
    // until the explicit chronological sort — i.e. it is an ordering key, not
    // a filter. Construct rows already chronological and assert all survive.
    let records = vec![
        obs_full("s1", 10, "Read", serde_json::json!({}), "decided x"),
        obs_full("s1", 20, "Read", serde_json::json!({}), "decided y"),
    ];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].ts, Some(format_ts(10)));
    assert_eq!(out[1].ts, Some(format_ts(20)));
}

// ── fidelity floor / provenance discriminability (ADR-006) ──────────────

#[test]
fn test_reconstruct_is_degraded_label_present() {
    // The candidates carry no per-candidate provenance field (by C4 design);
    // they are tagged Reconstructed per-session by C6 via SessionLossInfo.
    // This module's contribution is producing input the handler labels
    // degraded — assert it produces well-formed candidates the handler can
    // mark. (The Reconstructed label itself is asserted in distill-handler.)
    let records = vec![obs_full("s1", 1, "Read", serde_json::json!({}), "decided")];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert_eq!(out.len(), 1);
    assert!(!out[0].family_hints.is_empty());
    // byte_offset == 0 is the reconstructed sentinel (no stream position).
    assert_eq!(out[0].byte_offset, 0);
}

// ── advisory family hints ───────────────────────────────────────────────

#[test]
fn test_family_hints_non_empty_via_event_type_fallback() {
    // No keyword in text; falls back to event-type inference.
    let records = vec![obs("s1", 1, "cycle_phase_end")];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].family_hints, vec![FamilyHint::PhaseGate]);
}

#[test]
fn test_family_hints_inferred_from_text() {
    let records = vec![obs_full(
        "s1",
        1,
        "Edit",
        serde_json::json!({}),
        "this was a rework after the regression",
    )];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    assert!(out[0].family_hints.contains(&FamilyHint::Rework));
}

#[test]
fn test_compose_text_truncates_long_snippet() {
    let long = "y".repeat(900);
    let records = vec![obs_full("s1", 1, "Read", serde_json::json!({}), &long)];
    let out = reconstruct_from_observations("s1", &records, 24_000);
    // response: prefix + at most MAX_SNIPPET_CHARS of snippet.
    let count_y = out[0].text.matches('y').count();
    assert!(
        count_y <= MAX_SNIPPET_CHARS,
        "snippet bounded to {MAX_SNIPPET_CHARS} chars"
    );
}

#[test]
fn test_truncate_chars_utf8_safe() {
    // Multi-byte chars must not split mid-codepoint.
    let s = "héllo wörld ✓✓✓";
    let t = truncate_chars(s, 3);
    assert_eq!(t, "hél");
}

#[test]
fn test_format_ts_lexical_equals_numeric() {
    assert!(
        format_ts(9) < format_ts(10),
        "zero-padded ts sorts numerically"
    );
    assert!(format_ts(100) < format_ts(1000));
}

// ── Wave-boundary (R-11) ────────────────────────────────────────────────

#[test]
fn test_reconstruct_no_transcript_hold_reference() {
    // R-11 is enforced at compile time: this module has no `use` of
    // transcript_hold.rs (a Wave-B unimatrix-server type, not even in this
    // crate). This test documents the invariant; its mere compilation in
    // unimatrix-observe — which does not depend on unimatrix-server — proves
    // the absence of any cross-wave reference.
    let records = vec![obs("s1", 1, "PreToolUse")];
    let _ = reconstruct_from_observations("s1", &records, 24_000);
}
