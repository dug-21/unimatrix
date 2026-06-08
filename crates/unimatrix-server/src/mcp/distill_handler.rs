//! crt-052 C6 — Distill helper / handler glue (Wave A).
//!
//! One shared helper invoked at ALL FOUR `context_cycle_review` `result.is_ok()`
//! success returns, immediately BEFORE `purge_cycle_transcripts`. It orchestrates:
//!
//! 1. the exhaustive `TranscriptRetention` gate (ADR-005 / AC-10),
//! 2. the off-lock snapshot seam `take_transcripts_for_feature` (C1 / ADR-001),
//! 3. per-session Primary [`select_candidates`] (C3) vs Reconstructed
//!    [`reconstruct_from_observations`] (C5) via the shared fallback predicate
//!    (ADR-006),
//! 4. the per-cycle aggregate cap (deterministic chronological keep-earliest,
//!    R-15) plus per-session [`SessionLossInfo`] assembly (ADR-007), and
//! 5. assembly-level attach of the section to the response, strictly OUTSIDE the
//!    memoized `RetrospectiveReport` (ADR-004 / AC-06).
//!
//! **Wave A invariant (R-11):** this module has ZERO compile-time reference to
//! `transcript_hold.rs`. Held buffers (Wave B) arrive transparently through the
//! C1 seam; this helper never touches the hold store.
//!
//! **Secrets posture (AC-06):** candidates are response-transient. They are
//! NEVER written onto `RetrospectiveReport` (the persisted type has no slot), so
//! the memoization persist (`store_cycle_review` → `cycle_review_index`, #3793)
//! structurally cannot carry them. [`attach_to_response_assembly`] adds them to
//! the already-built `CallToolResult` AFTER the report is computed and memoized.

use rmcp::model::CallToolResult;
use unimatrix_observe::distill::{reconstruct_from_observations, select_candidates};
use unimatrix_observe::{
    CandidateProvenance, ObservationRecord, SessionLossInfo, TranscriptCandidate,
    TranscriptCandidatesSection,
};

use crate::infra::config::{RetentionConfig, TranscriptRetention};
use crate::infra::session::SessionRegistry;
use crate::infra::session_transcript::TranscriptSnapshot;

