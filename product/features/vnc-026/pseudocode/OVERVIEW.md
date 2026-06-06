# vnc-026 Pseudocode Overview — TS HTTP Hook Client (F3)

Stage 3a artifact. Per-component pseudocode lives beside this file; this overview defines
shared types, the wire-serialization rules every component must obey, data flow, and build
order. Oracles: `crates/unimatrix-server/src/uds/hook.rs` + `uds/transcript_block.rs`
(read-only). All ADR references: `product/features/vnc-026/architecture/ADR-00*.md`.

## Components

| File | Component | Depends on |
|---|---|---|
| `lib/hook-client/index.js` | entry/dispatch ([index.md](index.md)) | all below |
| `lib/hook-client/config.js` | config + root + hash ([config.md](config.md)) | — |
| `lib/hook-client/normalize.js` | event canonicalization ([normalize.md](normalize.md)) | — |
| `lib/hook-client/build-request.js` | HookRequest parity port ([build-request.md](build-request.md)) | normalize (contract) |
| `lib/hook-client/transcript.js` | JSONL tail-parse ([transcript.md](transcript.md)) | — |
| `lib/hook-client/transport-http.js` | POST /observe ([transport-http.md](transport-http.md)) | — |
| `lib/hook-client/transform.js` | host-envelope stdout ([transform.md](transform.md)) | — |
| `lib/hook-client/delta.js` | offset tracking + delta POST ([delta.md](delta.md)) | state, transport-http |
| `lib/hook-client/queue.js` | disk event queue ([queue.md](queue.md)) | state |
| `lib/hook-client/state.js` | state dir, atomic writes, breadcrumb ([state.md](state.md)) | — |
| `lib/init.js` + `lib/merge-settings.js` | init --remote ([init-remote.md](init-remote.md)) | transport-http (Ping) |
| Rust dev-test + suites | parity corpus ([parity-corpus.md](parity-corpus.md)) | hook.rs (oracle) |

Plain CommonJS, `"use strict"`, Node built-ins only (`fs`, `path`, `http`, `https`,
`crypto`, `os`, `process`), Node ≥18. No file exceeds 500 lines (largest is
build-request.js; if it approaches the limit, split the PostToolUse/cycle helpers into
`build-request-tools.js` — pseudocode already isolates them as standalone functions).

## Shared Types (JS shapes)

```js
// HookInput — port of wire.rs HookInput (defensive parse, see index.md parseHookInput)
{ hook_event_name: string,        // default ""
  session_id:    string|null,
  cwd:           string|null,
  transcript_path: string|null,
  prompt:        string|null,
  provider:      string|null,     // set by index.js after normalize, never from stdin… see note below
  mcp_context:   object|array|value|null,
  extra:         object|null }    // ALL remaining stdin keys, insertion order; null on parse failure,
                                  // {} on successful parse with no unknown keys (Rust flatten parity)

// ResolvedConfig (config.js)
{ ok: true, url, token, timeouts: {connectMs, syncMs, fnfMs}, source: "env"|"file",
  projectRoot, projectHash, stateDir }
| { ok: false, reason: "missing"|"partial_env"|..., projectRoot, projectHash, stateDir }

// SendResult (transport-http.js)
{ ok: boolean, status: number|0, contentType: string|null, body: Buffer|null,
  failureClass: null|"auth"|"connect"|"timeout"|"http_4xx"|"http_5xx" }
```

Note on `provider`: stdin MAY carry a `provider` key (HookInput names it). Parity rule:
parse it like Rust does (named field), then index.js OVERWRITES it from
`normalizeEventName` exactly as `hook.rs::run()` step 2b does (inference path only — F3
has no `--provider` flag).

## Wire Serialization Rules (binding on build-request.js and delta.js)

Mirrors wire.rs serde attributes; fixtures in `crates/unimatrix-engine/bindings/fixtures/`
are the authority (AC-14). Requests are plain objects + `JSON.stringify` — byte order is
NOT contractual (ADR-002); only stdout envelopes use literal templates.

| Frame | Optional-field encoding |
|---|---|
| `Ping` | `{"type":"Ping"}` |
| `SessionRegister` | `agent_role`, `feature`: **null** when absent |
| `SessionClose` | `outcome`: string or null; `duration_secs`: number |
| `RecordEvent` | ImplantEvent **flattened** into the top-level object next to `"type"` |
| `RecordEvents` | `events`: array of ImplantEvent objects |
| `ContextSearch` | `session_id/role/task/feature/k/max_tokens`: **null** when absent; `source`: **OMIT** key when absent |
| `CompactPayload` | `role/feature/token_limit`: **null**; `transcript_excerpt`: **OMIT** when absent |
| ImplantEvent | `topic_signal`, `provider`: **OMIT** key when null (serde `skip_serializing_if`) |

