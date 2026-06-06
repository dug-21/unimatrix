# vnc-026 Architecture: TS HTTP Hook Client + Client-Streamed Transcript Deltas (F3)

GH #679. Scope: `product/features/vnc-026/SCOPE.md` (RQ-1..RQ-8 binding). All decisions
referenced as ADR-NNN live in `product/features/vnc-026/architecture/` and Unimatrix.

## System Overview

F3 is the client edge of the OSS-cloud remote MVP. The server side is complete or in
flight: F1 (vnc-024, shipped) gives `/observe` content negotiation (`Accept: text/plain`
→ server-side `format_injection`) and the frozen ts-rs wire contract; F2 (vnc-025,
in flight) gives the per-session in-memory transcript buffer with offset-bounded
idempotent merge and server-side PreCompact restoration.

vnc-026 adds `lib/hook-client/` to the existing `@dug-21/unimatrix` npm package: a
zero-dependency CommonJS client spawned per hook event (`node /abs/path/lib/hook-client/index.js
<EVENT>`), which reproduces the Rust hook's request-building behavior byte-for-byte
(modulo volatile fields), POSTs to `{url}/observe` with Bearer auth, streams transcript
deltas on fire-and-forget events, and writes host envelopes to stdout on sync events.
`init --remote <url> --token <tok>` wires it into `.claude/settings.json`.

The client is **stateless except two artifacts** in `~/.unimatrix/{hash}/hook-client/`:
per-session `last_offset` and a minimal disk event queue (RQ-1, RQ-2; ADR-003).
Everything else is computed fresh per spawn.

```
Claude Code hook spawn
  └─ node lib/hook-client/index.js <EVENT>   (stdin: hook JSON, ≤1 MiB)
       ├─ config.js     resolve URL/token (env > settings.local.json)   [ADR-006]
       ├─ normalize.js  event canonicalization (Gemini aliases)
       ├─ build-request.js  HookRequest (parity port of hook.rs)        [ADR-001]
       ├─ transcript.js JSONL tail-parse (SubagentStart query, RQ-6)
       ├─ sync trio ──► transport-http.js POST /observe, Accept: text/plain
       │                  └─ transform.js  stdout host envelope         [ADR-002]
       └─ fire-and-forget ──► queue.js replay (bounded)                 [ADR-003]
                              transport-http.js POST #1 (carrying event)
                              delta.js  POST #2 (transcript_delta)      [ADR-004/007]
       always: exit 0; failures → health breadcrumb + stderr            [ADR-005]
```

## Component Breakdown (`packages/unimatrix/lib/hook-client/`)

| Module | Responsibility | Ported from (oracle) |
|---|---|---|
| `index.js` | Entry: argv event, stdin read (`fs.readFileSync(0)`, 1 MiB cap), defensive parse, orchestration of the run() pipeline, exit-0 guarantee (top-level try/catch) | `hook.rs::run`, `read_stdin`, `parse_hook_input`, `resolve_cwd` |
| `config.js` | URL/token resolution (ADR-006); project root walk (`detectProjectRoot` port from `lib/init.js`); project hash (SHA-256 hex, first 16 chars — `project.rs::compute_project_hash`); state-dir path | `project.rs:130-136`, `lib/init.js:26-41` |
| `normalize.js` | `mapToCanonical` / `normalizeEventName` — pure string maps incl. Gemini `BeforeTool`/`AfterTool`/`SessionEnd` and `__unknown__` sentinel | `hook.rs:50-105` |
| `build-request.js` | Full `build_request` port: session_id ppid fallback (`ppid-${process.ppid}`), MIN_QUERY_WORDS=5 gate, PostToolUse rework extraction (`is_bash_failure`, `extract_file_path`, MultiEdit → RecordEvents), PostToolUseFailure explicit arm, PreToolUse `context_cycle` interception (incl. mcp_context promotion, `validate_cycle_params`, MAX_GOAL_BYTES=1024 truncation), topic-signal extraction, SubagentStart prompt_snippet path | `hook.rs:440-951` + `extract_topic_signal`, `validate_cycle_params` |
| `transcript.js` | JSONL tail-parse for SubagentStart query derivation (RQ-6): 12,000-byte tail window, `build_exchange_pairs` (adjacent-record tool pairing, thinking-turn suppression), `format_turn`, `block_from_lines`, MAX_PRECOMPACT_BYTES=3000, `truncate_utf8` (byte-boundary-safe) | `transcript_block.rs` (entire module) |
| `transport-http.js` | Node `http`/`https` request to `{url}/observe`; `Authorization: Bearer <token>`; `Accept: text/plain` on sync, `application/json` otherwise; `Content-Type: application/json`; timeouts (connect 750 ms, sync total 2,000 ms, FNF total 3,000 ms — config-overridable); response classification (2xx/204 success, everything else failure class for the breadcrumb) | new |
| `transform.js` | Host-envelope stdout serialization only: plain text + trailing newline for UserPromptSubmit/PreCompact (non-empty 200 body), literal-template `hookSpecificOutput` envelope for SubagentStart (ADR-002); 204/empty → no stdout | `hook.rs:963-1028` |
| `delta.js` | Per-session offset tracking; `fstat` + positioned read of `[last_offset, file_len)`; UTF-8 boundary trim at span end; 64 KiB soft cap with head 48 KiB + tail 12 KiB + `…[N bytes elided]…` marker, end-anchored: declared `offset = file_len − bytes.length` (ADR-008); post-serialization 1 MiB assert; truncation/rewrite guard (`file_len < last_offset` → reset to `file_len`, ship nothing); separate second POST (ADR-007); offset advance rules (ADR-004) | new (mechanism per ass-069 Q2) |
| `queue.js` | Minimal disk queue per ADR-003 mini-spec: O_EXCL one-frame-per-file enqueue, lexicographic replay-before-send with per-spawn bounds, drop-oldest eviction, age prune | semantics modeled on `event_queue.rs`, simplified |
| `state.js` | State-dir layout, atomic file writes (temp + rename), offset persistence, health breadcrumb (ADR-005), session-id sanitization for filenames | new |

