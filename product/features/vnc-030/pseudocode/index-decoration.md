# index.js — FNF Stamp Decoration + Lifecycle Dispatch

**Source**: `packages/unimatrix/lib/hook-client/index.js` (extend). **ADR**:
ADR-002 (+ §7 transport ride). **Also covers**: AC-10/FR-29 UDS-stamp regression
(test seam, production logic is the decoration here, upstream of `selectTransport`).
**Constraints**: C-04 fail-open, C-06 sync-path budget (sync trio gains ZERO
I/O — decoration is FNF-only), C-11 no raw-cwd hashing, SR-09 minimal footprint
on the vnc-027-contended file (`index.js` orchestration only; zero
`build-request*.js` change).

## Purpose

Single decoration seam between `buildRequest` and transport dispatch that:
1. dispatches cycle-tracker lifecycle on CYCLE_* frames (cycles.writeCycle/
   updatePhase/deleteCycle);
2. attaches `cycle_stamp` to every ImplantEvent while a tracker exists;
3. strips `topic_signal` from non-CYCLE_* frames (suppression, AC-03);
4. on a tracker miss, runs the subagent-gated `stamp_miss` canary.

`buildRequest`/`build-request-tools.js` stay byte-pure — all tracker I/O lives
here (only `index.js` has `config.stateDir`).

## New Requires

```
const cycles = require("./cycles");
const { CYCLE_START_EVENT, CYCLE_PHASE_END_EVENT, CYCLE_STOP_EVENT } = require("./build-request");
   // already re-exported by build-request.js (build-request.js:143-145)
```

## Ordering inside main() (anchors against current index.js)

Insert the decoration call in `main()` between `selectTransport` (`:410`) and
the `runFireAndForget` call (`:414`), guarded by `isFnf`:

```
... existing through:
  if (request === null) return;                 // :366 sentinel — UNCHANGED, precedes all
  ... SubagentStart promotion ...               // :373-395 UNCHANGED
  const isFnf = ...;                            // :402-406 UNCHANGED
  const transport = selectTransport(config);   // :410 UNCHANGED

  if (isFnf) {
+   decorateCycleStamp(request, input, config); // NEW SEAM — mutates request in place
    await runFireAndForget(request, input, config, transport, canonical);   // :414 UNCHANGED
  } else {
    await runSync(request, reqSource, config, transport);   // sync path — NO decoration
  }
```

Why before `runFireAndForget` and not inside it: keeps the mutation provably
upstream of BOTH `transport.post` AND `queue.replay` (both run inside
runFireAndForget). The enqueue-on-failure at `index.js:286` stores the
post-decoration `request` ⇒ replayed frames carry the stamp true at event time
(ADR-002 §2.5). Either placement satisfies it as long as decoration precedes
`queue.replay` and `transport.post`; placing it at the call site is the smallest
readable diff and keeps `runFireAndForget`'s signature unchanged.

## Helpers

### `frameEvents(request) -> Array<ImplantEvent>`

The decoration must iterate BOTH single and batch shapes (R-06). Only
`RecordEvent`/`RecordEvents` carry ImplantEvents; `SessionRegister`/`SessionClose`
do not.
```
switch request.type:
  case "RecordEvent":  return [request]                 // RecordEvent IS a flattened ImplantEvent
  case "RecordEvents": return request.events ?? []
  default:             return []                          // SessionRegister/SessionClose → no events
```
Note: a `RecordEvent` frame is `{type:"RecordEvent", event_type, session_id,
timestamp, payload, topic_signal?, provider?}` (build-request-tools.js
`recordEventFrame`), so mutating the frame object IS mutating the ImplantEvent.

### `eventTypeOf(ev)` / `isCycleEvent(ev)`

```
isCycleEvent(ev) := ev.event_type === CYCLE_START_EVENT
                 || ev.event_type === CYCLE_PHASE_END_EVENT
                 || ev.event_type === CYCLE_STOP_EVENT
```

### `subagentContext(input)` — depth ≥ 1 detector (OQ-E dependent)

