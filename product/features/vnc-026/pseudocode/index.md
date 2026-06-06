# index.js — Entry / Dispatch

## Purpose
Per-spawn entry point: `node /abs/path/lib/hook-client/index.js <EVENT>`. Reads stdin via
fd 0, parses defensively, orchestrates the pipeline mirroring `hook.rs::run()`
step-for-step (with the OVERVIEW-documented deviations), guarantees exit 0 and zero
stdout on every failure path (C-05).

## Constants
```
STDIN_CAP = 1_048_576   // bytes, matches hook.rs read_stdin take(1 MiB)
```

## Functions

### main() — top-level, invoked immediately
```
async function main():
  try:
    rawEvent = process.argv[2] || ""
    raw      = readStdin()                          // never throws
    input    = parseHookInput(raw)                  // never throws
    [canonical, providerStr] = normalize.normalizeEventName(rawEvent)
    input.provider = providerStr                    // overwrite, hook.rs run() step 2b
    effectiveEvent = (canonical === "__unknown__") ? rawEvent : canonical

    cwd    = resolveCwd(input)
    config = configMod.resolve(cwd)                 // also yields stateDir/projectHash
    if not config.ok:
      // ADR-006 step 3 / partial pair: breadcrumb + stderr, exit 0, NO network
      stderrLine(config.reason === "partial_env" ? "auth" : "config",
                 describe(config.reason))
      state.writeBreadcrumb(config.stateDir, { failureClass:
        config.reason === "partial_env" ? "auth" : "connect", attempted: false })
        // see state.md: config-miss breadcrumb records failure WITHOUT counting a send
      return                                        // exit 0

    request = buildRequest(effectiveEvent, input)   // pure (build-request.md)

    // SubagentStart fallback — hook.rs run() step 5b
    if effectiveEvent === "SubagentStart" and request.type === "RecordEvent":
      role  = (input.extra is object) ? asStr(input.extra.agent_type) : null
      query = (input.transcript_path is non-empty string)
                ? transcript.extractTranscriptBlock(input.transcript_path) : null
      if query !== null:
        request = { type: "ContextSearch", query, session_id: input.session_id,
                    role, task: null, feature: null, k: null, max_tokens: null,
                    source: "SubagentStart" }       // source PRESENT (omit-when-null rule)

    reqSource = (request.type === "ContextSearch") ? (request.source ?? null) : null

    isFnf = request.type in {SessionRegister, SessionClose, RecordEvent, RecordEvents}
    if isFnf: await runFireAndForget(request, input, config)
    else:     await runSync(request, reqSource, config)
  catch (e):
    // last-resort guard: NEVER stdout, NEVER nonzero exit
    try: process.stderr.write("unimatrix: internal: " + String(e && e.message || e) + "\n")
    catch: /* swallow */
  // never call process.exit(); let the loop drain; process.exitCode stays 0
```

### readStdin() -> string
```
function readStdin():
  try:
    buf = fs.readFileSync(0)               // fd 0 — NEVER '/dev/stdin' (FR-01, R-14)
  catch: return ""                          // EOF/EAGAIN on console stdin (Windows) → ""
  if buf.length > STDIN_CAP: buf = buf.subarray(0, STDIN_CAP)
  return buf.toString("utf8")
  // Cap parity note: a JSON doc truncated at 1 MiB fails parse in BOTH clients →
  // empty HookInput in both. Corpus cases "stdin exactly 1 MiB" / "1 MiB + 1" pin this.
```

### parseHookInput(raw) -> HookInput
Port of `hook.rs::parse_hook_input` + serde semantics of `wire.rs::HookInput`.
Critical parity rule: Rust serde fails the WHOLE parse if any named field has a wrong
type; the result is then the all-empty HookInput with `extra = null`.
```
NAMED = ["hook_event_name","session_id","cwd","transcript_path","prompt",
         "provider","mcp_context"]
EMPTY = { hook_event_name:"", session_id:null, cwd:null, transcript_path:null,
          prompt:null, provider:null, mcp_context:null, extra:null }

function parseHookInput(raw):
  try: obj = JSON.parse(raw)
  catch: if raw !== "": stderrLine("parse", "stdin parse error"); return clone(EMPTY)
  if obj is not a plain object (null/array/scalar): stderrLine("parse",...); return clone(EMPTY)

  // type-check named fields exactly as serde would:
  //   hook_event_name: must be string if present
  //   session_id/cwd/transcript_path/prompt/provider: string or null if present
  //   mcp_context: any JSON value (Option<Value>)
  if any named-field type violation: stderrLine("parse", ...); return clone(EMPTY)

  out = { hook_event_name: obj.hook_event_name ?? "",
          session_id: obj.session_id ?? null, cwd: obj.cwd ?? null,
          transcript_path: obj.transcript_path ?? null, prompt: obj.prompt ?? null,
          provider: obj.provider ?? null, mcp_context: obj.mcp_context ?? null,
          extra: {} }
  for key of Object.keys(obj):              // insertion order — preserves unknown fields
    if key not in NAMED: out.extra[key] = obj[key]
  return out
  // Unknown-stdin-field parity (ass-071 carry-in / wire.rs:71-72 flatten):
  // unknown keys survive verbatim in `extra` and flow into RecordEvent payloads
  // untouched. Successful parse with zero unknown keys → extra = {} (Rust flatten
  // yields Value::Object(empty)), parse failure → extra = null (hook.rs fallback).
```

