# crt-057 Pseudocode — OVERVIEW

Fully non-destructive `context_cycle_review` with scoped, honest transcript retrieval.
Grounded in ARCHITECTURE §3/§7/§12, ADR-001..006, IMPLEMENTATION-BRIEF Component Map.
Every interface name below is traced to the architecture Integration Surface or existing code —
none invented.

## Chosen delivery decision: function rename (OQ-4 → RENAME)

`distill_before_purge` (`mcp/distill_handler.rs:48`) is **RENAMED** to:

> **`retrieve_scoped_candidates`**

Rationale: the function no longer precedes any purge; it performs a read-only, scoped retrieval and
returns `None` when no scope is supplied. The new name states exactly that. The source-assertion
tests that count the string `distill_before_purge(` (`distill_handler.rs:651-726`) MUST be updated to
count `retrieve_scoped_candidates(` — called out for the tester in `distill-before-purge.md` and
`render-dispatch.md`. (Component-map pseudocode file keeps its path `distill-before-purge.md` so the
Stage-3a 1:1 path map to the test plan is preserved; the function inside is the renamed one.)

## Components (one pseudocode file each)

| # | Component | File | Change |
|---|-----------|------|--------|
| 1 | `RetrospectiveParams` | retrospective-params.md | remove `include_transcript_candidates` (already absent here — never merged; confirm), add `transcript`, drop `"summary"` doc |
| 2 | `TranscriptScope` [NEW] | transcript-scope.md | new filter block + serde |
| 3 | `Window` [NEW] + clock normalization | window.md | new type + default + boundary helper |
| 4 | `context_cycle_review` handler | cycle-review-handler.md | thread scope; delete 4 purge calls; drop summary; gate fold read |
| 5 | `retrieve_scoped_candidates` (was `distill_before_purge`) | distill-before-purge.md | scope + clock norm + loss + search status; `None` when scope `None` |
| 6 | `attach_to_response_assembly` | attach-to-response-assembly.md | UNCHANGED core; new search-status attachment step |
| 7 | `snapshot()` reuse | snapshot-reuse.md | UNCHANGED — sole content reader |
| 8 | orphan deletion + re-home | orphan-deletion.md | delete purge fns; re-home exhaustive retention match |
| 9 | backstop reclaim | backstop-reclaim.md | sole reclamation path; re-homed exhaustive match |
| 10 | content-opaque fold read | activity-fold.md | UNCHANGED; stays gated ×4 |
| 11 | render dispatch | render-dispatch.md | drop `"summary"` arm ×4 |
| 12 | consumer reconciliation | consumer-reconciliation.md | SKILL.md + tool description edit specs |
| 13 | retro lifecycle | retro-lifecycle.md | both protocols edit specs |

## Shared Types (AUTHORITATIVE — component files reference these, never redefine)

All new types live in `unimatrix-observe/src/types.rs` next to the UNCHANGED
`TranscriptCandidate` / `SessionLossInfo` / `CandidateProvenance` / `TranscriptCandidatesSection`
(`types.rs:590-663`). All are response-transient; none is a field on `RetrospectiveReport`.

```
// INPUT surface — deserialized from tool params (ADR-002 / ARCH §12)
struct TranscriptScope {          // [NEW] all-optional, AND-composed
    phase:  Option<String>        // phase id → cycle_events bounds; self-bounding (ignores window)
    anchor: Option<String>        // finding id → HotspotFinding.evidence[].ts span [min,max]
    r#match: Option<String>       // #[serde(rename="match")] regex over whole TranscriptCandidate.text
    window: Option<Window>        // ±T ms / ±N blocks; modifies anchor/match; ignored by phase
}

struct Window {                   // [NEW]  serde: { millis?: u64, blocks?: u32 }
    millis: Option<u64>           // time radius for ts-bearing candidates; default DEFAULT_WINDOW_MILLIS
    blocks: Option<u32>           // byte_offset block radius for ts:None candidates; default DEFAULT_WINDOW_BLOCKS
}
const DEFAULT_WINDOW_MILLIS: u64 = 120_000   // ±2 min (ADR-006 / FR-18 / AC-18)
const DEFAULT_WINDOW_BLOCKS: u32 = 3         // ±3 candidate blocks (ts:None fallback)

// OUTPUT surface — response-transient loss/honesty projection (ADR-003 / FR-14-16)
struct SessionSearchStatus {      // [NEW] built at attach time; NEVER persisted
    session_id: String
    matched: Option<bool>         // Some(bool) only when scope.r#match supplied; None for anchor/phase-only
    search_complete: bool         // false iff elided_bytes>0 || has_holes || Reconstructed || dropped>0
    elided_bytes: u64
    provenance: CandidateProvenance
}
struct ResolvedBounds {           // [NEW] response-transient; anchor/phase window provenance (FR-16)
    kind: "anchor" | "phase"
    lo_epoch_ms: u64
    hi_epoch_ms: u64
}

// UNCHANGED (types.rs:590-663) — reference only, do not modify:
//   TranscriptCandidate { session_id, byte_offset:u64, ts:Option<String>, family_hints, text }
//   SessionLossInfo { session_id, elided_bytes:u64, has_holes:bool, provenance, dropped_candidates:u64 }
//   enum CandidateProvenance { Primary, Reconstructed }
//   TranscriptCandidatesSection { candidates:Vec<TranscriptCandidate>, loss:Vec<SessionLossInfo> }
```