```
// Branch A (signals independent, primary): detect "I am a subagent" from hook
// stdin INDEPENDENTLY of root-id inheritance. Candidate signals on input:
//   - canonical event === "SubagentStart"  (effectiveEvent / input)
//   - input.extra.agent_type present (subagent role marker)
//   - any depth indicator Claude Code emits on stdin (delivery probe, OQ-E)
// Returns { isSubagent: bool, rootSessionId: string|null }
// rootSessionId is the inherited root id the subagent event carries (= sessionIdOf today).
```
**OQ-E gate**: if the delivery probe finds these signals are co-dependent with
root-id inheritance (Branch B), `subagentContext` cannot reliably report
`isSubagent` under drift → the PRODUCTION canary increment is dropped; the
test-time invariant still ships (state-canary.md). Pseudocode below keeps the
increment call site; whether it is reached in production is the probe outcome.

## `decorateCycleStamp(request, input, config) -> void` (the seam)

All wrapped; never throws; mutates `request` in place; no return value.
```
try:
  stateDir := config.stateDir
  sid := sessionIdOf(request)               // existing index.js helper (:208)
  if sid is null/empty: return              // nothing to key on
  events := frameEvents(request)
  if events.length === 0: return            // SessionRegister/SessionClose — skip entirely

  // -- (1) LIFECYCLE DISPATCH (before decoration; cycle_stop stays unstamped) --
  for ev in events:
    if ev.event_type === CYCLE_START_EVENT:
        cycles.writeCycle(stateDir, sid, ev.payload.feature_cycle, payloadNextPhase(ev))
    else if ev.event_type === CYCLE_PHASE_END_EVENT:
        cycles.updatePhase(stateDir, sid, payloadNextPhase(ev))
    else if ev.event_type === CYCLE_STOP_EVENT:
        cycles.deleteCycle(stateDir, sid)
    // payloadNextPhase(ev) := (typeof ev.payload?.next_phase === "string") ? ev.payload.next_phase : null

  // -- (2) DECORATION: one readCycle --
  tracker := cycles.readCycle(stateDir, sid)         // {topic, phase}|null
  if tracker !== null:
    for ev in events:
      ev.cycle_stamp := { topic: tracker.topic }
      if tracker.phase !== null and tracker.phase !== undefined:
        ev.cycle_stamp.phase = tracker.phase         // omit-when-null parity (ADR-003 §4)
      if not isCycleEvent(ev):
        delete ev.topic_signal                        // SUPPRESSION (AC-03). CYCLE_* keep it.
  else:
    // -- (3) CANARY (subagent-gated; ADR-006) --
    ctx := subagentContext(input)
    if ctx.isSubagent and ctx.rootSessionId:
      // root id carried but no tracker for it = inheritance drift
      if cycles.readCycle(stateDir, ctx.rootSessionId) === null:
        state.bumpStampMiss(stateDir)
    // depth-0 / non-subagent miss → NO increment, NO extra read (never-declare = structural noise)
catch (_e):
  // last-resort: decoration never escalates; request sent as-is (unstamped)
  return
```

Notes:
- `ctx.rootSessionId` and `sid` are the same value today (depth-1: root ≡
  carried). The second `readCycle(rootSessionId)` is redundant with the first
  `readCycle(sid)` in the depth-1 case, but written explicitly so depth>1
  forward-compat (grandchild carrying an intermediate id) still trips the canary
  (ADR-006 §4, R-14). Delivery MAY collapse to the already-computed `tracker`
  when `rootSessionId === sid`.
- Suppression is STRIP-AT-DECORATION on non-CYCLE_* frames only (R-05): over-strip
  (CYCLE_* losing its declaration) and under-strip (stamped + extracted both
  present) are both wrong. CYCLE_* frames keep `topic_signal = topic` — byte-
  identical to the Rust hook's cycle frames (parity).
- TranscriptDelta frames are NOT stamped: they are F2-buffer-bound, not
  observation rows (ADR-002 §4). They flow through as RecordEvent but the server
  early-returns on `TRANSCRIPT_DELTA_EVENT` (listener.rs:838) before any
  stamp read, so a stray stamp on a delta is harmless; still, do not special-case
  them here — the decoration loop stamps them but the server ignores it.

## pruneCycles piggyback

In `runFireAndForget`, beside the existing `state.pruneOffsets` / `queue.prune`
(`index.js:267-268`), add (best-effort, wrapped):
```
cycles.pruneCycles(config.stateDir);     // 7-day prune, FNF path only (C-06)
```
This keeps the sync trio I/O-free (decoration + prune are FNF-only).

