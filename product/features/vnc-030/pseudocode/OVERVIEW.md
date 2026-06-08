# vnc-030 Pseudocode — OVERVIEW

Contractual Cycle Attribution (F4b). Component interaction, data flow, shared
types, the FNF decoration sequence, and the server precedence chain. Anchors are
real `file:line` against the merged-vnc-027 tree (verified 2026-06-08).

## Components (map → ARCHITECTURE.md C1–C9)

| File | Component | Source (change) | ADR |
|------|-----------|-----------------|-----|
| cycles.md | Cycle tracker module (NEW) | `lib/hook-client/cycles.js` | ADR-001 |
| index-decoration.md | FNF stamp decoration + lifecycle dispatch + UDS regression seam | `lib/hook-client/index.js` (extend) | ADR-002, §7 |
| state-canary.md | `bumpStampMiss`, `stamp_miss` breadcrumb default | `lib/hook-client/state.js` (extend) | ADR-006 |
| wire-cycle-stamp.md | `CycleStampPayload` + `ImplantEvent.cycle_stamp`; 7th ts-rs export | `unimatrix-engine/src/wire.rs` (extend) | ADR-003 |
| feature-source.md | `FeatureSource`/`InferredOrigin`, `apply_stamp`, sweep flip | `unimatrix-server/src/infra/session.rs` (extend) | ADR-004 |
| listener-stamp-read.md | 3-site stamp read, `topic_source`, close flip, enrich guard, INSERTs | `unimatrix-server/src/uds/listener.rs` (extend) | ADR-004/005 |
| topic-source-migration.md | v27→v28 ALTER, `CURRENT_SCHEMA_VERSION = 28` | `unimatrix-store/src/migration.rs` (extend) | ADR-005 |
| protocol-redeclaration.md | restart re-declaration line ×3 | `.claude/protocols/uni/uni-*-protocol.md` | AC-09 |
| docstring-driveby.md | `{alpha}-{digits}` docstring fix (comment-only) | `attribution.rs` + `topic-signal.js` | ADR-004 §C9 / FR-25 |

UDS-path stamp regression (AC-10/FR-29) is **test-only** — its production logic
is the decoration in index-decoration.md, upstream of `selectTransport`
(`index.js:410`). No vnc-030 source change to `transport-uds.js`.

## Shared Types (single source of truth — component files must match)

```
Tracker file (ADR-001)
  path: {config.resolve(cwd).stateDir}/cycles/{sanitizeSessionKey(sid)}.json
  shape: { "topic": string, "phase": string|null, "declared_at": secs, "updated": secs }

cycle_stamp (wire field, ADR-003)
  ImplantEvent.cycle_stamp: Option<CycleStampPayload>
  CycleStampPayload { topic: String, phase: Option<String> }
  serde(default, skip_serializing_if="Option::is_none") on the field AND on phase
  JS attach shape: cycle_stamp: { topic, phase? }   // phase key omitted when null;
                                                     // whole key omitted when no tracker

FeatureSource (registry precedence class, ADR-004)
  enum FeatureSource { Declared, Inferred(InferredOrigin) }
  enum InferredOrigin { Registered, Voted }
  SessionState.feature_source: FeatureSource   // default Inferred(Registered)
  precedence determinant is ONLY matches!(src, FeatureSource::Declared)

ObservationRow gains (ADR-005)
  topic_source: Option<String>    // 'declared'|'extracted'|'registry-fill'|'vote'|NULL
  observations.topic_source TEXT NULL ; bound as ?10 at both listener-local INSERTs

stamp_miss (canary, ADR-006)
  health.json gains stamp_miss: 0 (default). Count only — never topic/sid/path.
```

## FNF Decoration Sequence (client, index.js main()/runFireAndForget)

Anchors: `buildRequest` at `index.js:360`; null sentinel guard at `:366`;
`selectTransport` at `:410`. Decoration is inserted AFTER the null guard and
SubagentStart promotion, BEFORE `selectTransport`, and ONLY for FNF frames.

```
1. request = buildRequest(effectiveEvent, input)            [index.js:360, unchanged]
2. if request === null: return (exit 0)                     [index.js:366 sentinel]
       -- NO tracker touch, NO canary bump, NO decoration. Precedes everything.
3. SubagentStart ContextSearch promotion                    [index.js:373-395, unchanged]
4. isFnf = type in {SessionRegister, SessionClose, RecordEvent, RecordEvents}
5. transport = selectTransport(config)                      [index.js:410, unchanged]
6. if isFnf:  decorateCycleStamp(request, input, config)    <<< NEW SEAM (this feature)
       -- mutates `request` IN PLACE, upstream of transport.post and queue.replay
7. runFireAndForget(request, input, config, transport, canonical)  [unchanged call]
```

