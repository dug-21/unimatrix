# Component: `retrieve_scoped_candidates` (RENAMED from `distill_before_purge`)

File: `unimatrix-server/src/mcp/distill_handler.rs:48`

## RENAME (OQ-4 delivery decision — see OVERVIEW)

`distill_before_purge` → **`retrieve_scoped_candidates`**. No purge follows; the name now describes a
read-only, scoped retrieval. **Tester action:** update the source-assertion tests
(`distill_handler.rs:651-726`) to count `retrieve_scoped_candidates(` instead of
`distill_before_purge(`; the module doc-comment header (lines 1-24, "immediately BEFORE
`purge_cycle_transcripts`") must be rewritten — it is now the read-only retrieval helper.

## Purpose

The SOLE reader of buffer *content* (via `snapshot()`, CON-3/#4848). Returns `None` when no scope is
supplied (lean default — no buffer read). When a scope is present, applies the AND-composed
phase/anchor/match+window filters with server-side clock normalization, and produces candidates +
per-session `SessionLossInfo`.

## New signature (ARCH §12)

```
fn retrieve_scoped_candidates(
    registry: &SessionRegistry,
    feature_cycle: &str,
    observations: &[ObservationRecord],
    cfg: &RetentionConfig,
    scope: Option<&TranscriptScope>,      // NEW
    reviewer_session_id: Option<&str>,    // NEW — reserved for optional advisory (ADR-003), not a contract
) -> Option<TranscriptCandidatesSection>
```

## Body (pseudocode)

```
fn retrieve_scoped_candidates(registry, feature_cycle, observations, cfg, scope, reviewer_session_id):
    # (0) NEW EARLY RETURN — no scope ⇒ no buffer read ⇒ lean non-destructive default (FR-6).
    let scope = match scope:
        None    => return None            # omit `transcript` → section absent
        Some(s) => s

    # (1) EXHAUSTIVE retention gate — UNCHANGED, still no `_` arm (C-5). NOTE: this gate stays here
    #     because it decides whether a *retrieval* runs; the RECLAMATION-side exhaustive match is a
    #     SEPARATE obligation re-homed onto the backstops (orphan-deletion.md / backstop-reclaim.md).
    match cfg.transcript_retention:
        PurgeOnCycleClose => {}           # proceed
        RetainDays(_)     => return None  # neither retrieve nor (formerly) purge

    # (2) Read-only snapshot off-lock (CON-3). Reuses take_transcripts_for_feature → buf.snapshot().
    #     NOTE: this is a READ; it does NOT clear buffers (session.rs:483 seam is snapshot-only). The
    #     buffer survives (FR-11) — nothing downstream purges.
    let snapshots = registry.take_transcripts_for_feature(feature_cycle)   # Vec<(session_id, TranscriptSnapshot)>

    # (3) Resolve scope context ONCE (compile regex, resolve anchor/phase bounds) — reused per candidate.
    let ctx = build_scope_ctx(scope, observations, feature_cycle)?   # ? = invalid regex → Err path (see below)
        # ctx = { compiled_regex: Option<Regex>, anchor_bounds: Option<(lo,hi)>, phase_bounds: Option<(lo,hi)>,
        #         window: Option<&Window>, resolved_bounds: Option<ResolvedBounds> }

    # (4) Per session: build candidates (existing Primary/Reconstructed pipeline — UNCHANGED) then FILTER.
    let mut all_candidates = []
    let mut loss = []
    for (session_id, snap) in snapshots:
        # 4a. EXISTING selection pipeline (fallback predicate → select_candidates | reconstruct;
        #     per-session cap; provenance). This is the current distill body, UNCHANGED.
        (session_cands, provenance, dropped) = existing_select_for_session(&snap, &session_id, observations, cfg)

        # 4b. NEW scoped filter over the retained candidates (scope_predicate — transcript-scope.md).
        let kept = session_cands.into_iter().filter(|c| scope_predicate(scope, c, &ctx, &session_id)).collect()

        all_candidates.extend(kept)

        # 4c. EXISTING per-session loss assembly — UNCHANGED (elided/holes/Reconstructed/dropped).
        #     NOTE (R-01): a session that no-matched but is LOSSY still emits its loss row so the
        #     no-match is INDETERMINATE, never a bare false. The filter in 4b MUST NOT drop the loss row.
        push_loss_if_any(&mut loss, &session_id, &snap, provenance, dropped)

    # (5) EXISTING cross-session ordering + per-cycle aggregate cap (keep-earliest) — UNCHANGED.
    sort_candidates_chronological(&mut all_candidates)
    let cycle_dropped = keep_earliest_within_cycle(&mut all_candidates, cfg.transcript_candidate_cycle_cap_bytes)
    merge_cycle_drops_into_loss(&mut loss, &cycle_dropped)

    # (6) Absent-when-empty (FR-7). Nothing to report → None (section omitted, NOT null).
    if all_candidates.is_empty() and loss.is_empty(): return None
    Some(TranscriptCandidatesSection { candidates: all_candidates, loss })
```

### `build_scope_ctx` (new helper)

```
fn build_scope_ctx(scope, observations, feature_cycle) -> Result<ScopeCtx, ErrorData>:
    compiled = match &scope.r#match:
        None    => None
        Some(p) => Some(compile_bounded_regex(p).map_err(|e| ERROR_INVALID_PARAMS("Invalid 'match' regex"))?)
    anchor_bounds = scope.anchor.as_ref().map(|id| resolve_anchor_span(id))       # HotspotFinding.evidence ts min/max
    phase_bounds  = scope.phase.as_ref().map(|id| resolve_phase_bounds(id, feature_cycle))  # cycle_events
    resolved_bounds = pick_resolved_bounds(anchor_bounds, phase_bounds)           # for FR-16 response provenance
    Ok(ScopeCtx { compiled, anchor_bounds, phase_bounds, window: scope.window.as_ref(), resolved_bounds })
```

## Response-transient loss/honesty derivation (companion — FR-14/FR-15, AC-06)

Because the return type stays `Option<TranscriptCandidatesSection>` (ARCH §12) and both
`TranscriptCandidatesSection` and `SessionLossInfo` are UNCHANGED, the explicit per-session
`matched` / `search_complete` projection is derived AFTER retrieval by a small pure function consumed
by the handler (cycle-review-handler.md step 3):

```
fn derive_search_status(section: Option<&TranscriptCandidatesSection>, scope: Option<&TranscriptScope>)
        -> (Vec<SessionSearchStatus>, Option<ResolvedBounds>):
    if section is None or scope is None: return ([], None)
    let matched_sessions = distinct(section.candidates.map(|c| c.session_id))
    let mut rows = []
    # returned sessions = union of sessions in candidates and sessions in loss
    for sid in union(matched_sessions, section.loss.map(|l| l.session_id)):
        let lossrow = section.loss.find(|l| l.session_id == sid)
        rows.push(SessionSearchStatus {
            session_id: sid,
            matched: if scope.r#match.is_some() { Some(matched_sessions.contains(sid)) } else { None },
            # any loss row present ⇒ one of the four conditions true ⇒ search_complete false (INDETERMINATE
            # for a no-match). No loss row + has candidates ⇒ clean ⇒ true (trustworthy negative).
            search_complete: lossrow.is_none(),
            elided_bytes: lossrow.map(|l| l.elided_bytes).unwrap_or(0),
            provenance: lossrow.map(|l| l.provenance).unwrap_or(Primary),
        })
    return (rows, resolved_bounds_from_scope(scope))    # ResolvedBounds carried out-of-band for FR-16
```

Key property (R-01): `search_complete == false` ⟺ a `SessionLossInfo` row exists for the session, and
a loss row exists ⟺ `elided_bytes>0 ∥ has_holes ∥ Reconstructed ∥ dropped>0` (by the UNCHANGED
`push_loss_if_any` predicate). So the four-condition derivation is structurally guaranteed and never
partial. A `match` no-match over a lossy session ⇒ `matched:Some(false)` + `search_complete:false` ⇒
INDETERMINATE. `match` NEVER collapses to a bare boolean — every match result carries its loss row.

**FLAG (open question, non-blocking):** the fixed return type `Option<TranscriptCandidatesSection>`
(architecture-sacred) forces `matched`/`search_complete` to be a post-derivation rather than a field on
the section. If the architect prefers the helper to own the projection, the return type would widen to a
new response-transient struct — flagged for delivery; the derived-companion approach here keeps
`TranscriptCandidatesSection`/`SessionLossInfo` byte-unchanged as the Component Map requires.

## Error handling

- No scope → `None` (not an error).
- `RetainDays` → `None`.
- Invalid `match` regex → `ScopeCtx` build returns `ERROR_INVALID_PARAMS`; the handler propagates
  (the helper's return type may need a `Result` wrapper for the regex error, OR compile the regex in the
  handler before calling — **FLAG**: pick one; simplest is compile-in-handler so the helper stays
  `-> Option<...>`). Recommended: validate/compile the regex in the handler, pass a compiled matcher into
  the scope context, so `retrieve_scoped_candidates` remains infallible-total and `-> Option<...>`.
- Corrupt/poison buffer → existing totality: degrades to zero candidates + a loss row, never panics (R-16).

## Key test scenarios

- `scope None` → `None`, and `take_transcripts_for_feature` is NOT called (no buffer read) — assert the
  early return precedes any snapshot (AC-01, lean default).
- Per-loss-condition matrix (R-01): no-match over a session with exactly one of `elided_bytes>0`,
  `has_holes`, `Reconstructed`, `dropped>0` → `matched:false`, `search_complete:false`, triggering field
  surfaced; OR-combination still false; clean `Primary` → `matched:false`, `search_complete:true` (may be
  omitted). No bare boolean anywhere.
- Loss row present on a MATCH too (positive match over a lossy session still surfaces loss).
- Scoped filter narrows: `phase ∧ match` strict subset (R-09).
- Second identical `transcript:{}` returns identical candidates (buffer survived) — non-destructive.
- Existing fixtures (`distill_handler.rs` tests) extended, not replaced (CON-7); source-assertion strings
  renamed to `retrieve_scoped_candidates(`.