/// Distill transcript candidates for `feature_cycle` immediately BEFORE purge.
///
/// Returns `Some(section)` when at least one candidate OR one loss row is worth
/// reporting; `None` when there is nothing to surface (AC-04 — the response field
/// is then omitted entirely). Never panics: C1 is infallible, C3/C5 are total on
/// untrusted input (R-10), so a fully-corrupt snapshot degrades to zero
/// candidates rather than an error (AC-V-FUZZ handler level).
///
/// The four call sites pass the registry, the reviewed `feature_cycle`, the
/// ALREADY-loaded observation set (do NOT re-query — ADR-005), and the retention
/// config. All parsing happens after the seam returns; no lock is held here.
pub(crate) fn distill_before_purge(
    registry: &SessionRegistry,
    feature_cycle: &str,
    observations: &[ObservationRecord],
    cfg: &RetentionConfig,
) -> Option<TranscriptCandidatesSection> {
    // (1) EXHAUSTIVE retention gate (C7 / ADR-005 / AC-10). NO wildcard arm:
    //     adding a third `TranscriptRetention` variant MUST be a compile error
    //     (AC-10). Variant semantics are kept identical to C7's parallel match
    //     in `server.rs::purge_cycle_transcripts`.
    match cfg.transcript_retention {
        TranscriptRetention::PurgeOnCycleClose => {} // proceed
        TranscriptRetention::RetainDays(_) => return None, // neither distill nor purge
    }

    // (2) Snapshot off-lock (C1 / ADR-001). Returns registered ∪ held (Wave B)
    //     for the cycle. ALL parsing happens AFTER this returns; no lock held.
    let snapshots = registry.take_transcripts_for_feature(feature_cycle);

    let session_cap = cfg.transcript_candidate_session_cap_bytes;
    let mut all_candidates: Vec<TranscriptCandidate> = Vec::new();
    let mut loss: Vec<SessionLossInfo> = Vec::new();

    // (3) Per-session: Primary (C3) or Reconstructed (C5) via the SHARED
    //     fallback predicate (ADR-006). Whole-session either/or — never a
    //     byte-level mix within one session (OQ-2).
    for (session_id, snap) in snapshots {
        let is_fallback = fallback_triggered(&snap, cfg.transcript_fallback_hole_fraction);

        let (session_cands, provenance, dropped) = if is_fallback {
            // R-16: poison-recovered (treat-as-empty) snapshot lands here too —
            // the session still surfaces in `loss`, never silently absent.
            let recon = reconstruct_from_observations(&session_id, observations, session_cap);
            // C5 applies the per-session cap internally; aggregate-cap drops are
            // folded in below. Session-cap-drop accounting for the reconstructed
            // path is left to C5's own keep-earliest, surfaced as 0 here.
            (recon, CandidateProvenance::Reconstructed, 0u64)
        } else {
            // R-09 corner: non-empty bytes with ZERO marker matches is still
            // Primary-with-zero-candidates, NOT a fallback — only the predicate
            // decides fallback.
            let primary =
                select_candidates(&snap.bytes, &session_id, snap.base_offset, session_cap);
            let dropped = count_dropped_by_session_cap(&snap, &session_id, session_cap);
            (primary, CandidateProvenance::Primary, dropped)
        };

        all_candidates.extend(session_cands);

        // (4a) Per-session SessionLossInfo (ADR-007 / AC-08). The Primary/
        //      Reconstructed label is derived from the SAME predicate result —
        //      not recomputed (ADR-007 warning). A clean Primary session with no
        //      loss and no cap-drop is OMITTED (silence == nothing to report).
        let has_holes = !snap.holes.is_empty();
        if snap.elided_bytes > 0
            || has_holes
            || provenance == CandidateProvenance::Reconstructed
            || dropped > 0
        {
            loss.push(SessionLossInfo {
                session_id,
                elided_bytes: snap.elided_bytes,
                has_holes,
                provenance,
                dropped_candidates: dropped,
            });
        }
    }

    // (4b) Order the cross-session UNION deterministically (R-15) by
    //      (ts, session_id, byte_offset) — same key C3 uses per session.
    sort_candidates_chronological(&mut all_candidates);

    // (4c) PER-CYCLE aggregate cap (ADR-005 §4 / FR-4). Deterministic
    //      chronological KEEP-EARLIEST; repeatable across runs (R-15).
    let cycle_dropped_by_session = keep_earliest_within_cycle(
        &mut all_candidates,
        cfg.transcript_candidate_cycle_cap_bytes,
    );

    // AC-08: fold aggregate-cap drops into `loss` so no aggregate-cap drop is
    // silent — ensure a row exists per affected session and add to its count.
    merge_cycle_drops_into_loss(&mut loss, &cycle_dropped_by_session);

    // (5) Absent-when-empty (AC-04): nothing to report → None → field omitted.
    if all_candidates.is_empty() && loss.is_empty() {
        return None;
    }
    Some(TranscriptCandidatesSection {
        candidates: all_candidates,
        loss,
    })
}

/// Shared fallback predicate (ADR-006). A session falls back to reconstruction
/// when its snapshot is EMPTY (no readable bytes — empty buffer, poison-recovery,
/// or all-elided) OR its hole/elision loss exceeds the configured threshold,
/// expressed against ADR-002 tail-window-equivalence (NOT assumed losslessness,
/// SR-08).
///
/// This is the ONE predicate both the Primary/Reconstructed routing AND the
/// per-session provenance label derive from (ADR-007 warning — no recomputation).
fn fallback_triggered(snap: &TranscriptSnapshot, hole_fraction_threshold: f64) -> bool {
    // Empty readable span → nothing for the primary path to select.
    if snap.bytes.is_empty() {
        return true;
    }
    // Ring-tail clipping advanced base_offset and dropped head deltas: the
    // primary window is no longer tail-window-equivalent to the full session.
    if snap.elided_bytes > 0 {
        return true;
    }
    // Holes covering MORE than the configured fraction of the logical span.
    // Span is measured from base_offset to high_water (the logical stream
    // extent), consistent with ADR-002 metadata semantics.
    let span = snap.high_water.saturating_sub(snap.base_offset);
    if span == 0 {
        // No logical extent but non-empty bytes: treat as primary (let selection
        // run); the empty-bytes case above already handled true emptiness.
        return false;
    }
    let hole_bytes: u64 = snap
        .holes
        .iter()
        .map(|h| h.end.saturating_sub(h.start))
        .sum();
    let fraction = hole_bytes as f64 / span as f64;
    fraction > hole_fraction_threshold
}