### Renamed function signature (ARCH §12; brief Key Signatures)

```
fn retrieve_scoped_candidates(
    registry: &SessionRegistry,
    feature_cycle: &str,
    observations: &[ObservationRecord],
    cfg: &RetentionConfig,
    scope: Option<&TranscriptScope>,       // NEW — returns None early when None
    reviewer_session_id: Option<&str>,     // NEW — reserved for optional live-sibling advisory (ADR-003; not a contract)
    resolved_bounds: Option<ResolvedBounds>, // NEW 7th — anchor/phase bounds resolved by the handler (ADR-006 as-built)
) -> Option<TranscriptCandidatesSection>

// companion handler helper (ADR-006) — expressed once, invoked at all four success returns
fn resolve_transcript_scope_bounds(
    scope: Option<&TranscriptScope>,
    hotspots: &[HotspotFinding],        // anchor F-NN (POSITIONAL over report.hotspots) → evidence[].ts span [min,max]
    cycle_events: &[CycleEventRecord],  // phase → compute_phase_stats window (sec→ms; self-bounding)
) -> Option<ResolvedBounds>            // None ⇒ absent section (FR-7), never an error
```

`anchor`/`phase` resolve end-to-end only on the full-pipeline and `force` returns (where `hotspots`/
`cycle_events` are in scope); the cached-`MetricVector` / memo-hit / purged degenerate returns resolve
`anchor`/`phase` to an ABSENT section (in-code, honest, never an error). `match`/`window`/clock-norm/
loss-honesty stay reachable on all paths.

## Data Flow (one of four success returns)

```
context_cycle_review(params)
  ├─ parse format  → "markdown" | "json" | else ERROR_INVALID_PARAMS  (render-dispatch.md; "summary" gone)
  ├─ force?        → report path selection over durable observations   (unchanged)
  ├─ build report  → build_report(records)  [NO transcript arg — buffer-independent, ARCH §5]
  ├─ FOLD READ     → activity_snapshots_for_feature (gated at all 4 returns; SOLE side-effect)  (activity-fold.md)
  ├─ bounds = resolve_transcript_scope_bounds(scope, report.hotspots, cycle_events)  (ADR-006 as-built)
  │     anchor F-NN → hotspot evidence[].ts span; phase → compute_phase_stats window
  │     out-of-scope returns (cached/memo/purged) have no hotspots/cycle_events → None → absent section
  ├─ section = retrieve_scoped_candidates(reg, cycle, obs, cfg, scope, reviewer_sid, bounds)  (distill-before-purge.md)
  │     scope None  → None (no buffer read — lean default)
  │     scope Some  → snapshot() → scoped filter (phase/anchor/match+window) → clock-normalize
  │                   → candidates + per-session SessionLossInfo
  ├─ status = derive_search_status(section, scope)  → Vec<SessionSearchStatus> + ResolvedBounds
  ├─ attach_to_response_assembly(&mut result, section)          (unchanged; no-op on None)
  ├─ attach_search_status(&mut result, status, bounds)          (new; no-op when empty)
  └─ (NO purge — fully non-destructive; buffer survives)         (4 purge calls DELETED)
```

Crosses component boundaries: `TranscriptScope` (params → handler → retrieval), `snapshot()` output
(retrieval-internal only), `TranscriptCandidatesSection` (retrieval → attach), `SessionSearchStatus`/
`ResolvedBounds` (derive → attach). Nothing transcript-derived crosses into `build_report`
(summary ⟂ Plane-B invariant, ARCH §2/§5).

## Sequencing constraints (what must be built first)

1. Shared types (`transcript-scope.md`, `window.md`) before handler/retrieval — everything depends on them.
2. `retrieve_scoped_candidates` rename + new signature before handler threads it.
3. Orphan deletion (`orphan-deletion.md`) AFTER the four purge calls are removed from the handler
   (`cycle-review-handler.md`) — deletion of `purge_cycle_transcripts` only compiles once its callers are gone.
4. Re-home exhaustive retention match (`orphan-deletion.md` + `backstop-reclaim.md`) in the same change
   as the purge deletion (the match currently lives inside the deleted function).
5. Consumer/protocol docs (`consumer-reconciliation.md`, `retro-lifecycle.md`) ship in the SAME atomic
   unit as the server change (CON-1) — no partial ship.

## Cross-cutting invariants (apply to every component)

- CON-2/#4750: fold-read gate + scope threading expressed once, applied identically at all four returns
  incl. memo-hit (site 3). Purge-count / attach-before-purge source assertions REMOVED with in-source
  rationale; fold-read ×4 assertion PRESERVED.
- CON-3/#4848: retrieval reuses existing `snapshot()`; no new buffer reader.
- CON-4: never-persist-raw-to-disk absolute; candidates + loss + `SessionSearchStatus` + `ResolvedBounds`
  all response-transient, outside the memoized `RetrospectiveReport`.
- CON-5: `format` accepts exactly `markdown|json`; unknown → `ERROR_INVALID_PARAMS`
  (`Unknown format '…'. Valid values: "markdown", "json".`).
- CON-7 / 500-line limit: extend existing `distill_handler.rs` + `types.rs`; no isolated scaffolding.