## UDS-Path Stamp Ride (ADR-002 §7) — production behavior, AC-10 verifies

No code change beyond the decoration above. Because `decorateCycleStamp` mutates
the in-memory `request` BEFORE `selectTransport` returns and BEFORE
`runFireAndForget` calls `transport.post`/`queue.replay`, both transports
serialize the SAME object:
- HTTP: `transport-http.js:74` `JSON.stringify(frame)`
- UDS:  `transport-uds.encodeFrame` → `:62` `Buffer.from(JSON.stringify(payloadObj))`

`cycle_stamp` is therefore byte-identical on the UDS frame and the HTTP body. The
AC-10/FR-29 regression (test-plan/uds-stamp-regression.md) drives a stamped FNF
`RecordEvent` through `runFireAndForget` with `config.mode = "uds"` and asserts
`transport-uds.encodeFrame`'s bytes decode to a payload containing `cycle_stamp`,
byte-equivalent to the HTTP body for identical input (R-23). No vnc-030 edit to
`transport-uds.js`.

## State Machine (per FNF spawn)

```
buildRequest → [null? → exit0]
            → isFnf? → decorateCycleStamp:
                 events=[]            → skip
                 CYCLE_START present  → writeCycle, then stamp this frame too
                 CYCLE_PHASE_END      → updatePhase, then stamp
                 CYCLE_STOP           → deleteCycle (frame then unstamped — tracker gone)
                 tracker present      → stamp all + strip topic_signal on non-CYCLE_*
                 tracker absent       → subagent+drift? bumpStampMiss : nothing
            → runFireAndForget (post-decoration request; replay/enqueue carry stamp)
```

## Error Handling

`decorateCycleStamp` is fully wrapped in try/catch and every internal call is a
never-throw `cycles.*`/`state.*` function. A failure leaves `request` unstamped
and the event is still sent — exit 0, no stdout. The top-level `main()` try/catch
(`index.js:418`) remains the last-resort guard.

## Key Test Scenarios

- Seam-survival (FR-28, gates before server work): `context_cycle(start)`
  PreToolUse → tracker created (cycles.writeCycle) AND CYCLE_START frame sent with
  `cycle_stamp` (request !== null, not the :326 sentinel). Non-cycle PreToolUse →
  :326 sentinel, `:366` exit 0, no tracker touch, no `bumpStampMiss`, no network.
- Multi-turn (R-02): cycle_start → 3× (Stop + RecordEvent) → every post-Stop
  RecordEvent still finds the tracker and stamps (Stop builds SessionClose, which
  has no events → lifecycle dispatch no-op; tracker untouched).
- Suppression (R-05): same prompt with/without tracker → with → `cycle_stamp`
  present, no `topic_signal`; without → `topic_signal`, no stamp. CYCLE_* frame
  keeps `topic_signal = topic`.
- Batch (R-06): `RecordEvents` of mixed CYCLE_*/RecordEvent → every event in the
  batch carries the stamp; `topic_signal` stripped on every non-CYCLE_* member.
  Send-failure → enqueue → replay carries the stamp (post-decoration enqueue).
- Canary: depth-0 never-declare miss → no increment; depth≥1 subagent with
  inherited root tracker present → no increment; depth≥1 carrying a non-inherited
  id while root tracker exists → exactly one increment; depth>1 grandchild id with
  no tracker → lands in stamp_miss (state-canary.md fixtures own the assertions).
- cycle_stop frame goes unstamped (lifecycle deletes before decoration reads).
- UDS regression (AC-10/R-23): stamped RecordEvent over `config.mode="uds"` →
  `encodeFrame` bytes decode to payload with `cycle_stamp`, byte-equiv to HTTP.
- Fail-open: throw injected anywhere in decoration → request sent unstamped, exit 0.

## Open Questions / Gaps

- **OQ-E (canary independence)** governs whether the `subagentContext` →
  `bumpStampMiss` call site fires in production. Branch A: ship it. Branch B:
  gate the increment behind a constant/flag that the test fixtures still exercise
  but production sets to no-op (test-time invariant only). The decoration's
  stamp/suppress/lifecycle paths are unaffected by the branch.
- `subagentContext` detection signal is delivery-probe-defined (which stdin field
  marks depth≥1). Pseudocode names the candidates; the probe pins the field.
