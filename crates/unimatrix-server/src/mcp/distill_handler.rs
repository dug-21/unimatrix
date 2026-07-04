//! crt-057 — Read-only scoped transcript retrieval (renamed from crt-052's
//! `distill_before_purge`; there is NO purge anymore, NG-6).
//!
//! One shared helper — [`retrieve_scoped_candidates`] — invoked at ALL FOUR
//! `context_cycle_review` `result.is_ok()` success returns. It is the SOLE reader
//! of buffer *content* (via `snapshot()`, CON-3/#4848) and is fully
//! non-destructive: the review never purges (crt-057, ADR-001). It orchestrates:
//!
//! 0. an EARLY RETURN of `None` when no `transcript` scope is supplied — the lean
//!    default reads no buffer at all (FR-6),
//! 1. the exhaustive `TranscriptRetention` gate (ADR-005 / AC-10) — this gate
//!    decides whether a *retrieval* runs; the RECLAMATION-side exhaustive match
//!    is a SEPARATE obligation re-homed onto the backstops (`server.rs`
//!    `reclaim_permitted_by_retention`, orphan-deletion.md / backstop-reclaim.md),
//! 2. the off-lock snapshot seam `take_transcripts_for_feature` (C1 / ADR-001) —
//!    a READ that does NOT clear buffers (the buffer survives, FR-11),
//! 3. per-session Primary [`select_candidates`] (C3) vs Reconstructed
//!    [`reconstruct_from_observations`] (C5) via the shared fallback predicate,
//! 4. the NEW AND-composed scope filter (`phase`/`anchor`/`match`+`window`) with
//!    server-side cross-plane clock normalization (`distill_scope`, ADR-006),
//! 5. the per-cycle aggregate cap plus per-session [`SessionLossInfo`] assembly
//!    (a no-match over a lossy session stays INDETERMINATE, never a bare false —
//!    R-01), and
//! 6. assembly-level attach of the section + the response-transient search-status
//!    projection, strictly OUTSIDE the memoized `RetrospectiveReport`
//!    (ADR-004 / CON-4).
//!
//! **Wave A invariant (R-11):** this module has ZERO compile-time reference to
//! `transcript_hold.rs`. Held buffers arrive transparently through the C1 seam;
//! this helper never touches the hold store.
//!
//! **Secrets posture (CON-4):** candidates, loss, and search-status are
//! response-transient. They are NEVER written onto `RetrospectiveReport` (the
//! persisted type has no slot), so the memoization persist (`store_cycle_review`
//! → `cycle_review_index`, #3793) structurally cannot carry them.
//! [`attach_to_response_assembly`] / [`attach_search_status`] add them to the
//! already-built `CallToolResult` AFTER the report is computed and memoized.

use rmcp::model::CallToolResult;
use unimatrix_observe::distill::{reconstruct_from_observations, select_candidates};
use unimatrix_observe::{
    BoundsKind, CandidateProvenance, ObservationRecord, ResolvedBounds, SessionLossInfo,
    SessionSearchStatus, TranscriptCandidate, TranscriptCandidatesSection, TranscriptScope,
};

use crate::infra::config::{RetentionConfig, TranscriptRetention};
use crate::infra::session::SessionRegistry;
use crate::infra::session_transcript::TranscriptSnapshot;
use crate::mcp::distill_scope::{self, ScopeCtx};