`decorateCycleStamp(request, input, config)` (single seam, both transports ride it):
```
sessionId := sessionIdOf(request)              // existing index.js helper
if sessionId is null/empty: return             // nothing to key on; fail-open

LIFECYCLE DISPATCH (keys on frame event_type, NEVER on canonical/lifecycle name):
  for each ImplantEvent ev in frameEvents(request):     // single OR batch
    if ev.event_type == CYCLE_START_EVENT:      cycles.writeCycle(stateDir, sid, ev.payload.feature_cycle, ev.payload.next_phase ?? null)
    elif ev.event_type == CYCLE_PHASE_END_EVENT: cycles.updatePhase(stateDir, sid, ev.payload.next_phase ?? null)
    elif ev.event_type == CYCLE_STOP_EVENT:      cycles.deleteCycle(stateDir, sid)
  -- runs BEFORE decoration so cycle_stop frame itself goes unstamped (ADR-002 §5)

DECORATION (one readCycle):
  tracker := cycles.readCycle(stateDir, sid)
  if tracker present:
    for each ImplantEvent ev:
      ev.cycle_stamp := { topic: tracker.topic }; if tracker.phase != null: ev.cycle_stamp.phase = tracker.phase
      if ev.event_type is NOT a CYCLE_* type: delete ev.topic_signal     // suppression, AC-03
  else (tracker absent):
    CANARY (subagent-gated, ADR-006): if event is subagent-context (depth>=1) AND
        readCycle(stateDir, rootSidCarried) is null → state.bumpStampMiss(stateDir)
    depth-0 / non-subagent → no increment, no readdir
```

Sequencing rules baked in: lifecycle dispatch precedes decoration (cycle_stop
unstamped); decoration precedes `runFireAndForget` so queue.enqueue on send
failure stores the POST-decoration request (replay carries the stamp). Both HTTP
(`transport-http.js:74`) and UDS (`transport-uds.encodeFrame:62`) `JSON.stringify`
the same mutated `request` ⇒ `cycle_stamp` byte-identical on both wires (AC-10).

## Server Precedence Chain (presence-gated — structurally un-invertable)

Write-time, per record site (3 sites, ONE shared helper `apply_stamp_to_row`):
```
if event.cycle_stamp is Some(stamp):
    obs.topic_signal := stamp.topic
    obs.phase        := stamp.phase  ?? registry.current_phase
    obs.topic_source := 'declared'
    registry.apply_stamp(session_id, stamp.topic)     // idempotent Declared set
    SKIP record_topic_signal tally  AND  SKIP enrich_topic_signal
else if event is a CYCLE_* event:
    obs.topic_source := 'declared'   (topic_signal already = the declaration)
else:
    (signal, source) := enrich_with_source(event.topic_signal, session_id, registry)
        // ADR-004 §4 decision tree, see listener-stamp-read.md
    obs.topic_signal := signal ; obs.topic_source := source
```

Close/sweep (session-level, both flips minimal-diff):
```
resolved := if feature_source == Declared && feature.is_some()
              then feature                       // declared wins (inversion fixed)
              else majority_vote(...).or_else(feature)   // today's order, NULL-gated
```
- sweep flip: `session.rs:628` (feature-source.md)
- close flip: `process_session_close` snapshot at `listener.rs:1951`, branch at
  the `final_feature_cycle` computation `:2010` (listener-stamp-read.md)

## Sequencing Constraints (what must be built / validated first)

1. **wire-cycle-stamp** (ADR-003) defines the field both client and server read —
   build/verify the struct + ts-rs export before the read sites assume it.
2. **Seam-survival test (FR-28/ADR-007 §1)** gates BEFORE any server-work
   validation: a `context_cycle(start)` PreToolUse must reach `cycles.writeCycle`
   and emit a CYCLE_START frame with `cycle_stamp` (not the `build-request-tools.js:326`
   null sentinel); a non-cycle PreToolUse yields the sentinel, no tracker touch.
3. **topic-source-migration** must land before listener INSERTs bind `?10` (column
   must exist). `CURRENT_SCHEMA_VERSION = 28` — confirm no parallel feature claims 28.
4. cycles.js + state-canary before index-decoration (decoration calls both).

## Constraint Map (C-01..C-13 → component)

- C-01 frozen wire / C-16 no deny_unknown_fields → wire-cycle-stamp
- C-04 fail-open / C-06 sync-path budget → cycles, index-decoration, state-canary
- C-09 registry 4-touchpoint fence / C-10 minimal-diff flips → feature-source, listener-stamp-read
- C-07 pragma-guarded migration → topic-source-migration
- C-11 no raw-cwd hashing → cycles (path via config.resolve), index-decoration
- C-12 pinned CLI → state-canary (drift detector)