Estimated payload well under the 100 KB gate (AC-12); zero runtime dependencies —
built-ins only: `fs`, `path`, `http`, `https`, `crypto`, `os`, `process`.

## Component Interactions / Data Flow

### Pipeline (mirrors `hook.rs::run()` step-for-step)

1. Read stdin (1 MiB cap) → defensive parse (parse failure → empty HookInput, never throw).
2. Normalize event name; set `provider` on input (inference path; no `--provider` flag in F3 —
   the hook command carries only `<EVENT>`).
3. Resolve cwd (stdin `cwd` > `process.cwd()`), project root, project hash, state dir.
4. Resolve remote config (ADR-006). Missing config → breadcrumb + exit 0 (no stdout).
5. `buildRequest(effectiveEvent, input)` — pure, parity-tested (ADR-001).
6. SubagentStart fallback: if result is RecordEvent, derive query from transcript tail
   (`transcript.js`) + `agent_type` role → ContextSearch with `source: "SubagentStart"`.
7. Classify: fire-and-forget = SessionRegister | SessionClose | RecordEvent | RecordEvents;
   sync = ContextSearch | CompactPayload | Ping (identical to `hook.rs:244-251`).
8. **Sync path**: POST with `Accept: text/plain`; 200 body → `transform.js` envelope to
   stdout; 204/empty/failure → no stdout. No delta machinery runs; no queue replay
   (SR-03). The only file I/O permitted on a sync spawn is the SubagentStart 12 KB tail
   read mandated by RQ-6 (see Open Questions — AC-08 wording).
9. **Fire-and-forget path**: bounded queue replay first (ADR-003) → POST carrying event;
   on failure enqueue the frame → if stdin had non-empty `transcript_path`, run `delta.js`
   (second POST, ADR-007); offset advances only per ADR-004 rules. No stdout ever.
10. Exit 0 unconditionally.

### Delta mechanics (`delta.js`)

- Span = `[last_offset, file_len)` read with a positioned `fs.readSync` into a Buffer.
- **UTF-8 boundary rule**: back the span end off up to 3 bytes to the last complete UTF-8
  character before decoding; `last_offset` advances only by bytes actually shipped. Span
  starts are therefore always boundary-clean. This prevents lossy-decode replacement
  characters from corrupting the byte-offset accounting against F2's merge.