Helper shared by build-request.js and delta.js:

```js
function implantEvent(event_type, session_id, payload, topic_signal, provider) {
  const e = { event_type, session_id, timestamp: nowSecs(), payload };
  if (topic_signal !== null && topic_signal !== undefined) e.topic_signal = topic_signal;
  if (provider !== null && provider !== undefined) e.provider = provider;
  return e;
}
// RecordEvent frame = Object.assign({ type: "RecordEvent" }, implantEvent(...))
function nowSecs() { return Math.floor(Date.now() / 1000); }
```

## Pipeline (index.js orchestration — mirrors hook.rs::run() with documented deltas)

```
argv[2] event → readStdin (fd 0, 1 MiB cap) → parseHookInput (defensive)
→ normalizeEventName → input.provider = inferred; effectiveEvent
→ resolveCwd (stdin.cwd > process.cwd(); no --project-dir in F3)
→ config.resolve(cwd)  — miss → breadcrumb("config") + stderr + exit 0, NO network
→ buildRequest(effectiveEvent, input)              [pure]
→ SubagentStart fallback (RecordEvent → ContextSearch via transcript tail)
→ classify ON REQUEST TYPE (hook.rs:244-251):
    FNF  = SessionRegister | SessionClose | RecordEvent | RecordEvents
    sync = ContextSearch | CompactPayload | Ping
→ sync: POST (Accept: text/plain, 2000 ms) → transform stdout; no queue, no delta,
        no transcript I/O except the SubagentStart tail read already done above
→ FNF : queue.pruneAndReplay (≤32 frames/256 KiB) →
        Promise.allSettled([ postCarrying, delta.maybeSend ]) →
        carrying failure → queue.enqueue; delta failure → offset non-advance, never queued
→ breadcrumb update on every spawn that attempted a send → exit 0 always
```

Documented deviations from hook.rs::run():
1. **PreCompact does NOT read the transcript client-side** (hook.rs step 5d / D-5
   prepend). F2 restores server-side; client prints the `text/plain` body verbatim.
2. **Remote Ping event prints nothing**: server Pong is JSON; the sync path drops
   non-`text/plain` 200 bodies (R-15). Rust prints Pong JSON locally — accepted, Ping is
   not in the sync trio's stdout contract.
3. Transport is HTTP, queue format is ADR-003 (not event_queue.rs); replay outcome does
   not gate the carrying send (Rust parity: replay is best-effort, then send anyway).

## Data Flow Between Components

- `index → config`: resolved cwd string in; ResolvedConfig out. The SAME
  `projectRoot` string feeds the state-dir hash (ADR-006 split-brain guard).
- `index → build-request`: `(effectiveEvent, input)` in; HookRequest object out. Pure.
- `index → transcript`: `transcript_path` in; `string|null` block out (SubagentStart only).
- `index/delta/queue/init → transport-http`: `(config, frameObject, opts)` in; SendResult out.
- `index → transform`: `(reqSource, SendResult)` in; bytes on stdout (or nothing) out.
- `index → delta`: `(input, sessionId, provider, config, state)` in; delta SendResult-or-skip out.
- `queue/delta → state`: sanitized session keys, atomic writes, breadcrumb updates.
- Breadcrumb aggregation rule (state.md): any attempted send failing → failure recorded
  (carrying-event class wins over delta class); all attempted sends 2xx → success.

## Sequencing Constraints (build order for Stage 3b)

1. **parity-corpus generator FIRST** (FR-22: goldens exist before client modules) —
   needs only hook.rs.
2. `state.js`, `normalize.js`, `transcript.js`, `transform.js` (leaf modules).
3. `config.js`, `queue.js`, `transport-http.js`, `build-request.js`.
4. `delta.js` (needs state + transport), then `index.js` (needs everything).
5. `merge-settings.js` generalization + `init.js` remote branch (needs transport for Ping).
6. Layer 1 / Layer 2 suites + CI wiring.

## Hard Limits (single source for all components)

stdin cap 1,048,576 B; body guard 1,048,576 B; delta soft cap 65,536 B raw
(head 49,152 + tail 12,288); MIN_QUERY_WORDS 5; MAX_GOAL_BYTES 1,024; tail window
12,000 B; MAX_PRECOMPACT_BYTES 3,000; TOOL_RESULT_SNIPPET_BYTES 300;
TOOL_KEY_PARAM_BYTES 120; queue 500 files / 5 MiB / 24 h; replay 32 frames / 256 KiB;
offsets prune 7 d; timeouts 750/2,000/3,000 ms. All byte budgets are measured with
`Buffer.byteLength(str, "utf8")` — never `String.prototype.length` (UTF-16 trap).
