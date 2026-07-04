# Component: `context_cycle_review` handler

File: `unimatrix-server/src/mcp/tools.rs:2125` (handler); four success returns at
purge sites `:2379, :2558, :3328, :3451`.

## Purpose

Dispatch across four `result.is_ok()` success returns (purged-signals, cached-metrics, memo-hit,
full-pipeline). Thread the `transcript` scope to the retrieval helper at each; DELETE the four
`purge_cycle_transcripts` calls; keep the fold read gated ×4; drop the `"summary"` render alias.

## The four success returns (identical treatment — CON-2/#4750)

Express the success-return side-effect ONCE (mentally / via the shared helper calls) and apply
identically at all four sites, INCLUDING the memo-hit site (site 3, `:3317-3330`), which is the
highest drift risk and is enforced behaviorally, not by source assertion (ADR-004, CON-3).

Per success return, the sequence becomes:

```
# (report already built for this path; format already dispatched — render-dispatch.md)
result = <render dispatch for this path>          # "summary" arm dropped

# 1. Content-opaque FOLD READ — SOLE surviving success side-effect, gated here (activity-fold.md)
if result.is_ok():
    self.land_activity_fold(&feature_cycle, ...)   # crt-054/055; UNCHANGED behavior, stays at all 4 returns

# 2. Read-only scoped retrieval (distill-before-purge.md) — threads params.transcript
section = retrieve_scoped_candidates(
    &self.session_registry,
    &feature_cycle,
    &attributed,                                   # already-loaded observations — do NOT re-query
    &self.retention_config,
    params.transcript.as_ref(),                    # NEW — None ⇒ helper returns None (no buffer read)
    ctx.audit_ctx.session_id.as_deref(),           # NEW — reviewer_session_id (advisory only)
)

# 3. Derive response-transient loss/honesty projection (distill-before-purge.md)
(status_rows, resolved_bounds) = derive_search_status(section.as_ref(), params.transcript.as_ref())

# 4. Attach response-transient content (attach-to-response-assembly.md) — no-ops on None
attach_to_response_assembly(&mut result, section)
attach_search_status(&mut result, status_rows, resolved_bounds)

# 5. NO PURGE. The four `if result.is_ok() { self.purge_cycle_transcripts(&feature_cycle); }`
#    blocks (:2379,:2558,:3328,:3451) are DELETED. The review is fully non-destructive (FR-11).
return result
```

Notes tying to the real code:
- Sites 1 & 2 (`:2366`, `:2545`) sit inside the cached-metrics / purged-signals blocks; sites 3 & 4
  (`:3317`, `:3437`) are memo-hit and full-pipeline. Each currently reads
  `distill_before_purge(&self.session_registry, &feature_cycle, &attributed, &self.retention_config)`
  → rename call to `retrieve_scoped_candidates(...)` and add the two new args at ALL FOUR.
- The `land_activity_fold` step is the existing fold-read seam (activity-fold.md) — it is UNCHANGED and
  must remain at all four returns; the ONLY deletion in this handler is the purge block.

## Threading correctness (memo-hit parity — CON-3, AC-12)

The memo-hit path currently distills FRESH from call-time buffers even though `memo_report` is cached
(comment at `:3313-3316`). Preserve that: the scoped retrieval reads the LIVE buffer via `snapshot()`
regardless of whether the report was memoized. `params.transcript` must be threaded at the memo-hit
site identically to full-pipeline — this is not source-assertable, so the tester carries explicit
memo-hit behavioral rows (memo-hit + transcript present → scoped candidates + buffer intact; memo-hit +
no transcript → no candidates + buffer intact).

## Data flow / transformations

- IN: `RetrospectiveParams { transcript: Option<TranscriptScope>, format, force, ... }`.
- `params.transcript.as_ref()` → `Option<&TranscriptScope>` threaded unchanged to the helper.
- OUT: `Result<CallToolResult, ErrorData>` with 0..2 additive response-transient content items
  (candidates section, search-status). Report content itself is buffer-independent.

## Error handling

- Bad `format` (incl. `"summary"`) → `ERROR_INVALID_PARAMS` at render dispatch, BEFORE retrieval
  (render-dispatch.md). No retrieval runs on the error path.
- Invalid `match` regex → `ERROR_INVALID_PARAMS` surfaced from the helper (transcript-scope.md); the
  handler propagates it; no partial candidate attach.
- Retrieval never panics (helper is total on untrusted buffer input); a corrupt buffer degrades to
  loss rows, never an error (R-16).
- `attach_*` are no-ops on `Err(_)` and on `None` section — an error response is never rewritten.

## Key test scenarios

- Spy/trace across default, `json`, `force:true`, and every `transcript` shape: `purge_cycle_transcripts`
  NEVER invoked; buffer intact after each (synchronous read — R-10); repeat `transcript:{}` returns
  identical candidates (AC-03).
- Per-site path-proof rows (which of the four returns executed — #4452) for fold-landed integers,
  memo-hit non-optional (AC-04/AC-12, R-07).
- `force:true` + `transcript:{}` → report recomputed AND scoped slice returned; buffer intact (AC-09).
- Source-assertion suite updated: `purge_cycle_transcripts(` count → 0 in handler body with rationale
  comment; `retrieve_scoped_candidates(` / `attach_to_response_assembly(` counts stand at 4 (render/
  distill/orphan-deletion cross-refs).
