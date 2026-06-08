# cycles.js — Cycle Tracker Module (NEW)

**Source**: `packages/unimatrix/lib/hook-client/cycles.js` (NEW). **ADR**: ADR-001.
**Constraints**: C-04 fail-open (never throws), C-11 no raw-cwd hashing (path
derives from caller-supplied `stateDir` = `config.resolve(cwd).stateDir`).

## Purpose

Per-session cycle tracker file lifecycle. The tracker is the stamp's source of
truth and survives all three server-state-loss events (per-turn drain,
resume/compact re-register, server restart) because it is a disk file keyed by
the root session_id. Reuses `state.js` machinery (`atomicWrite`,
`sanitizeSessionKey`, `ensureStateDir`, `nowSecs`) — no new failure modes.

## Tracker File

```
path:  {stateDir}/cycles/{sanitizeSessionKey(sid)}.json     // beside offsets/
shape: { "topic": string, "phase": string|null, "declared_at": secs, "updated": secs }
```

Sanitization happens INSIDE this module (pattern #4772 — never pre-sanitize at
call sites). Raw `session_id` is passed in.

## Module Constants / Helpers (mirror state.js)

```
PRUNE_SECS = 7 * 24 * 60 * 60                 // same 7-day policy as OFFSET_PRUNE_SECS

cyclesDir(stateDir)  -> path.join(stateDir, "cycles")
cyclePath(stateDir, sessionId)
  -> path.join(stateDir, "cycles", state.sanitizeSessionKey(sessionId) + ".json")
     // reuse state.sanitizeSessionKey — do NOT re-implement
```

`ensureStateDir` in state.js only mkdir's `offsets/`. cycles.js needs its own
`cycles/` dir created on write. Add a local `ensureCyclesDir(stateDir)` that
mkdir's `{stateDir}/cycles` with mode 0700, recursive, returns bool; OR (smaller
diff, preferred) extend `state.ensureStateDir` to also mkdir `cycles/`. **Open
choice — see Open Questions.** Pseudocode below assumes a local `ensureCyclesDir`.

## Functions (all never-throw — F3 C-05)

### `readCycle(stateDir, sessionId) -> {topic, phase} | null`

```
if not state.usable-equivalent(stateDir): return null      // typeof string && length>0
try:
    raw := fs.readFileSync(cyclePath(stateDir, sessionId), "utf8")
    parsed := JSON.parse(raw)
catch: return null                                          // missing / unreadable / bad JSON
if parsed is not a plain object: return null
topic := parsed.topic
if typeof topic !== "string" or topic === "": return null   // mistyped/empty → treat as no tracker
phase := (typeof parsed.phase === "string") ? parsed.phase : null
return { topic, phase }
```
Note: returns ONLY `{topic, phase}` (the stamp surface). `declared_at`/`updated`
are file-internal. Corrupt/mistyped → `null` (event sent unstamped). R-03/R-06 (security).

### `writeCycle(stateDir, sessionId, topic, phase) -> bool`

```
if not usable(stateDir): return false
if not ensureCyclesDir(stateDir): return false
now := state-style nowSecs()
declared_at := preserve existing on overwrite? NO — full-file create-or-overwrite,
   last-writer-wins (ADR-001 scenario 17). declared_at := now on every write.
   (Rationale: a re-declaration is a fresh declaration; declared_at is informational,
    not load-bearing for the stamp. Keeps writeCycle a pure create-or-overwrite —
    no read-modify-write, smaller + atomic. See Open Questions OQ-cycles-1.)
body := JSON.stringify({ topic, phase: (phase ?? null), declared_at: now, updated: now })
return state.atomicWrite(cyclePath(stateDir, sessionId), body)    // temp+rename
```
Caller passes `topic = payload.feature_cycle`, `phase = payload.next_phase ?? null`
(index-decoration.md). Topic is stored verbatim — server does not normalize it
(R-10: validation at frame-construction time is the gate, not here).

### `updatePhase(stateDir, sessionId, phase) -> bool`

```
if not usable(stateDir): return false
existing := readFile+parse of cyclePath        // RMW; needs full object incl. topic/declared_at
if read/parse fails OR not a plain object OR typeof existing.topic !== "string":
    return false        // MISSING FILE → no-op false; NEVER recreate (R-22, ADR-001)
                        // phase-end without a prior start is a protocol violation; degrade
body := JSON.stringify({
    topic: existing.topic,
    phase: (phase ?? null),
    declared_at: (Number.isSafeInteger(existing.declared_at) ? existing.declared_at : nowSecs()),
    updated: nowSecs()
})
return state.atomicWrite(cyclePath(stateDir, sessionId), body)
```

