# queue.js — Minimal Disk Event Queue (ADR-003 Mini-Spec)

## Purpose
Enqueue-on-failure for NON-DELTA fire-and-forget frames; bounded lexicographic
replay-before-send; drop-oldest eviction; 24 h age prune; poison-pill immunity. Lock-free
by construction (one O_EXCL file per frame). Runs ONLY on FNF spawns — never the sync
trio (SR-03). Every operation is wrapped: queue failures never affect exit code or stdout.

## Constants
```
MAX_FILES = 500;  MAX_TOTAL_BYTES = 5 * 1024 * 1024;  MAX_AGE_MS = 24 * 3600 * 1000
REPLAY_MAX_FRAMES = 32;  REPLAY_MAX_BYTES = 262_144            // per spawn
```
Queue dir: `{stateDir}/queue/` — distinct from the Rust hook's `event-queue/` (no shared
files, no cross-format reads; integration assertion).

## Functions

### enqueue(stateDir, frame) -> void  (best-effort)
```
function enqueue(stateDir, frame):
  guard: if stateDir is null: return
  guard: if frame.type === "RecordEvent" and frame.event_type === "transcript_delta":
    return                                   // defense-in-depth; ADR-004 — index.js never
                                             // routes deltas here, this guard makes it structural
  try:
    dir = ensureQueueDir(stateDir)           // 0700, via state.ensureStateDir
    data = JSON.stringify(frame)
    enforceBounds(dir, Buffer.byteLength(data))   // prune age + drop-oldest BEFORE write
    ts = Date.now(); seq = 0
    loop (max 1000):
      name = pad13(ts) + "-" + process.pid + "-" + pad4(seq) + ".json"
      // zero-padded ts (13) and seq (4): lexicographic order == age order even across
      // a future epoch-digit rollover and double-digit seqs
      try: fs.writeFileSync(path.join(dir,name), data, { flag:"wx", mode:0o600 }); break
      catch EEXIST: seq += 1                 // same-ms same-pid collision bumps seq
  catch (anything): breadcrumb-note via return value only; swallow (AC-15)
```

### enforceBounds(dir, incomingBytes)
```
function enforceBounds(dir, incomingBytes):
  entries = listQueueFiles(dir)              // names sorted lexicographically ascending (oldest first)
  now = Date.now()
  // 1. age prune: parse leading ts from filename; older than 24 h → unlink (deleted, NOT replayed)
  // 2. size/count: while (count+1 > MAX_FILES) or (totalBytes + incomingBytes > MAX_TOTAL_BYTES):
  //      unlink oldest remaining (drop-oldest)
  all unlinks individually wrapped
```

### replay(config, post) -> Promise<{sent, stoppedOnFailure}>
Replay-before-send (FR-13/FR-15). Called by index.js BEFORE the carrying POST on FNF
spawns. Outcome does not gate the carrying send (Rust run() parity — best-effort).
```
async function replay(config, post):
  guard stateDir null → {sent:0}
  files = listQueueFiles(queueDir).sort()    // oldest first
  sentBytes = 0; sentFrames = 0
  for name of files:
    if sentFrames >= REPLAY_MAX_FRAMES or sentBytes >= REPLAY_MAX_BYTES: break  // leave remainder
    try: raw = fs.readFileSync(p) catch: continue          // vanished (concurrent spawn) → skip
    try: frame = JSON.parse(raw.toString("utf8"))
    catch: unlinkWrapped(p); continue                      // poison pill: delete, keep going
    res = await post(config, frame, { sync:false })
    if not res.ok: return {sent:sentFrames, stoppedOnFailure:true}   // stop at FIRST failure,
                                                                     // file NOT deleted
    unlinkWrapped(p)                                       // delete only after 2xx
    sentFrames += 1; sentBytes += raw.length
  return {sent:sentFrames, stoppedOnFailure:false}
// Concurrent recovering spawns may double-send a frame (read…2xx…unlink race) — accepted;
// server tolerates duplicate observations (R-08).
```

### prune(stateDir) / queueDepth(stateDir) -> number
```
prune: age-prune pass only (called each FNF spawn before replay; cheap readdir ≤500)
queueDepth: count of *.json in queue dir (breadcrumb's queue_depth); errors → 0
```

## Error Handling
- Module never throws: full-disk, unwritable dir, readdir failure → swallowed; the send
  path proceeds regardless (Failure Modes table: "send still attempted").
- A corrupt frame is deleted and replay continues (FR-15 poison-pill immunity).
- No frame content ever logged (queued payloads are secrets-adjacent, R-16).

## Key Test Scenarios (AC-15 + R-08)
1. Lifecycle: send failure → file appears (0600, one frame) → server recovers → next FNF
   spawn replays in order BEFORE its own frame → queue drained.
2. Bounds: 501st enqueue and >5 MiB enqueue trigger drop-oldest; >24 h files pruned
   unreplayed; same-ms same-pid collisions bump seq.
3. Replay budget: 33 queued frames → exactly 32 sent, remainder left; 256 KiB cap
   enforced; stop-at-first-failure leaves the failed file + remainder.
4. Poison pill: unparseable file deleted, replay continues with the next.
5. Sync trio spawns: fs-spy proves zero `queue/` I/O (SR-03 / R-13).
6. Delta guard: no file whose content has `event_type:"transcript_delta"` EVER appears
   in `queue/` across the full failure matrix (the load-bearing at-rest guarantee).
7. Queue failures (read-only dir, full disk): exit 0, no stdout, send still attempted.
8. Dir distinctness: `hook-client/queue/` never reads/writes `event-queue/`.