- **Cap (SR-04)**: cap applies to raw span bytes (64 KiB). With JSON string-escaping
  worst-case inflation (~6x for control-char-dense content) the serialized frame stays
  ≤ ~384 KiB, under the 1 MiB body guard. A post-serialization assert re-truncates in the
  (theoretically unreachable) overflow case. **Elided frames are end-anchored (ADR-008)**:
  declared `offset = file_len − bytes.length` so the frame ends exactly at `file_len`;
  `last_offset` advances to `file_len` (AC-07) = `offset + bytes.length` — the uniform
  ADR-004 rule. F2 records the elided region as a hole *behind* the shipped content at
  apply time (`holes == [(last_offset, file_len − bytes.length)]`, `high_water == file_len`);
  the next delta extends contiguously at `file_len`, and PreCompact's `contiguous_tail`
  serves the full catch-up immediately. Never declare `offset = last_offset` for an elided
  frame — that defers the hole to the next delta's arrival, placing it *in front of* the
  catch-up content and permanently starving PreCompact (`contiguous_tail` floors at the
  hole end = `file_len`).
- **Rewrite guard (A-4)**: `file_len < last_offset` → set `last_offset = file_len`, ship
  nothing this spawn.
- Frame shape: `{"type":"RecordEvent", event: { event_type: "transcript_delta", session_id,
  timestamp, payload: { offset, bytes }, topic_signal: null, provider }}` — payload matches
  `TranscriptDeltaPayload` binding exactly; no new wire surface.

### `init --remote` (extends `lib/init.js` + `lib/merge-settings.js`)

- New flags: `--remote <url> --token <tok>` (plus existing `--dry-run`).
- Steps: detect project root → write `{root}/.claude/settings.local.json` `unimatrix.remote`
  block (mode 0600; merge-preserving; warn if not gitignored) → merge hooks into
  `.claude/settings.json` with command `node /abs/path/to/lib/hook-client/index.js <EVENT>`
  (absolute path resolved from the installed package via `require.resolve`) → **skip**
  `.mcp.json` with an informative message (RQ-4) → **skip** binary resolution, DB
  pre-creation, binary validation (no binary in remote mode) → connectivity check: build a
  `Ping` HookRequest and POST it via `transport-http.js` with the new token; non-Pong/non-2xx
  → fail init with actionable message (the only loud failure point in F3, ADR-005).
