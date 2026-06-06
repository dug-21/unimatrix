# state.js — State Dir, Atomic Writes, Offsets, Health Breadcrumb

## Purpose
Owns `~/.unimatrix/{hash}/hook-client/` (ADR-003): directory creation with 0700/0600
modes, temp+rename atomic writes, offset persistence + prune, session-key sanitization,
and the ADR-005 content-free health breadcrumb. Everything best-effort: no function in
this module ever throws to a caller.

## Layout
```
{stateDir}/                      // = ~/.unimatrix/{projectHash}/hook-client
  offsets/{session_key}.json     // { "offset": N, "updated": <unix secs> }
  queue/{ts}-{pid}-{seq}.json    // owned by queue.js
  health.json                    // breadcrumb
```

## Functions

### ensureStateDir(stateDir) -> boolean
```
mkdirSync(stateDir + "/offsets", {recursive:true, mode:0o700})  // and /queue on demand
chmod-by-construction: pass mode at mkdir; on Windows chmod/mode are advisory no-ops —
wrapped, MUST NOT throw (R-14 scenario 3)
stateDir === null (no HOME) → return false; callers skip persistence, sends proceed
```

### sanitizeSessionKey(sessionId) -> string  (ADR-003)
```
function sanitizeSessionKey(id):
  if /^[A-Za-z0-9_-]{1,64}$/.test(id): return id
  return crypto.createHash("sha256").update(id, "utf8").digest("hex").slice(0, 16)
// Closes path traversal via attacker-shaped session ids ("../../x", "/abs", NUL, 65+ chars).
```

### atomicWrite(filePath, jsonString) -> boolean
```
tmp = filePath + ".tmp-" + process.pid + "-" + randomHex(4)     // SAME directory (rename atomicity)
writeFileSync(tmp, jsonString, {mode:0o600}); renameSync(tmp, filePath); return true
on any error: best-effort unlink(tmp); return false
// POSIX rename is atomic; Windows renameSync overwrites — acceptable (last-writer-wins
// is the declared concurrency model, FR-11)
```

### readOffset(stateDir, key) -> number
```
try: o = JSON.parse(readFileSync(offsetPath(key)))
catch: return 0                                       // missing or corrupt → 0:
                                                      // re-ship from 0 is SAFE (F2 idempotent merge)
v = o?.offset
return (Number.isSafeInteger(v) and v >= 0) ? v : 0   // negative/non-numeric/float → 0, never throw
```

### writeOffset(stateDir, key, offset) / deleteOffset(stateDir, key)
```
writeOffset: ensureStateDir; atomicWrite(offsetPath, JSON.stringify({offset, updated: nowSecs()}))
deleteOffset: unlink wrapped (called on successful SessionClose send — FR-16 lifecycle)
pruneOffsets(stateDir): unlink offset files with updated older than 7 days
  (called opportunistically on FNF spawns after replay; readdir wrapped)
```

### Breadcrumb (ADR-005)

`health.json` shape — content-free: NO token, NO payload/transcript bytes, NO full URL:
```
{ "last_success": <unix secs>|null, "last_failure": <unix secs>|null,
  "failure_class": "auth"|"connect"|"timeout"|"http_4xx"|"http_5xx"|null,
  "consecutive_failures": N, "queue_depth": N, "url_host": "host[:port]" }
```

### recordSendOutcomes(stateDir, urlHost, sendResults[], queueDepth)
Called once per spawn that attempted ≥1 send (sync AND FNF spawns — R-10 scenario 4).
Aggregation rule (deterministic, pinned here):
```
function recordSendOutcomes(stateDir, urlHost, results, queueDepth):
  attempted = results.filter(r => r != null)            // delta {attempted:false} excluded upstream
  if attempted.length === 0: return
  prev = readBreadcrumb(stateDir)                       // corrupt/missing → zeroed default
  anyFail = attempted.some(r => !r.ok)
  next = {
    last_success: attempted.every(r => r.ok) ? nowSecs() : prev.last_success,
    last_failure: anyFail ? nowSecs() : prev.last_failure,
    failure_class: anyFail
      ? (firstFailure(carrying-first-order(attempted)).failureClass)   // carrying event's class
        // wins over the delta's when both fail (index.js passes carrying first)
      : prev.failure_class,
    consecutive_failures: anyFail ? (prev.consecutive_failures + 1) : 0,
    queue_depth: queueDepth,
    url_host: urlHost }
  atomicWrite(healthPath, JSON.stringify(next))         // best-effort, swallowed on failure
```

### writeBreadcrumb(stateDir, {failureClass, attempted:false})
Config-miss variant (index.js): records `last_failure` + class WITHOUT touching
`consecutive_failures` semantics? — No: pinned rule: config-miss DOES increment
`consecutive_failures` and sets class (`auth` for partial_env, `connect` for
missing/malformed) so a perpetually misconfigured install shows a growing counter — the
SR-10 diagnostic intent. `url_host` left at previous value or "".

## Error Handling
- Every fs op wrapped; all functions return values/false rather than throwing.
- Breadcrumb write failure (read-only dir, full disk): spawn still exits 0, no stdout,
  the send was already attempted (R-10 scenario 3).
- Sanitized keys make offset/queue filenames attacker-proof.

## Key Test Scenarios
1. Breadcrumb transition matrix (R-10): each failure class lands verbatim;
   consecutive_failures increments across spawns, resets to 0 on all-success;
   queue_depth equals actual file count; driven through the W4 outage/recovery workflow.
2. Content-free scan: across the full failure matrix, health.json never contains the
   token string, any payload fragment, transcript bytes, or a full URL (host only).
3. Atomic write: no partial JSON ever observable under concurrent spawns (reader loop);
   tmp files cleaned up.
4. Offset corruption: `{"offset":"x"}`, `-5`, `1.5`, truncated file → readOffset 0,
   no throw, session recovers via re-ship-from-0.
5. 7-day offset prune; offset file deleted on successful SessionClose; mid-session file
   pruned → re-ship from 0 (documented-safe).
6. ppid-collision keys (R-19): same `ppid-N` → shared offset file (documented Rust-parity
   behavior); traversal corpus (`../../`, absolute, NUL, 65 chars) → hashed key.
7. Windows: mode flags no-op without throwing; rename-overwrite path exercised.
8. No HOME: every function degrades to no-op, sends unaffected.
