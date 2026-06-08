# Component: index-dispatch (`lib/hook-client/index.js`)

ADR-002 §4, ADR-004 §1, ADR-006. FR-14, FR-27 (sentinel handling), FR-30.
Risks R-11 (s4), R-14, R-04. Merge: transport selection in step 3; sentinel +
FR-16 rekey in step 4. Existing source: index.js (read in full).

## Purpose

Select the transport once per spawn from `config.mode` and inject it into the
pipeline (replacing the hard `require("./transport-http")`); short-circuit a `null`
build-request sentinel before any transport work; and rekey the offset delete to the
canonical `TaskCompleted` event while wiring `pruneOffsets` onto the FNF path.

## Modified: transport selection (ADR-002 §4, FR-14)

Today `index.js:24` does `const transport = require("./transport-http")`. Change to
import both and select per spawn:

```
const transportHttp = require("./transport-http")
const transportUds  = require("./transport-uds")

FUNCTION selectTransport(config):
  RETURN config.mode === "uds" ? transportUds : transportHttp
```

The selected module's `post` is passed wherever `transport.post` is used today:
`runSync`, `runFireAndForget` (carrying + delta), and `queue.replay(config,
transport.post)`. Queued frames replay over whichever transport THIS spawn selected
(SR-10 — accepted session-id split). `runSync`/`runFireAndForget` gain a `transport`
parameter (or read a module-scoped selection set in `main`).

```
// in main(), after config resolves ok:
transport = selectTransport(config)
// pass `transport` down to runSync(request, reqSource, config, transport)
//                    and runFireAndForget(request, input, config, transport, canonicalEvent)
```

## Modified: `null` sentinel short-circuit (ADR-004 §1, FR-27, R-11 s4)

`buildRequest` (via build-request-sentinel) may now return `null` for non-cycle
PreToolUse. Handle it BEFORE transport selection — no network, no stdout, exit 0.

```
// in main(), immediately after: request = buildRequestMod.buildRequest(effectiveEvent, input)
IF request === null:
    return                        // exit 0; no transport, no queue, no stdout (R-11 s4)
// ... SubagentStart fallback block stays AFTER this guard (it only runs when request
//     is a RecordEvent, never null) ...
```

Order matters: the `null` check precedes `selectTransport` and the SubagentStart
ContextSearch promotion. The SubagentStart block references `request.type`, so the
non-null guard must come first.

## Modified: `runFireAndForget` — FR-16 rekey + pruneOffsets (ADR-006)

Current (index.js:247-286): prunes queue, replays, posts carrying+delta, and on
`request.type === "SessionClose"` calls `deleteOffset`. Two changes per ADR-006:

```
FUNCTION runFireAndForget(request, input, config, transport, canonicalEvent):
  state.pruneOffsets(config.stateDir)        // NEW (ADR-006 §2): wire the 7-day prune live, FNF path only
  queue.prune(config.stateDir)               // unchanged (24 h queue age prune)
  await queue.replay(config, transport.post) // unchanged seam; transport injected

  sessionId = sessionIdOf(request)
  tasks = [ transport.post(config, request, { sync:false }) ]
  IF hasTranscript(input): tasks.push(delta.maybeSendDelta(..., config))
  results = await Promise.allSettled(tasks)

  carrying = settledSendResult(results[0])
  IF NOT carrying.ok:
      queue.enqueue(config.stateDir, request)             // NEVER a delta frame (ADR-004)
      stderrLine(carrying.failureClass, "send failed, event queued")
  ELSE IF canonicalEvent === "TaskCompleted":             // CHANGED (ADR-006 §3): was request.type==="SessionClose"
      state.deleteOffset(config.stateDir, sessionId)      // keyed by CANONICAL EVENT, never frame type
  // NOTE: a Stop spawn is ALSO a SessionClose frame but its canonicalEvent is "Stop"
  //       → delete does NOT fire (the assertable negative, ADR-006 / R-04 s2)

  // delta breadcrumb + recordSendOutcomes: UNCHANGED
```

### Why `pruneOffsets` here only (NFR-4, ADR-006 §2)

`pruneOffsets` runs on the FNF path exclusively — the sync trio gains no extra file
I/O. It is best-effort/fail-open (state.js already wraps it; unreadable dir / ENOENT
→ no-op). See state-offset-rekey.md.

### Passing `canonicalEvent`

`main()` already computes `canonical`/`effectiveEvent` from
`normalize.normalizeEventName(rawEvent)`. Pass the canonical event name into
`runFireAndForget` so the keying discriminates by event, not frame type:

```
// in main(): runFireAndForget(request, input, config, transport, canonical)
```

`canonical` is the normalized event (e.g. "Stop", "TaskCompleted", "SessionStart").
Use the canonical value, NOT `effectiveEvent` (which falls back to rawEvent on
UNKNOWN) and NOT `request.type`. The `TaskCompleted` branch is unreachable under
current HOOK_EVENTS (not registered — ADR-006, R-04) but pinned by unit test.

## Removed: SessionClose delete branch

The old `else if (request.type === "SessionClose") deleteOffset(...)` is replaced by
the canonical-event branch above. This is the per-turn re-stream fix (Stop fires
every turn and was deleting the offset each time).

## State / sequencing (unchanged shape, OVERVIEW data-flow)

`main` → parse → normalize → resolveCwd → config.resolve → buildRequest →
[null? return] → [SubagentStart promotion] → selectTransport → isFnf ?
runFireAndForget : runSync. No `process.exit()`; event loop drains after the awaited
transport promise settles.

## Error handling

All existing fail-open guards retained: top-level try/catch, stderr one-liners, exit
0 always, no stdout on failure. The new `pruneOffsets` and `selectTransport` calls
are non-throwing (state.js wraps; selectTransport is a pure dispatch). transport-uds
never rejects, so `Promise.allSettled` + `settledSendResult` handle it identically to
HTTP.

## Key test scenarios (hints for tester)

1. `config.mode==="uds"` selects transport-uds; `"http"` selects transport-http; the
   same `post` flows into `queue.replay` — FR-14.
2. `null` request → immediate return before transport selection: no network, no
   stdout, exit 0 — R-11 s4.
3. SubagentStart promotion still runs (request non-null RecordEvent → ContextSearch)
   — regression guard.
4. FR-16: a `TaskCompleted` canonical spawn deletes the offset after a successful
   carrying send; a `Stop` spawn (same SessionClose frame) does NOT delete — R-04
   s1/s2, AC-10.
5. Multi-turn: offsets persist across N Stop turns; deltas after turn 1 are true
   deltas (no re-stream from 0) — R-04 s3, AC-10.
6. `pruneOffsets` runs on FNF only (sync trio gains no file I/O); fail-open on
   unreadable dir/ENOENT — R-14 s2, NFR-4.
7. AC-12: full F3 HTTP suite passes; only delete-timing assertions change — R-14 s1.
8. Cross-transport replay: enqueue under UDS → next spawn HTTP replays; reverse too
   — AC-04 (driven via queue.replay + selected transport).