- `merge-settings.js` changes:
  - `UNIMATRIX_PATTERNS` gains `/(^|\s|\/)node\s+\S*\/hook-client\/index\.js\s/` so
    remote entries are recognized as unimatrix-owned on re-run, and so re-runs replace
    old-style `unimatrix hook` entries when switching modes (SR-08; same prefix-match
    pattern as ADR #1201).
  - `mergeSettings(filePath, binaryPath, options)` is generalized to
    `mergeSettings(filePath, commandSource, options)` where `commandSource` is
    `{ commandForEvent(event), events }`; a thin back-compat wrapper preserves the current
    local-mode call site. Remote mode passes the node command builder + the full 9-event set.
  - `HOOK_EVENTS` (both files) fixed to include `PreCompact` (matcher `""`) and
    `PostToolUseFailure` (matcher `"*"`) for **both** local and remote modes (RQ-8, AC-16).
    Remote event set = local event set = 9 events: SessionStart, Stop, UserPromptSubmit,
    PreToolUse, PostToolUse, PostToolUseFailure, PreCompact, SubagentStart, SubagentStop.

### Parity suite (RQ-7, ADR-001)

- **Corpus**: committed stdin fixtures + Rust-generated golden outputs under
  `packages/unimatrix/test/fixtures/parity/`. Generator is a Rust dev-test in
  `unimatrix-server` (same drift-check pattern as the ts-rs bindings, entry #4726):
  regenerating must produce zero diff in CI. The corpus is the F6 retirement evidence.
- **Layer 1 (AC-01/AC-04/AC-05)**: pure-module tests — `buildRequest` output vs golden
  request JSON (structural equality after normalizing volatile fields: `timestamp`,
  `ppid-*` session ids); stdout envelopes vs golden bytes (byte-identical), with
  deterministic server-buffer pre-population isolated behind **one test helper** (SR-11)
  so F2-internal changes localize.
- **Layer 2 (AC-05/AC-10)**: integration — real server process, streamed deltas with
  injected drops, ≥8 interleaved sessions; content-equivalence modulo elision markers;
  per-buffer byte ownership assertion (ass-069 PoC model).
- AC-14: client-built frames round-trip against `crates/unimatrix-engine/bindings/fixtures/*.json`
  (extend the `contract.test.mjs` pattern).

## Technology Decisions

| Decision | Rationale | ADR |
|---|---|---|
| Plain CommonJS, zero runtime deps, Node ≥18 | Matches existing `lib/`; supply-chain posture; <100 KB gate (scope constraint) | — (scope-bound) |
| Parity corpus generated from the Rust hook as committed goldens with CI drift check | SR-01: the Rust implementation is the oracle; tests can't drift silently | ADR-001 |
| Stdout envelopes from literal templates, never object serialization | SR-02: removes serde_json-vs-JSON.stringify byte-order/escaping risk from the host contract | ADR-002 |
| Client state dir `~/.unimatrix/{hash}/hook-client/`; O_EXCL per-frame queue files; atomic-rename offsets; bounds + 24 h prune | RQ-2; SR-05/SR-06: lock-free single-writer-per-file, bounded growth, short secrets-adjacent retention | ADR-003 |
| transcript_delta frames are never queued; failure = don't advance offset, re-derive next spawn | SR-06: zero transcript bytes at rest; transcript file is the durable source; F2 merge idempotence makes re-ship free | ADR-004 |
| Fail-open with content-free health breadcrumb + stderr; init Ping is the only loud checkpoint | SR-10: explicit observability trade-off, F5 surfaces it | ADR-005 |
| Config: env vars > `{project_root}/.claude/settings.local.json`; single deterministic location | SR-09/RQ-3: spawn-time resolution from any cwd; token never on command line | ADR-006 |
| Delta ships as a separate second POST (RQ-5 confirmed) | AC-09 independence by construction; batching couples failure domains for ~nothing | ADR-007 |
| HTTP timeouts: connect 750 ms / sync 2,000 ms / FNF 3,000 ms, overridable | WAN reality vs the 500 ms LAN-era budget; sync timeout → silent no-injection, not a hang | ADR-005 (consequences) |

## Integration Points

- **F1 wire contract (frozen)**: types consumed from `crates/unimatrix-engine/bindings/*.ts`
  + fixtures; never hand-mirrored. `transcript_delta` rides `ImplantEvent.payload`
  (vnc-024 ADR-004, #4720).
- **F1 content negotiation**: `Accept: text/plain` on sync → server `format_injection`
  with production `MAX_INJECTION_BYTES=1400`; `200 text/plain` | `204` empty/over-budget;
  Pong/Error stay JSON (vnc-024 ADR-003, #4714). Client treats non-text/plain 200 on the
  sync path as no-output (defensive).
- **F2 buffer (vnc-025, #670)**: offset-bounded idempotent merge; accumulated cap 4 MiB;
  server-side PreCompact block. Client depends only on the **wire behavior** (idempotent
  accept of `{offset, bytes}`), not buffer internals (SR-11). Delivery gated on #670 merge.
- **Server session namespacing**: client sends raw `session_id`; server mints
  `http-{session_id}` (`observe.rs`). Client never prefixes.
- **npm package**: `files` already includes `lib/` — `lib/hook-client/` ships automatically.
  `init.js` (root detection reused; remote branch added), `merge-settings.js` (patterns +
  events + command-source generalization), `test/` (cumulative — extend existing
  `node:test` suites).
- **Rust hook (`uds/hook.rs`, `uds/transcript_block.rs`)**: read-only oracle. No Rust
  changes in F3 except the additive parity-corpus generator test.

## Integration Surface

| Integration Point | Type/Signature | Source |
|---|---|---|
| Hook endpoint | `POST {url}/observe`, body = `HookRequest` JSON, headers `Authorization: Bearer <token>`, `Content-Type: application/json`, sync adds `Accept: text/plain` | `http/router.rs:202-210`, `observe.rs:25-93` |
| Sync responses | `200 text/plain` (formatted text) \| `204` (empty/over-budget/Ack) \| JSON for Pong/Error | vnc-024 ADR-003 (#4714) |
| `HookRequest` | discriminated union on `"type"`: `Ping` \| `SessionRegister{session_id,cwd,agent_role,feature}` \| `SessionClose{session_id,outcome,duration_secs}` \| `RecordEvent{event}` \| `RecordEvents{events}` \| `ContextSearch{query,session_id,source?,role,task,feature,k,max_tokens}` \| `CompactPayload{session_id,injected_entry_ids,role,feature,token_limit,transcript_excerpt?}` | `bindings/HookRequest.ts` |
| `ImplantEvent` | `{event_type, session_id, timestamp:u64, payload:Json, topic_signal?, provider?}` | `bindings/ImplantEvent.ts` |
| Delta payload | `event_type: "transcript_delta"`, `payload: {offset: u64, bytes: string}` | `bindings/TranscriptDeltaPayload.ts`, `wire.rs:46` |
| SubagentStart envelope (stdout) | `{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":<JSON string>}}` + `\n`, compact (no spaces), key order as shown (workspace serde_json has `preserve_order`) | `hook.rs:994-1006`; golden corpus is authority |
| Plain-text envelope (stdout) | response body verbatim + `\n` iff body non-empty | `hook.rs:963-985` |
| Project hash | first 16 hex chars of SHA-256 of project-root path string | `project.rs:130-136` |
| State dir | `~/.unimatrix/{hash}/hook-client/{offsets/,queue/,health.json}` | ADR-003 |
| Config file | `{project_root}/.claude/settings.local.json` → `{"unimatrix":{"remote":{"url":"…","token":"…"}}}`; env override `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN` | ADR-006 |
| Hook command | `node /abs/path/lib/hook-client/index.js <EVENT>` | ass-068 Q1; AC-11 |
| Ownership pattern (new) | `/(^|\s|\/)node\s+\S*\/hook-client\/index\.js\s/` added to `UNIMATRIX_PATTERNS` | `merge-settings.js:11-16` |
| Sync/FNF split | FNF = SessionRegister, SessionClose, RecordEvent, RecordEvents; sync = ContextSearch, CompactPayload, Ping | `hook.rs:244-251` |
| Hard limits | stdin 1 MiB; body guard 1 MiB (413); delta soft cap 64 KiB (48 KiB head + 12 KiB tail); MIN_QUERY_WORDS 5; MAX_GOAL_BYTES 1024; tail window 12,000 B; MAX_PRECOMPACT_BYTES 3000 | `hook.rs`, `transcript_block.rs`, `wire.rs:16` |
| ppid fallback | `session_id = "ppid-" + process.ppid` when stdin omits session_id | `hook.rs:449-453`; `process.ppid` (Node) |

## Risk Disposition

| Risk | Disposition |
|---|---|
| SR-01 | ADR-001 — Rust-oracle golden corpus, adversarial inventory enumerated, CI drift check |
| SR-02 | ADR-002 — literal templates; goldens are byte authority |
| SR-03 | ADR-003 — replay bounded (32 frames / 256 KiB per spawn), FNF spawns only |
| SR-04 | delta.js cap-on-raw + inflation bound + post-serialization assert (above) |
| SR-05 | ADR-003 — full mini-spec: bounds, eviction, lock-free O_EXCL, prune |
| SR-06 | ADR-004 (deltas never on disk) + ADR-003 (0600/0700 perms, 24 h prune for what is queued) |
| SR-07 | RQ-8 kept to list + matchers + AC-16 regression test; no other local-path change |
| SR-08 | new ownership pattern + re-run tests over old-style configs (AC-11) |
| SR-09 | ADR-006 — deterministic root-anchored resolution; subdirectory-cwd test required |
| SR-10 | ADR-005 — explicit trade-off: breadcrumb + queue depth observable; surfacing deferred to F5 |
| SR-11 | F2 consumed at wire level only; buffer pre-population behind one test helper |
| A-4 | rewrite guard: `file_len < last_offset` → reset to `file_len`, ship nothing |

## Open Questions (for spec / leader)

1. **AC-08 wording vs RQ-6**: AC-08 says "the sync request path performs no transcript
   file I/O", but RQ-6 mandates the SubagentStart 12 KB tail read (sync trio) for query
   derivation — exactly as the Rust hook does. Spec should narrow AC-08 to "no
   delta-related transcript I/O (no stat/span-read/offset persistence) on sync spawns."
2. **AC-15 wording vs ADR-004**: AC-15 says fire-and-forget frames are enqueued on send
   failure. ADR-004 carves out `transcript_delta` frames (never queued; offset re-drive
   instead — strictly better delivery and zero transcript bytes at rest). Spec should
   reflect the carve-out.
3. **Env var names** `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN` (ADR-006) should be
   confirmed against F5 (#681) naming before delivery to avoid a rename.
4. **Timeout defaults** (750/2,000/3,000 ms) are architect-chosen for WAN reality; spec
   may tune, but the structure (separate connect/sync/FNF deadlines, config-overridable,
   silent on expiry) is the decision.