/// Retrieve scope-filtered transcript candidates for `feature_cycle` (crt-057;
/// renamed from `distill_before_purge` — no purge follows, NG-6).
///
/// Returns `None` when no `scope` is supplied (the lean non-destructive default
/// reads NO buffer, FR-6), or when there is nothing to surface (FR-7 — the
/// response field is then omitted entirely). Otherwise `Some(section)` carrying
/// the scope-filtered candidates + per-session loss. Never panics: the snapshot
/// seam is infallible, C3/C5 are total on untrusted input (R-10), so a
/// fully-corrupt snapshot degrades to zero candidates rather than an error.
///
/// The four call sites pass the registry, the reviewed `feature_cycle`, the
/// ALREADY-loaded observation set (do NOT re-query — ADR-005), the retention
/// config, the caller's `scope` (`None` ⇒ early `None`), and the reviewing
/// session id (`reviewer_session_id`, reserved for an optional live-sibling
/// advisory per ADR-003 — NOT a contract). All parsing happens after the seam
/// returns; no lock is held here. The `match` regex is validated UP FRONT in the
/// handler (`distill_scope::validate_scope_regex`) so this helper stays
/// infallible-total (`-> Option<...>`).
pub(crate) fn retrieve_scoped_candidates(
    registry: &SessionRegistry,
    feature_cycle: &str,
    observations: &[ObservationRecord],
    cfg: &RetentionConfig,
    scope: Option<&TranscriptScope>,
    reviewer_session_id: Option<&str>,
    resolved_bounds: Option<ResolvedBounds>,
) -> Option<TranscriptCandidatesSection> {
    // (0) EARLY RETURN — no scope ⇒ no buffer read ⇒ lean non-destructive default
    //     (FR-6). `omit transcript` → section absent.
    let scope = scope?;
    // Reserved for an optional live-sibling advisory (ADR-003); not yet consumed.
    let _ = reviewer_session_id;

    // (0b) UNRESOLVED time scope ⇒ absent section (FR-7), never an error. A scope
    //      that requests `anchor`/`phase` whose id did not resolve to bounds
    //      (unknown finding/phase id, or a degenerate path with no hotspots/
    //      cycle_events) yields nothing — NOT a full dump. Resolved bounds are
    //      computed by the handler (`resolve_transcript_scope_bounds`) where the
    //      report `hotspots` + `cycle_events` are in scope.
    let wants_time_scope = scope.anchor.is_some() || scope.phase.is_some();
    if wants_time_scope && resolved_bounds.is_none() {
        return None;
    }

    // (1) EXHAUSTIVE retention gate. NO wildcard arm: adding a third
    //     `TranscriptRetention` variant MUST be a compile error. This gate decides
    //     whether a RETRIEVAL runs; the reclamation-side exhaustive match is
    //     re-homed onto the backstops (`server.rs::reclaim_permitted_by_retention`).
    match cfg.transcript_retention {
        TranscriptRetention::PurgeOnCycleClose => {} // proceed
        TranscriptRetention::RetainDays(_) => return None, // neither retrieve nor (formerly) purge
    }

    // (2) Snapshot off-lock (C1 / ADR-001). Returns registered ∪ held for the
    //     cycle. This is a READ; it does NOT clear buffers — the buffer survives
    //     (FR-11), nothing downstream purges. ALL parsing happens AFTER this.
    let snapshots = registry.take_transcripts_for_feature(feature_cycle);

    // (3) Resolve the scope context ONCE (compile regex; wire the handler-resolved
    //     anchor/phase bounds). The regex was already validated by the handler, so
    //     an internal compile failure is unreachable; if it somehow occurs the
    //     section is absent rather than a panic.
    let ctx = build_scope_ctx(scope, resolved_bounds);

    let session_cap = cfg.transcript_candidate_session_cap_bytes;
    let mut all_candidates: Vec<TranscriptCandidate> = Vec::new();
    let mut loss: Vec<SessionLossInfo> = Vec::new();

    // (4) Per-session: Primary (C3) or Reconstructed (C5) via the SHARED
    //     fallback predicate (ADR-006), THEN the NEW scope filter. Whole-session
    //     either/or — never a byte-level mix within one session (OQ-2).
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

        // (4b) NEW scoped filter over the retained candidates (AND-composed;
        //      transcript-scope.md / distill_scope). The `ts:None` byte_offset
        //      fallback is applied per session below. NOTE (R-01): the loss row
        //      is pushed UNCONDITIONALLY (4c) — a lossy no-match session still
        //      surfaces so the no-match is INDETERMINATE, never a bare false.
        let kept = apply_scope_filter(session_cands, &ctx);
        all_candidates.extend(kept);

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

/// Build the resolved scope context ONCE per retrieval (compile the already-
/// validated `match` regex; wire the handler-resolved anchor/phase bounds).
///
/// `resolved_bounds` is computed by the handler (`resolve_transcript_scope_bounds`)
/// where the report `hotspots` (for `anchor`) and `cycle_events` (for `phase`) are
/// in scope. `Anchor` bounds honor `window` (windowed join); `Phase` bounds are
/// self-bounding (window IGNORED). Absent bounds ⇒ no time filter (a `match`-only
/// or empty scope).
fn build_scope_ctx(scope: &TranscriptScope, resolved_bounds: Option<ResolvedBounds>) -> ScopeCtx {
    let compiled = scope
        .r#match
        .as_deref()
        .and_then(|p| distill_scope::compile_bounded_regex(p).ok());
    let (anchor_bounds, phase_bounds) = match resolved_bounds {
        Some(rb) => match rb.kind {
            BoundsKind::Anchor => (Some((rb.lo_epoch_ms, rb.hi_epoch_ms)), None),
            BoundsKind::Phase => (None, Some((rb.lo_epoch_ms, rb.hi_epoch_ms))),
        },
        None => (None, None),
    };
    ScopeCtx {
        compiled,
        anchor_bounds,
        phase_bounds,
        window: scope.window.clone(),
    }
}

/// Apply the AND-composed scope filter to one session's selected candidates.
///
/// With no time-bounds (`match`-only or empty scope) the filter reduces to the
/// regex (or passes everything for an empty scope) and `ts:None` candidates are
/// never dropped by time (R-09 / AC-05). With time-bounds, ts-bearing candidates
/// are decided by the windowed join and `ts:None` candidates are included when
/// within `±blocks` `byte_offset`-proximity of an in-window candidate (AC-07 —
/// never a silent drop).
fn apply_scope_filter(
    candidates: Vec<TranscriptCandidate>,
    ctx: &ScopeCtx,
) -> Vec<TranscriptCandidate> {
    let has_time_bounds = ctx.anchor_bounds.is_some() || ctx.phase_bounds.is_some();
    if !has_time_bounds {
        return candidates
            .into_iter()
            .filter(|c| distill_scope::scope_predicate(c, ctx, true))
            .collect();
    }

    // Time-bounded: order by byte_offset for block indices, find the in-window
    // ts-bearing block range, then include ts:None candidates within ±blocks.
    let mut indexed: Vec<TranscriptCandidate> = candidates;
    indexed.sort_by_key(|c| c.byte_offset);
    let range = indexed
        .iter()
        .enumerate()
        .filter(|(_, c)| c.ts.is_some() && distill_scope::scope_predicate(c, ctx, false))
        .map(|(i, _)| i)
        .fold(None, |acc: Option<(usize, usize)>, i| match acc {
            None => Some((i, i)),
            Some((lo, hi)) => Some((lo.min(i), hi.max(i))),
        });
    let blocks = unimatrix_observe::Window::effective(ctx.window.as_ref()).1;

    indexed
        .into_iter()
        .enumerate()
        .filter(|(block_idx, c)| {
            if c.ts.is_some() {
                distill_scope::scope_predicate(c, ctx, false)
            } else {
                match range {
                    Some((lo, hi)) => {
                        distill_scope::block_within(*block_idx, lo, hi, blocks)
                            && distill_scope::scope_predicate(c, ctx, true)
                    }
                    None => false,
                }
            }
        })
        .map(|(_, c)| c)
        .collect()
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

/// Attach the response-transient per-session search-status projection + anchor/
/// phase `ResolvedBounds` at ASSEMBLY level (crt-057, FR-14/15/16).
///
/// Same secrets discipline as [`attach_to_response_assembly`]: a no-op on `Err`
/// and when there is nothing to report; the payload carries NO transcript bytes
/// (only session ids, booleans, counters, epoch bounds), so the R-03 content-scan
/// sees no verbatim/secret-shaped run. Never a field on any persisted struct.
pub(crate) fn attach_search_status(
    result: &mut Result<CallToolResult, rmcp::model::ErrorData>,
    rows: Vec<SessionSearchStatus>,
    bounds: Option<ResolvedBounds>,
) {
    let Ok(call_result) = result.as_mut() else {
        return;
    };
    if rows.is_empty() && bounds.is_none() {
        return;
    }
    let payload = serde_json::json!({ "search": rows, "resolved_bounds": bounds });
    if let Ok(json) = serde_json::to_string(&payload) {
        call_result.content.push(rmcp::model::Content::text(format!(
            "\ntranscript_search: {json}"
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infra::session_transcript::HoleInfo;
    use unimatrix_observe::{FamilyHint, Window};

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

    /// The empty scope `{}` (all-None) ≡ `match:".*"` — the full candidate set
    /// under the existing per-cycle cap (AC-05). Used wherever a retrieval must
    /// run (scope present) without narrowing.
    fn full_scope() -> TranscriptScope {
        TranscriptScope {
            phase: None,
            anchor: None,
            r#match: None,
            window: None,
        }
    }

    /// A `match`-only scope with the given pattern.
    fn match_scope(pattern: &str) -> TranscriptScope {
        TranscriptScope {
            phase: None,
            anchor: None,
            r#match: Some(pattern.to_string()),
            window: None,
        }
    }

    #[test]
    fn test_helper_returns_none_when_scope_none() {
        // FR-6: no scope ⇒ lean non-destructive default ⇒ None, and the buffer is
        // never read. Proven synchronously (R-10): the registered buffer is still
        // present with its content after the call (no snapshot/clear occurred).
        let reg = SessionRegistry::new();
        reg.register_session("s1", None, Some("crt-057".to_string()));
        reg.apply_transcript_delta("s1", 0, b"lean-default-bytes");
        let out = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            None, // no scope
            None,
            None,
        );
        assert!(out.is_none(), "no scope ⇒ None (lean default)");
        // Buffer intact (synchronous observable state, never absence of an audit).
        let state = reg.get_state("s1").expect("session stays registered");
        assert_eq!(
            state
                .transcript
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .len(),
            b"lean-default-bytes".len(),
            "buffer untouched — no read occurred on the scope-absent path"
        );
    }

    #[test]
    fn test_empty_scope_returns_full_candidate_set() {
        // R-09 / AC-05: `transcript:{}` ≡ `match:".*"` — full dump under the cap.
        let reg = SessionRegistry::new();
        reg.register_session("s1", None, Some("crt-057".to_string()));
        let jsonl = b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"We decided to adopt Option B.\"}]},\"timestamp\":\"2026-06-08T10:00:00Z\"}\n";
        reg.apply_transcript_delta("s1", 0, jsonl);

        let empty = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&full_scope()),
            None,
            None,
        )
        .expect("empty scope must yield the full set");
        let star = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&match_scope(".*")),
            None,
            None,
        )
        .expect("match:.* must yield the full set");
        assert_eq!(
            empty.candidates.len(),
            star.candidates.len(),
            "transcript:{{}} ≡ match:\".*\""
        );
        assert!(
            !empty.candidates.is_empty(),
            "populated fixture yields candidates"
        );
    }

    #[test]
    fn test_match_scope_narrows_intersection() {
        // R-09: a `match` that excludes the block returns a strict subset (here
        // empty), while `.*` returns the block — AND-composition NARROWS.
        let reg = SessionRegistry::new();
        reg.register_session("s1", None, Some("crt-057".to_string()));
        let jsonl = b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"We decided to adopt Option B.\"}]},\"timestamp\":\"2026-06-08T10:00:00Z\"}\n";
        reg.apply_transcript_delta("s1", 0, jsonl);

        let all = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&match_scope(".*")),
            None,
            None,
        )
        .expect("full set");
        let narrowed = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&match_scope("this-token-is-not-present-xyzzy")),
            None,
            None,
        );
        assert!(!all.candidates.is_empty());
        // Narrowed drops the only candidate → nothing to surface → None (FR-7),
        // a strict subset of the full set.
        assert!(
            narrowed.is_none() || narrowed.unwrap().candidates.len() < all.candidates.len(),
            "match narrows to a strict subset"
        );
    }

    #[test]
    fn test_non_destructive_repeat_identical_candidates() {
        // AC-03: a second identical `transcript:{}` retrieval returns the same
        // candidates — the buffer survived (no purge, fully non-destructive).
        let reg = SessionRegistry::new();
        reg.register_session("s1", None, Some("crt-057".to_string()));
        let jsonl = b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"We decided to adopt Option B.\"}]},\"timestamp\":\"2026-06-08T10:00:00Z\"}\n";
        reg.apply_transcript_delta("s1", 0, jsonl);

        let first = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&full_scope()),
            None,
            None,
        )
        .expect("first retrieval");
        let second = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&full_scope()),
            None,
            None,
        )
        .expect("second retrieval — buffer survived");
        assert_eq!(
            first.candidates.len(),
            second.candidates.len(),
            "non-destructive: identical candidate count on repeat"
        );
    }

    /// Anchor id "F-NN"; resolved bounds are supplied directly here (the handler
    /// resolves them from `report.hotspots` via `resolve_transcript_scope_bounds`).
    fn anchor_scope(id: &str, window: Option<Window>) -> TranscriptScope {
        TranscriptScope {
            phase: None,
            anchor: Some(id.to_string()),
            r#match: None,
            window,
        }
    }

    // 2026-06-08T10:00:00Z in epoch-millis (fixed offset, never now_ts()).
    const ANCHOR_T: u64 = 1_780_912_800_000;

    fn two_decision_blocks(reg: &SessionRegistry) {
        // INSIDE  = anchor + 60s (within ±120s default window)
        // OUTSIDE = anchor + 300s (outside the window)
        let inside = b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"We decided to adopt INSIDE.\"}]},\"timestamp\":\"2026-06-08T10:01:00Z\"}\n";
        let outside = b"{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\",\"content\":[{\"type\":\"text\",\"text\":\"We decided to adopt OUTSIDE.\"}]},\"timestamp\":\"2026-06-08T10:05:00Z\"}\n";
        reg.register_session("s1", None, Some("crt-057".to_string()));
        reg.apply_transcript_delta("s1", 0, inside);
        reg.apply_transcript_delta("s1", inside.len() as u64, outside);
    }

    fn anchor_bounds() -> ResolvedBounds {
        ResolvedBounds {
            kind: BoundsKind::Anchor,
            lo_epoch_ms: ANCHOR_T,
            hi_epoch_ms: ANCHOR_T,
        }
    }

    #[test]
    fn test_anchor_bounds_filters_candidates_within_window() {
        // End-to-end: anchor span [T,T] + default ±120s window keeps the +60s
        // block, drops the +300s block. Proves the windowed join over RESOLVED
        // anchor bounds (the handler resolves F-NN → this span from hotspots).
        let reg = SessionRegistry::new();
        two_decision_blocks(&reg);
        let out = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&anchor_scope("F-01", None)),
            None,
            Some(anchor_bounds()),
        )
        .expect("in-window candidate present");
        assert_eq!(out.candidates.len(), 1, "only the in-window block survives");
        assert!(
            out.candidates[0].text.contains("INSIDE"),
            "the +60s block is the one kept"
        );
    }

    #[test]
    fn test_anchor_and_match_and_compose() {
        // R-09: anchor (keeps INSIDE, drops OUTSIDE) ∧ match narrows further.
        let reg = SessionRegistry::new();
        two_decision_blocks(&reg);
        // anchor ∧ match:"INSIDE" → the single in-window block.
        let mut scope = anchor_scope("F-01", None);
        scope.r#match = Some("INSIDE".to_string());
        let kept = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&scope),
            None,
            Some(anchor_bounds()),
        )
        .expect("intersection non-empty");
        assert_eq!(kept.candidates.len(), 1);
        assert!(kept.candidates[0].text.contains("INSIDE"));

        // anchor ∧ match:"OUTSIDE" → anchor drops OUTSIDE (out of window), match
        // drops INSIDE → empty intersection → absent section (FR-7).
        let mut scope2 = anchor_scope("F-01", None);
        scope2.r#match = Some("OUTSIDE".to_string());
        let empty = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&scope2),
            None,
            Some(anchor_bounds()),
        );
        assert!(
            empty.is_none(),
            "AND-composition intersects to nothing → absent"
        );
    }

    #[test]
    fn test_unknown_anchor_id_yields_absent_section() {
        // A scope that requests `anchor` whose id did NOT resolve (bounds None) →
        // absent section (FR-7), NOT a full dump and NOT an error.
        let reg = SessionRegistry::new();
        two_decision_blocks(&reg);
        let out = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&anchor_scope("F-99", None)),
            None,
            None, // unresolved
        );
        assert!(
            out.is_none(),
            "unresolved anchor id → absent, never a full dump"
        );
    }

    #[test]
    fn test_phase_bounds_are_self_bounding_ignore_window() {
        // Phase span [T, T+120s]; a candidate at T+300s is OUTSIDE the phase and
        // is dropped even though a huge supplied window would have included it —
        // phase is self-bounding (window IGNORED). The +60s block is kept.
        let reg = SessionRegistry::new();
        two_decision_blocks(&reg);
        let phase_scope = TranscriptScope {
            phase: Some("implementation".to_string()),
            anchor: None,
            r#match: None,
            // A 10-minute window that WOULD include the +300s block if honored.
            window: Some(Window {
                millis: Some(600_000),
                blocks: None,
            }),
        };
        let phase_bounds = ResolvedBounds {
            kind: BoundsKind::Phase,
            lo_epoch_ms: ANCHOR_T,
            hi_epoch_ms: ANCHOR_T + 120_000,
        };
        let out = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&phase_scope),
            None,
            Some(phase_bounds),
        )
        .expect("in-phase candidate present");
        assert_eq!(
            out.candidates.len(),
            1,
            "self-bounding phase ignores the window; only the in-phase block survives"
        );
        assert!(out.candidates[0].text.contains("INSIDE"));
    }

    #[test]
    fn test_anchor_scope_loss_honesty_preserved() {
        // R-01 attached to the anchor path: a clean in-window primary hit returns
        // a candidate with NO loss row (trustworthy), proving the anchor path runs
        // the same loss-honesty assembly as the match path (never a bare result).
        let reg = SessionRegistry::new();
        two_decision_blocks(&reg);
        let out = retrieve_scoped_candidates(
            &reg,
            "crt-057",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&anchor_scope("F-01", None)),
            None,
            Some(anchor_bounds()),
        )
        .expect("anchor path returns a section");
        assert_eq!(out.candidates.len(), 1);
        assert!(
            out.loss.is_empty(),
            "clean primary anchor hit → no loss row (trustworthy negative)"
        );
    }

    #[test]
    fn test_helper_returns_none_on_retaindays() {
        // AC-10: RetainDays gate → neither distill nor purge → None.
        let reg = SessionRegistry::new();
        reg.register_session("s1", None, Some("crt-052".to_string()));
        reg.apply_transcript_delta("s1", 0, b"some transcript bytes");
        let out = retrieve_scoped_candidates(
            &reg,
            "crt-052",
            &[],
            &cfg(TranscriptRetention::RetainDays(30)),
            Some(&full_scope()),
            None,
            None,
        );
        assert!(out.is_none(), "RetainDays must short-circuit to None");
    }

    #[test]
    fn test_zero_attributed_sessions_section_absent() {
        // AC-04: no attributed session → None → response field omitted.
        let reg = SessionRegistry::new();
        let out = retrieve_scoped_candidates(
            &reg,
            "no-such-cycle",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&full_scope()),
            None,
            None,
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
        let out = retrieve_scoped_candidates(
            &reg,
            "crt-052",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&full_scope()),
            None,
            None,
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

        let out = retrieve_scoped_candidates(
            &reg,
            "crt-052",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&full_scope()),
            None,
            None,
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

        let out = retrieve_scoped_candidates(
            &reg,
            "crt-052",
            &[],
            &cfg(TranscriptRetention::PurgeOnCycleClose),
            Some(&full_scope()),
            None,
            None,
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

    /// MERGE GATE (R-07 / R-11): every `result.is_ok()` success return in
    /// `context_cycle_review` must wire the shared scoped-retrieval helper and
    /// attach the section at assembly level. A fifth success return added without
    /// wiring both FAILS this test.
    ///
    /// crt-057 REMOVED the `purge_cycle_transcripts(&feature_cycle)` ×4 count and
    /// the attach-before-purge ordering assertion (`test_distill_strictly_before_
    /// purge_at_each_return`, deleted) WITH this rationale: the review has NO purge
    /// verb anymore (NG-6 / ADR-001) — there is nothing to count or order against.
    /// The `distill_before_purge(` string is renamed to `retrieve_scoped_candidates(`;
    /// the ×4 count STANDS. Reclamation is delegated entirely to the backstops
    /// (the exhaustive `TranscriptRetention` match re-homed onto
    /// `server.rs::reclaim_permitted_by_retention`).
    #[test]
    fn test_exhaustiveness_fifth_return_fails() {
        let src = include_str!("tools.rs");
        // Scope to the context_cycle_review handler body so unrelated calls
        // (e.g. test helpers) do not pollute the count.
        let start = src
            .find("async fn context_cycle_review(")
            .expect("handler present");
        let end = src[start..]
            .find("\n    // -- vnc-015: context_edge --")
            .map(|i| start + i)
            .unwrap_or(src.len());
        let body = &src[start..end];

        // Purge must be GONE from the handler body (NG-6 — fully non-destructive).
        let purge_calls = body
            .matches("self.purge_cycle_transcripts(&feature_cycle)")
            .count();
        assert_eq!(
            purge_calls, 0,
            "crt-057: the review has no purge verb — no purge_cycle_transcripts call may remain"
        );

        let retrieve_calls = body
            .matches("distill_handler::retrieve_scoped_candidates(")
            .count();
        let attach_calls = body
            .matches("distill_handler::attach_to_response_assembly(")
            .count();

        assert_eq!(
            retrieve_calls, 4,
            "context_cycle_review must wire retrieve_scoped_candidates at all four \
             result.is_ok() success returns (#4750); a fifth return must wire it too"
        );
        assert_eq!(
            attach_calls, 4,
            "every scoped-retrieval call must attach the section at assembly level (ADR-004)"
        );
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

    // ── crt-057 Gate-3c carry-forward: AC-19 ownership boundary (negative) ───

    /// crt-057 CARRY-FORWARD (Gate 3b → 3c): **AC-19 / NG-5** — the ownership
    /// boundary as a NEGATIVE requirement. `context_cycle_review`'s crt-057
    /// response surface (the scoped Plane-B slice + honesty projections) carries
    /// NO synthesized cross-source field: no GH `## Knowledge Stewardship` join,
    /// no applied-entry attribution, no rework-count↔cause join, no
    /// human-intervention ledger. Standalone schema-shape + code-path assertion —
    /// NOT leaned on R-18 (report-body invariance is a DIFFERENT negative).
    #[test]
    fn test_ac19_ownership_boundary_no_cross_source_synthesis() {
        use unimatrix_observe::{BoundsKind, ResolvedBounds, SessionSearchStatus};

        // (1) SCHEMA-SHAPE — populate every crt-057 response type, serialize, and
        //     assert (a) an allow-list of known content-free fields (an ADDED
        //     synthesis field is caught) and (b) NO key anywhere carries a
        //     cross-source-synthesis concept token.
        fn collect_keys(v: &serde_json::Value, out: &mut Vec<String>) {
            match v {
                serde_json::Value::Object(m) => {
                    for (k, val) in m {
                        out.push(k.clone());
                        collect_keys(val, out);
                    }
                }
                serde_json::Value::Array(a) => a.iter().for_each(|e| collect_keys(e, out)),
                _ => {}
            }
        }

        let section = TranscriptCandidatesSection {
            candidates: vec![cand("s", 0, Some("2026-06-08T10:00:00Z"), "decided")],
            loss: vec![SessionLossInfo {
                session_id: "s".to_string(),
                elided_bytes: 3,
                has_holes: true,
                provenance: CandidateProvenance::Reconstructed,
                dropped_candidates: 1,
            }],
        };
        let status = SessionSearchStatus {
            session_id: "s".to_string(),
            matched: Some(false),
            search_complete: false,
            elided_bytes: 3,
            provenance: CandidateProvenance::Primary,
        };
        let bounds = ResolvedBounds {
            kind: BoundsKind::Anchor,
            lo_epoch_ms: 1,
            hi_epoch_ms: 2,
        };
        // The exact payload attach_search_status emits at assembly level.
        let search_payload = serde_json::json!({ "search": [status], "resolved_bounds": bounds });

        let mut keys = Vec::new();
        collect_keys(&serde_json::to_value(&section).unwrap(), &mut keys);
        collect_keys(&search_payload, &mut keys);

        const ALLOWED: &[&str] = &[
            "candidates",
            "loss",
            "session_id",
            "byte_offset",
            "ts",
            "family_hints",
            "text",
            "elided_bytes",
            "has_holes",
            "provenance",
            "dropped_candidates",
            "search",
            "matched",
            "search_complete",
            "resolved_bounds",
            "kind",
            "lo_epoch_ms",
            "hi_epoch_ms",
        ];
        for k in &keys {
            assert!(
                ALLOWED.contains(&k.as_str()),
                "AC-19: unexpected response field '{k}' — crt-057 must add no \
                 attribution/join/ledger field to the review response surface"
            );
        }

        const FORBIDDEN: &[&str] = &[
            "attribution",
            "applied",
            "ledger",
            "stewardship",
            "rework",
            "cause",
            "human",
            "intervention",
            "github",
            "gh_",
            "join",
            "synthes",
        ];
        for k in &keys {
            let lk = k.to_lowercase();
            for bad in FORBIDDEN {
                assert!(
                    !lk.contains(bad),
                    "AC-19: response field '{k}' implies cross-source synthesis ('{bad}')"
                );
            }
        }

        // (2) CODE-PATH — the crt-057 production modules synthesize nothing across
        //     GH stewardship blocks / applied entries / human ledger. The boundary
        //     is enforced by the ABSENCE of any such symbol in the shipping source
        //     (comments stripped so prose cannot trip the guard).
        for full in [
            include_str!("distill_handler.rs"),
            include_str!("distill_scope.rs"),
        ] {
            let cut = full.find("#[cfg(test)]").unwrap_or(full.len());
            let code: String = full[..cut]
                .lines()
                .map(|l| match l.find("//") {
                    Some(i) => &l[..i],
                    None => l,
                })
                .collect::<Vec<_>>()
                .join("\n")
                .to_lowercase();
            for bad in [
                "knowledge stewardship",
                "stewardship",
                "applied_entry",
                "attribution",
                "human_intervention",
                "rework",
                "ledger",
                "cross_source",
            ] {
                assert!(
                    !code.contains(bad),
                    "AC-19: crt-057 production code must not synthesize across sources ('{bad}')"
                );
            }
        }
    }
}