/// Count candidates dropped to the PER-SESSION cap on the Primary path (AC-08).
///
/// C3 returns the already-capped vec; to surface the silent per-session drop we
/// re-select uncapped (cap = `usize::MAX`) and diff the counts. Pure and total
/// (same untrusted-input hardening as the capped call), so this never panics.
fn count_dropped_by_session_cap(
    snap: &TranscriptSnapshot,
    session_id: &str,
    session_cap: usize,
) -> u64 {
    let uncapped = select_candidates(&snap.bytes, session_id, snap.base_offset, usize::MAX).len();
    let capped = select_candidates(&snap.bytes, session_id, snap.base_offset, session_cap).len();
    uncapped.saturating_sub(capped) as u64
}

/// Deterministic ordering of the cross-session candidate union by
/// `(ts, session_id, byte_offset)` (R-15). `None` timestamps sort last but
/// stably, matching C3's per-session key shape.
fn sort_candidates_chronological(candidates: &mut [TranscriptCandidate]) {
    candidates.sort_by(|a, b| {
        ts_sort_key(&a.ts)
            .cmp(&ts_sort_key(&b.ts))
            .then_with(|| a.session_id.cmp(&b.session_id))
            .then_with(|| a.byte_offset.cmp(&b.byte_offset))
    });
}

/// Ordering key for an optional timestamp: present timestamps sort before
/// absent ones, lexically among themselves (ISO-8601 sorts chronologically).
fn ts_sort_key(ts: &Option<String>) -> (bool, &str) {
    match ts {
        Some(s) => (false, s.as_str()),
        None => (true, ""),
    }
}

/// Enforce the per-cycle aggregate BYTE cap with deterministic chronological
/// keep-earliest truncation (brief "truncation order" pin; R-15). Mutates
/// `candidates` in place to the kept prefix and returns a per-session count of
/// the candidates dropped to the aggregate cap (AC-08 — no silent drop).
///
/// Byte accounting mirrors C3's per-session cap exactly: each candidate costs
/// `text.len()` bytes; inclusion stops once the cap WOULD be exceeded
/// (keep-earliest). The candidates must already be chronologically ordered.
fn keep_earliest_within_cycle(
    candidates: &mut Vec<TranscriptCandidate>,
    cycle_cap: usize,
) -> Vec<(String, u64)> {
    let mut total: usize = 0;
    let mut keep_count = 0usize;
    for c in candidates.iter() {
        let next = total.saturating_add(c.text.len());
        if next > cycle_cap {
            break;
        }
        total = next;
        keep_count += 1;
    }

    // Tally dropped-per-session over the truncated tail BEFORE we drop it.
    let mut dropped: Vec<(String, u64)> = Vec::new();
    for c in &candidates[keep_count..] {
        match dropped.iter_mut().find(|(sid, _)| sid == &c.session_id) {
            Some((_, n)) => *n += 1,
            None => dropped.push((c.session_id.clone(), 1)),
        }
    }

    candidates.truncate(keep_count);
    dropped
}

/// Fold per-cycle aggregate-cap drops into the loss rows (AC-08). For each
/// affected session, ensure a [`SessionLossInfo`] row exists and add the dropped
/// count to it. A session that was clean-Primary but lost candidates to the
/// aggregate cap gets a fresh `Primary` loss row so the drop is never invisible.
fn merge_cycle_drops_into_loss(loss: &mut Vec<SessionLossInfo>, dropped: &[(String, u64)]) {
    for (session_id, n) in dropped {
        match loss.iter_mut().find(|l| &l.session_id == session_id) {
            Some(row) => row.dropped_candidates = row.dropped_candidates.saturating_add(*n),
            None => loss.push(SessionLossInfo {
                session_id: session_id.clone(),
                elided_bytes: 0,
                has_holes: false,
                provenance: CandidateProvenance::Primary,
                dropped_candidates: *n,
            }),
        }
    }
}

