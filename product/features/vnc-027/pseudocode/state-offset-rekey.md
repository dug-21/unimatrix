# Component: state-offset-rekey (`lib/hook-client/state.js`)

ADR-006 (amended — authoritative over spec FR-30/AC-10 "and/or"). FR-30, FR-31,
AC-10. Risks R-04, R-14. Merge step 4 (paired with index-dispatch FR-16 wiring).
Existing source: state.js (read in full).

## Purpose

Support the FR-16 offset-delete rekey: `deleteOffset` is invoked by index.js ONLY
when the carrying send succeeds AND the canonical event is `TaskCompleted` (keyed by
canonical event name, never frame type), and `pruneOffsets` (currently caller-less
dead code) becomes the sole effective deletion mechanism via the index.js FNF wiring.

## Key clarification: most of this file is UNCHANGED

The functions `deleteOffset(stateDir, sessionId)` (line 118) and
`pruneOffsets(stateDir)` (line 133) already exist and already do exactly what
ADR-006 needs. The behavioral change is in the CALLER (index.js), not here:

- `deleteOffset` — signature and body unchanged. It still unlinks the offset file
  fail-open. What changes is WHO calls it and WHEN (index.js: canonical event
  `TaskCompleted`, not `request.type === "SessionClose"`). See index-dispatch.md.
- `pruneOffsets` — signature and body unchanged. It is currently caller-less
  (verified: ADR-006 context). It goes live because index.js's `runFireAndForget`
  now calls `state.pruneOffsets(config.stateDir)` on the FNF path. See
  index-dispatch.md.

## Doc-comment corrections (FR-30 traceability, no logic change)

Update the doc comments to reflect the amended ADR-006 so future readers do not
re-introduce the SessionClose-delete bug:

```
// deleteOffset (line 117): change the comment from
//   "Delete a session's offset file (on successful SessionClose — FR-16)."
// to
//   "Delete a session's offset file. Fired by index.js ONLY when the carrying send
//    succeeds AND the canonical event is TaskCompleted (ADR-006 vnc-027 — keyed by
//    canonical event name, NEVER frame type; Stop and TaskCompleted both build
//    SessionClose frames). Unreachable under current HOOK_EVENTS; pinned by unit
//    test. Fail-open."

// pruneOffsets (line 128-132): the existing comment already says "called
//   opportunistically on FNF spawns after replay" — now TRUE (index.js wires it).
//   Confirm the comment still matches; no edit needed if it already reads so.
```

These are comment-only edits; with the new size gate (comment-stripped) they cost no
shipped-logic budget.

## Unchanged behavioral contracts (must stay byte-stable — R-14 s3)

- `OFFSET_PRUNE_SECS = 7 * 24 * 60 * 60` (7-day cutoff).
- `pruneOffsets`: reads `offsets/`; for each `*.json`, prunes when `updated` (JSON,
  mtime fallback) `< now - 7d`; skips `.tmp-*`; fail-open on unreadable dir
  (readdir throws → return), unreadable file (→ mtime), unlink error (→ best-effort).
- `deleteOffset`: `unlinkSync` wrapped, returns false on failure; never throws.
- `readOffset`/`writeOffset`/`sanitizeSessionKey`/`atomicWrite`/breadcrumb functions:
  untouched. Offset write cadence and format unchanged (SR-08 scope guard).

## Behavioral consequences (ADR-006 §2-3)

- Stop fires every turn but is canonical event `"Stop"`, not `"TaskCompleted"` →
  `deleteOffset` does NOT fire → offsets persist across turns → deltas are true
  deltas (no per-turn re-stream from 0). This is the FR-16 defect fix.
- `TaskCompleted` is registered nowhere (not in HOOK_EVENTS / settings.json) →
  the delete branch is unreachable end-to-end, retained as a zero-cost forward
  provision, pinned by unit test (R-04 s1; assertable negative R-04 s2).
- Effective deletion = 7-day age-prune only. A mid-session pruned offset degrades to
  one full re-stream — safe (idempotent server-side merge, R-04 s4).

## Data flow

index.js `runFireAndForget` → `state.pruneOffsets(stateDir)` (top of FNF, after which
queue.prune/replay/sends run) and, on `carrying.ok && canonical==="TaskCompleted"`,
`state.deleteOffset(stateDir, sessionId)`. No sync-path call (NFR-4 budget).

## Error handling

Already fail-open throughout (module contract: no function throws). `pruneOffsets`
I/O errors are swallowed best-effort (R-14 s2). No new error paths introduced.

## Key test scenarios (hints for tester)

1. Pinning: a spawn whose canonical event is `TaskCompleted` deletes the offset after
   a successful carrying send (proves the branch works if the event ever arrives) —
   R-04 s1, AC-10.
2. Assertable negative: a `Stop` spawn (also a SessionClose frame) does NOT delete the
   offset — keying discriminates by canonical event, not frame type — R-04 s2, AC-10.
3. Multi-turn integration: offsets persist across N Stop turns; delta sends after turn
   1 are true deltas — R-04 s3, AC-10.
4. `pruneOffsets` deletes only files older than 7 days; mid-session prune degrades to
   one full re-stream with no error path — R-04 s4.
5. `pruneOffsets` fail-open: unreadable dir / ENOENT → no-op, spawn proceeds — R-14 s2.
6. Offset write cadence, format, 1 MiB caps, never-queue-delta rule pinned unchanged —
   R-14 s3, AC-12.