### `deleteCycle(stateDir, sessionId) -> bool`

```
if not usable(stateDir): return false
try: fs.unlinkSync(cyclePath(stateDir, sessionId)); return true
catch: return false        // already gone / unwritable → false, never throw
```

### `pruneCycles(stateDir)  -> void`

Mirror `state.pruneOffsets` exactly, over `cycles/` with `PRUNE_SECS`.
```
if not usable(stateDir): return
try: names := fs.readdirSync(cyclesDir(stateDir)); catch: return
cutoff := nowSecs() - PRUNE_SECS
for name in names:
    if not name.endsWith(".json"): continue            // skip .tmp-* remnants
    fp := path.join(cyclesDir, name)
    updated := read parsed.updated (safe int) else mtime fallback (statSync) else continue
    if updated < cutoff: try fs.unlinkSync(fp) catch (best-effort)
```
Called where `queue.prune` / `state.pruneOffsets` already run on the FNF path
(`index.js:267-268`) — piggyback, best-effort. See index-decoration.md.

## Removed by ADR-006 rev2

`anyOtherCycleFile` is **NOT implemented** — the concurrent-file canary rule was
retired. The canary reads only the carried root tracker via `readCycle`
(state-canary.md). Do NOT add a directory scan.

## Initialization Sequence

No constructor. Module exports pure functions. `cycles/` dir is created lazily on
first `writeCycle` via `ensureCyclesDir`. State dir root (`~/.unimatrix/{hash}/
hook-client/`) is created by `state.ensureStateDir` on the offsets path already.

## Data Flow

- IN: `stateDir` (string), `sessionId` (raw, unsanitized), `topic`/`phase` (from
  validated cycle payload).
- OUT: bool (write/update/delete/ensure), `{topic, phase}|null` (read), void (prune).
- Sanitization: internal. Atomicity: via `state.atomicWrite` (temp+rename).

## Error Handling

Every fs call wrapped; every degrade path returns the never-throw sentinel
(`null`/`false`). No stdout. No throw. No secrets in any output (this module
writes no stderr). A disk-full `writeCycle` on cycle_start → `false`; the event
is still sent (unstamped this spawn; next spawn re-reads). Fail-open by contract.

## Key Test Scenarios

- writeCycle on cycle_start produces the file atomically; readCycle returns
  `{topic, phase}`; invalid params never reach writeCycle (gated upstream, R-10).
- updatePhase on existing file bumps phase + `updated`, preserves topic; on
  MISSING file → no-op `false`, no recreate (R-22).
- deleteCycle removes the file; second delete → `false`, no throw.
- readCycle on missing / corrupt-JSON / mistyped-topic / empty-topic → `null` (R-03).
- pruneCycles removes only files with `updated` older than 7 days; uses mtime
  fallback when JSON unreadable; ignores non-`.json`.
- Path traversal: adversarial session_id (`../../`, absolute, NUL, 65+ chars,
  Unicode) → `sanitizeSessionKey` neutralizes; all ops stay within `cycles/`.
- Failure injection (EACCES/ENOENT/EROFS) on each fs touchpoint → never-throw
  degrade, exit 0, no stdout, no secret/path in stderr (R-03, NFR-03).
- C-11: every path derives from the passed `stateDir`; no `process.cwd()` hash here.

## Open Questions / Gaps

- **OQ-cycles-1 (declared_at on overwrite)**: writeCycle is specified as
  create-or-overwrite with `declared_at := now` each time (no RMW). If delivery
  wants `declared_at` to mean "first declaration of this session_id" (survives
  re-declaration), writeCycle would need a read-existing step. ADR-001 calls it
  "create-or-overwrite (last declaration wins)" and never reads `declared_at`
  back, so reset-on-overwrite is consistent with the ADR. Flagged, not blocking.
- **ensureCyclesDir vs extend state.ensureStateDir**: a directory-creation
  touchpoint not named in the ADR. Local helper keeps state.js untouched;
  delivery may prefer extending `ensureStateDir` for one fewer mkdir site. Either
  satisfies the contract — pick at implementation, keep it never-throw.