/// Attach the distilled section to the response at ASSEMBLY level (ADR-004 /
/// AC-06 — secrets-critical).
///
/// The section is appended as an additive JSON text content item on the
/// already-built `CallToolResult`, AFTER the `RetrospectiveReport` has been
/// computed and memoized via `store_cycle_review()` (#3793). It is NEVER written
/// onto `RetrospectiveReport` — the persisted type has no candidate field, so the
/// leak is structurally impossible (ADR-004). `None` → the response is left
/// byte-identical to pre-crt-052 (AC-04 / golden-output).
///
/// No candidate or buffer content reaches any SQL write, file write, or log line:
/// this function only mutates the in-flight response value (AC-06 content-leak).
pub(crate) fn attach_to_response_assembly(
    result: &mut Result<CallToolResult, rmcp::model::ErrorData>,
    section: Option<TranscriptCandidatesSection>,
) {
    // Error paths keep transcripts and produce no candidates (AC-05) — and the
    // helper is never called on them; defensively, attach nothing on Err.
    let (Ok(call_result), Some(section)) = (result.as_mut(), section) else {
        return;
    };
    // Serialize the response-transient section as a tagged JSON text item. On the
    // (unreachable in practice) serialization failure, emit nothing rather than
    // risk a partial/garbled content item — the review response stays valid.
    if let Ok(json) = serde_json::to_string(&section) {
        call_result.content.push(rmcp::model::Content::text(format!(
            "\ntranscript_candidates: {json}"
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::session_transcript::HoleInfo;
    use unimatrix_observe::FamilyHint;

    fn snap(
        bytes: &[u8],
        base: u64,
        hw: u64,
        elided: u64,
        holes: Vec<(u64, u64)>,
    ) -> TranscriptSnapshot {
        TranscriptSnapshot {
            bytes: bytes.to_vec(),
            base_offset: base,
            high_water: hw,
            elided_bytes: elided,
            holes: holes
                .into_iter()
                .map(|(start, end)| HoleInfo { start, end })
                .collect(),
        }
    }

    fn cand(session: &str, offset: u64, ts: Option<&str>, text: &str) -> TranscriptCandidate {
        TranscriptCandidate {
            session_id: session.to_string(),
            byte_offset: offset,
            ts: ts.map(|s| s.to_string()),
            family_hints: vec![FamilyHint::Decision],
            text: text.to_string(),
        }
    }

    // ── fallback predicate (AC-07 (i)(ii), R-09) ────────────────────────────

    #[test]
    fn test_trigger_empty_snapshot_falls_back() {
        // Empty readable span → reconstruct.
        assert!(fallback_triggered(&snap(b"", 0, 0, 0, vec![]), 0.5));
    }

    #[test]
    fn test_trigger_elided_above_threshold_falls_back() {
        // Any elision (ring-tail clipping) → fallback, against tail-window
        // equivalence, not assumed losslessness (SR-08, #4764).
        assert!(fallback_triggered(
            &snap(b"some bytes", 100, 200, 1, vec![]),
            0.5
        ));
    }

    #[test]
    fn test_trigger_holes_fraction_boundary() {
        // span = 100; holes = 60 → fraction 0.6 > 0.5 → fallback.
        assert!(fallback_triggered(
            &snap(b"x", 0, 100, 0, vec![(0, 60)]),
            0.5
        ));
        // holes = 40 → fraction 0.4 < 0.5 → primary.
        assert!(!fallback_triggered(
            &snap(b"x", 0, 100, 0, vec![(0, 40)]),
            0.5
        ));
        // exactly at the edge (0.5) is NOT > 0.5 → primary (boundary is strict).
        assert!(!fallback_triggered(
            &snap(b"x", 0, 100, 0, vec![(0, 50)]),
            0.5
        ));
    }

    #[test]
    fn test_trigger_nonempty_no_loss_is_primary() {
        // R-09: non-empty bytes, no elision, no holes → primary regardless of
        // whether any marker matches (that is selection's concern, not fallback).
        assert!(!fallback_triggered(
            &snap(b"hello world", 0, 11, 0, vec![]),
            0.5
        ));
    }

    // ── per-cycle aggregate cap (AC-02, AC-08, R-15) ────────────────────────

    #[test]
    fn test_cycle_cap_truncation_chronological_keep_earliest_repeatable() {
        let build = || {
            vec![
                cand("a", 0, Some("2026-01-01T00:00:00Z"), "0123456789"), // 10 bytes
                cand("b", 0, Some("2026-01-02T00:00:00Z"), "0123456789"), // 10 bytes
                cand("c", 0, Some("2026-01-03T00:00:00Z"), "0123456789"), // 10 bytes
            ]
        };
        // cap = 20 bytes → keep first two (earliest), drop the third.
        let mut run1 = build();
        sort_candidates_chronological(&mut run1);
        let d1 = keep_earliest_within_cycle(&mut run1, 20);
        let mut run2 = build();
        sort_candidates_chronological(&mut run2);
        let d2 = keep_earliest_within_cycle(&mut run2, 20);

        assert_eq!(run1.len(), 2);
        assert_eq!(run1[0].session_id, "a");
        assert_eq!(run1[1].session_id, "b");
        assert_eq!(d1, vec![("c".to_string(), 1)]);
        // Repeatable: identical kept set and drop tally across runs (R-15).
        assert_eq!(
            run1.iter()
                .map(|c| c.session_id.clone())
                .collect::<Vec<_>>(),
            run2.iter()
                .map(|c| c.session_id.clone())
                .collect::<Vec<_>>()
        );
        assert_eq!(d1, d2);
    }

    #[test]
    fn test_aggregate_cap_drop_surfaces_count() {
        // Two sessions, each over-contributing; cap forces a drop from each.
        let mut cands = vec![
            cand("a", 0, Some("2026-01-01T00:00:00Z"), "0123456789"),
            cand("a", 1, Some("2026-01-01T00:00:01Z"), "0123456789"),
            cand("b", 0, Some("2026-01-02T00:00:00Z"), "0123456789"),
        ];
        sort_candidates_chronological(&mut cands);
        let dropped = keep_earliest_within_cycle(&mut cands, 10); // keep only 1
        assert_eq!(cands.len(), 1);
        // Two dropped: one more from "a", one from "b".
        let total_dropped: u64 = dropped.iter().map(|(_, n)| n).sum();
        assert_eq!(total_dropped, 2);

        let mut loss = vec![];
        merge_cycle_drops_into_loss(&mut loss, &dropped);
        assert!(!loss.is_empty(), "aggregate-cap drops must surface as loss");
        let surfaced: u64 = loss.iter().map(|l| l.dropped_candidates).sum();
        assert_eq!(surfaced, 2, "no silent aggregate-cap drop (AC-08)");
    }

    #[test]
    fn test_merge_cycle_drops_into_existing_loss_row() {
        let mut loss = vec![SessionLossInfo {
            session_id: "a".to_string(),
            elided_bytes: 100,
            has_holes: true,
            provenance: CandidateProvenance::Primary,
            dropped_candidates: 1, // a per-session drop already recorded
        }];
        merge_cycle_drops_into_loss(&mut loss, &[("a".to_string(), 3)]);
        assert_eq!(loss.len(), 1, "must reuse the existing row, not duplicate");
        assert_eq!(
            loss[0].dropped_candidates, 4,
            "per-session + cycle drops add"
        );
    }

    // ── ordering ─────────────────────────────────────────────────────────

    #[test]
    fn test_sort_none_ts_last_then_session_then_offset() {
        let mut cands = vec![
            cand("z", 5, None, "n"),
            cand("a", 2, Some("2026-01-02T00:00:00Z"), "x"),
            cand("a", 1, Some("2026-01-01T00:00:00Z"), "x"),
            cand("b", 0, Some("2026-01-01T00:00:00Z"), "x"),
        ];
        sort_candidates_chronological(&mut cands);
        // earliest ts first; ties broken by session then offset; None-ts last.
        assert_eq!(
            cands
                .iter()
                .map(|c| c.session_id.clone())
                .collect::<Vec<_>>(),
            vec!["a", "b", "a", "z"]
        );
        assert!(cands.last().unwrap().ts.is_none());
    }

    // ── assembly-level attach (AC-04, AC-06) ────────────────────────────────

    #[test]
    fn test_attach_none_leaves_response_unchanged() {
        let mut result: Result<CallToolResult, rmcp::model::ErrorData> = Ok(
            CallToolResult::success(vec![rmcp::model::Content::text("report")]),
        );
        attach_to_response_assembly(&mut result, None);
        let call = result.unwrap();
        assert_eq!(
            call.content.len(),
            1,
            "None must not add a content item (AC-04)"
        );
    }

    #[test]
    fn test_attach_some_appends_json_content_item() {
        let mut result: Result<CallToolResult, rmcp::model::ErrorData> = Ok(
            CallToolResult::success(vec![rmcp::model::Content::text("report")]),
        );
        let section = TranscriptCandidatesSection {
            candidates: vec![cand(
                "a",
                0,
                Some("2026-01-01T00:00:00Z"),
                "decided to ship",
            )],
            loss: vec![],
        };
        attach_to_response_assembly(&mut result, Some(section));
        let call = result.unwrap();
        assert_eq!(call.content.len(), 2, "section attaches as one extra item");
    }

    #[test]
    fn test_attach_on_err_is_noop() {
        // Error paths keep transcripts + produce no candidates (AC-05).
        let mut result: Result<CallToolResult, rmcp::model::ErrorData> = Err(
            rmcp::model::ErrorData::new(crate::error::ERROR_INTERNAL, "boom", None),
        );
        attach_to_response_assembly(
            &mut result,
            Some(TranscriptCandidatesSection {
                candidates: vec![],
                loss: vec![],
            }),
        );
        assert!(result.is_err(), "attach must not rewrite an error response");
    }

    // ── distill_before_purge end-to-end (gate / empty / corrupt / poison) ───

    fn cfg(retention: TranscriptRetention) -> RetentionConfig {
        RetentionConfig {
            transcript_retention: retention,
            ..RetentionConfig::default()
        }
    }

    #[test]
    fn test_helper_returns_none_on_retaindays() {
        // AC-10: RetainDays gate → neither distill nor purge → None.
        let reg = SessionRegistry::new();
        reg.register_session("s1", None, Some("crt-052".to_string()));
        reg.apply_transcript_delta("s1", 0, b"some transcript bytes");
        let out = distill_before_purge(
            &reg,
            "crt-052",
            &[],
            &cfg(TranscriptRetention::RetainDays(30)),
        );
        assert!(out.is_none(), "RetainDays must short-circuit to None");
    }

    #[test]
    fn test_zero_attributed_sessions_section_absent() {
        // AC-04: no attributed session → None → response field omitted.
        let reg = SessionRegistry::new();
        let out = distill_before_purge(
            &reg,
            "no-such-cycle",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
        );
        assert!(out.is_none(), "no sessions → None");
    }

    #[test]
    fn test_handler_fully_corrupt_snapshot_normal_response() {
        // AC-V-FUZZ handler level: a fully-corrupt snapshot must NEVER panic; the
        // session degrades to zero primary candidates (non-empty bytes, no
        // markers, no loss → may be omitted entirely).
        let reg = SessionRegistry::new();
        reg.register_session("corrupt", None, Some("crt-052".to_string()));
        // Truncated JSON + embedded NUL + non-UTF-8 bytes.
        reg.apply_transcript_delta("corrupt", 0, b"{\"role\":\"user\x00\xff\xfe not json");
        let out = distill_before_purge(
            &reg,
            "crt-052",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
        );
        // No panic reaching here is the assertion; section is None or candidate-free.
        if let Some(section) = out {
            assert!(
                section.candidates.is_empty(),
                "corrupt input must not synthesize candidates"
            );
        }
    }

    #[test]
    fn test_poison_recovery_surfaces_loss() {
        // R-16: a poison-recovered (treat-as-empty) session → empty bytes →
        // fallback predicate fires → Reconstructed → surfaces in `loss`, never
        // silently absent. No observations → zero candidates but a loss row.
        let reg = SessionRegistry::new();
        reg.register_session("poison", None, Some("crt-052".to_string()));
        reg.apply_transcript_delta("poison", 0, b"pre-poison bytes");
        let arc = reg.get_state("poison").unwrap().transcript;
        let arc2 = std::sync::Arc::clone(&arc);
        let _ = std::thread::spawn(move || {
            let _g = arc2.lock().unwrap();
            panic!("poison the buffer lock");
        })
        .join();

        let out = distill_before_purge(
            &reg,
            "crt-052",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
        )
        .expect("poison-recovered session must surface as loss, not None");
        assert_eq!(out.loss.len(), 1, "the lossy session must appear in loss");
        assert_eq!(out.loss[0].session_id, "poison");
        assert_eq!(out.loss[0].provenance, CandidateProvenance::Reconstructed);
    }

    #[test]
    fn test_primary_path_produces_candidates_from_markered_transcript() {
        // A non-empty, marker-bearing transcript on the PRIMARY path yields
        // candidates with no loss row (clean primary → omitted from loss).
        let reg = SessionRegistry::new();
        reg.register_session("s1", None, Some("crt-052".to_string()));
        let jsonl = b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"We decided to adopt Option B for the held buffer.\"}]},\"timestamp\":\"2026-06-08T10:00:00Z\"}\n";
        reg.apply_transcript_delta("s1", 0, jsonl);

        let out = distill_before_purge(
            &reg,
            "crt-052",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
        )
        .expect("markered primary transcript must yield a section");
        assert!(!out.candidates.is_empty(), "primary candidates expected");
        assert!(
            out.loss.is_empty(),
            "clean primary session (no elision/holes/drop) is omitted from loss"
        );
        // The section serializes (it is response content the agent consumes).
        let json = serde_json::to_string(&out).expect("section serializes");
        assert!(json.contains("decided to adopt Option B"));
    }

    // ── four-return exhaustiveness (AC-05, MERGE GATE, R-07) ────────────────

    /// MERGE GATE (R-07 / SR-05): every `result.is_ok()` purge site in
    /// `context_cycle_review` must be IMMEDIATELY PRECEDED by the shared distill
    /// helper. A fifth success return added without wiring the helper FAILS this
    /// test. Modeled on vnc-025's purge exhaustiveness shape (#4750): a source
    /// assertion over the handler body, counting purge gates and distill calls.
    #[test]
    fn test_exhaustiveness_fifth_return_fails() {
        let src = include_str!("tools.rs");
        // Scope to the context_cycle_review handler body so unrelated purge calls
        // (e.g. test helpers) do not pollute the count.
        let start = src
            .find("async fn context_cycle_review(")
            .expect("handler present");
        let end = src[start..]
            .find("\n    // -- vnc-015: context_edge --")
            .map(|i| start + i)
            .unwrap_or(src.len());
        let body = &src[start..end];

        let purge_gates = body
            .matches("self.purge_cycle_transcripts(&feature_cycle)")
            .count();
        let distill_calls = body
            .matches("distill_handler::distill_before_purge(")
            .count();
        let attach_calls = body
            .matches("distill_handler::attach_to_response_assembly(")
            .count();

        assert_eq!(
            purge_gates, 4,
            "context_cycle_review must have exactly four result.is_ok() purge sites \
             (#4750); a fifth success return must wire the distill helper too"
        );
        assert_eq!(
            distill_calls, purge_gates,
            "every purge site must be preceded by distill_before_purge (AC-05); a \
             fifth unwired success return breaks this lockstep"
        );
        assert_eq!(
            attach_calls, purge_gates,
            "every distill call must attach the section at assembly level (ADR-004)"
        );
    }

    /// AC-05 ordering: distill STRICTLY precedes purge at every site. Source
    /// assertion that each `purge_cycle_transcripts` is textually preceded by an
    /// `attach_to_response_assembly` (which is itself preceded by the distill
    /// call) within the handler body.
    #[test]
    fn test_distill_strictly_before_purge_at_each_return() {
        let src = include_str!("tools.rs");
        let start = src
            .find("async fn context_cycle_review(")
            .expect("handler present");
        let end = src[start..]
            .find("\n    // -- vnc-015: context_edge --")
            .map(|i| start + i)
            .unwrap_or(src.len());
        let body = &src[start..end];

        // Walk each purge gate; assert an attach call appears before it and after
        // the previous purge gate (so the pairing is 1:1 and ordered).
        let mut search_from = 0usize;
        let mut prev_purge = 0usize;
        for _ in 0..4 {
            let purge = body[search_from..]
                .find("self.purge_cycle_transcripts(&feature_cycle)")
                .map(|i| search_from + i)
                .expect("expected a purge gate");
            let attach = body[prev_purge..purge]
                .rfind("attach_to_response_assembly(")
                .map(|i| prev_purge + i);
            assert!(
                attach.is_some(),
                "each purge must be preceded by an assembly-level attach (distill→attach→purge)"
            );
            prev_purge = purge;
            search_from = purge + 1;
        }
    }

    // ── Wave-boundary R-11 (MERGE GATE) ─────────────────────────────────────

    /// MERGE GATE (R-11): the Wave A distill handler must have ZERO compile-time
    /// reference to `transcript_hold.rs`. Held buffers arrive through the C1 seam
    /// transparently; this module never names the hold store. Reverting Wave B
    /// must leave this module compiling untouched.
    #[test]
    fn test_wave_a_handler_no_transcript_hold_dependency() {
        let full = include_str!("distill_handler.rs");
        // Scope to PRODUCTION code only — the portion before the #[cfg(test)]
        // module. The test module legitimately names the forbidden symbols as
        // assertion data; only the shipping handler must be hold-free (R-11).
        let cut = full
            .find("#[cfg(test)]")
            .expect("test module marker present");
        let src = &full[..cut];
        // Strip line comments so prose mentioning Wave B / the hold seam does not
        // trip the assertion — only executable references are forbidden.
        let code: String = src
            .lines()
            .map(|l| match l.find("//") {
                Some(idx) => &l[..idx],
                None => l,
            })
            .collect::<Vec<_>>()
            .join("\n");
        for forbidden in [
            "transcript_hold",
            "TranscriptHold",
            "hold_on_drain",
            "readopt",
        ] {
            assert!(
                !code.contains(forbidden),
                "R-11: Wave A handler must not reference Wave B hold symbol '{forbidden}'"
            );
        }
    }

    // ── content-leak structural (AC-06, MERGE GATE) ─────────────────────────

    /// MERGE GATE (AC-06 / R-04): candidates are STRUCTURALLY barred from the
    /// memoized `RetrospectiveReport`. This is a compile-level guarantee — the
    /// persisted type has no candidate field, so serializing it can never contain
    /// candidate text. Asserted by serializing a report and confirming the
    /// `transcript_candidates` key cannot appear (the type has no such field).
    #[test]
    fn test_candidates_structurally_absent_from_memoized_report() {
        let report = unimatrix_observe::RetrospectiveReport {
            feature_cycle: "crt-052".to_string(),
            session_count: 1,
            total_records: 1,
            metrics: unimatrix_observe::MetricVector::default(),
            hotspots: vec![],
            is_cached: false,
            baseline_comparison: None,
            entries_analysis: None,
            narratives: None,
            recommendations: vec![],
            session_summaries: None,
            feature_knowledge_reuse: None,
            rework_session_count: None,
            context_reload_pct: None,
            attribution: None,
            phase_narrative: None,
            goal: None,
            cycle_type: None,
            attribution_path: None,
            is_in_progress: None,
            phase_stats: None,
            curation_health: None,
        };
        let json = serde_json::to_string(&report).expect("serialize memoized report");
        assert!(
            !json.contains("transcript_candidates"),
            "AC-06: the persisted RetrospectiveReport must have NO candidate slot"
        );
    }
}