### resolveCwd(input) -> string
Port of `hook.rs::resolve_cwd` minus `--project-dir` (no flag in F3).
```
function resolveCwd(input):
  if input.cwd is non-empty string: return input.cwd
  try: return process.cwd()
  catch: return "."
```

### runSync(request, reqSource, config)
Sync path: ContextSearch | CompactPayload | Ping. NO queue replay, NO delta, NO
transcript I/O here (the SubagentStart tail read already happened pre-dispatch — the one
FR-09 exception).
```
async function runSync(request, reqSource, config):
  res = await transport.post(config, request, { sync: true })   // Accept: text/plain, 2000 ms
  transform.writeSyncOutput(reqSource, res)                      // stdout iff 200 text/plain non-empty
  state.recordSendOutcomes(config.stateDir, config.urlHost, [res], queueDepth = state.queueDepthFast(config.stateDir))
  if not res.ok: stderrLine(res.failureClass, "sync request failed")
```

### runFireAndForget(request, input, config)
```
async function runFireAndForget(request, input, config):
  queue.prune(config.stateDir)                       // 24 h age + bounds, wrapped
  await queue.replay(config, transport.post)         // ≤32 frames/256 KiB, stop-at-first-failure;
                                                     // outcome does NOT gate the carrying send (Rust parity)
  sessionId = sessionIdOf(request)                   // see below
  tasks = [ transport.post(config, request, { sync: false }) ]   // 3000 ms
  if input.transcript_path is non-empty string:
    tasks.push(delta.maybeSendDelta(input.transcript_path, sessionId,
                                    input.provider, config))     // ADR-007: concurrent
  results = await Promise.allSettled(tasks)          // independence by construction (AC-09)

  carrying = settledSendResult(results[0])           // rejected promise → synthetic {ok:false, class:"connect"}
  if not carrying.ok:
    queue.enqueue(config.stateDir, request)          // never a delta frame here (ADR-004) — wrapped, best-effort
    stderrLine(carrying.failureClass, "send failed, event queued")
  else if request.type === "SessionClose":
    state.deleteOffset(config.stateDir, sanitizeSessionKey(sessionId))  // FR-16 lifecycle, wrapped

  deltaRes = results[1] ? settledDeltaResult(results[1]) : null  // null | {skipped} | SendResult
  state.recordSendOutcomes(config.stateDir, config.urlHost,
                           attemptedOf(carrying, deltaRes), state.queueDepthFast(config.stateDir))
```

### sessionIdOf(request) -> string
```
switch request.type:
  SessionRegister | SessionClose -> request.session_id
  RecordEvent                    -> request.session_id        // flattened ImplantEvent
  RecordEvents                   -> request.events[0].session_id
// always present: build-request applies the ppid fallback before constructing frames
```

### stderrLine(class, message)
```
process.stderr.write("unimatrix: " + class + ": " + message + "\n")   // ADR-005 format; wrapped in try
// MUST never include token, URL (host ok), payload or transcript content (R-16)
```

## Error Handling
- Every path ends with implicit exit 0; `process.exit()` is never called with nonzero.
- All state/queue/breadcrumb calls are internally wrapped (their modules never throw).
- The only writers to stdout in the entire client are the two template emissions in
  transform.js — no `console.log` anywhere (integration risk: stdout contract).

## Key Test Scenarios
1. Malformed/empty/missing stdin → empty HookInput, exit 0, no stdout (corpus cases).
2. Wrong-typed named field (`"session_id": 123`) → whole input falls back to EMPTY
   (serde parity), unknown-extra-key case round-trips through RecordEvent payload.
3. Dispatch table: each of the 13 canonical events lands on the hook.rs:244-251 side;
   short (<5-word) UserPromptSubmit becomes RecordEvent → FNF path runs delta machinery.
4. SubagentStart with usable tail → ContextSearch{source:"SubagentStart"}; missing/empty
   transcript → stays RecordEvent (FNF).
5. No config: no network attempt (transport spy), breadcrumb written, exit 0.
6. fd-0 reads piped/empty/>1 MiB on Linux+macOS+Windows runners (R-14).
7. Carrying-event rejection + delta rejection both settled — exit 0, no stdout.
