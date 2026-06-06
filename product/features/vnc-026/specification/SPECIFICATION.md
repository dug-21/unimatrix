# SPECIFICATION: vnc-026 — TS HTTP Hook Client + Client-Streamed Transcript Deltas (F3)

**Source scope**: `product/features/vnc-026/SCOPE.md` (approved; RQ-1..RQ-8 binding)
**Risk input**: `product/features/vnc-026/SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-11, A-1..A-4)
**Tracking**: GH #679 | Depends on vnc-024/#672 (shipped) and vnc-025/#670 (delivery gate)

---

## 1. Objective

Give remote (HTTP) Unimatrix deployments a hook client. A pure-JS, zero-dependency client in
`@dug-21/unimatrix` reads hook stdin, builds the same `HookRequest` the Rust hook builds, POSTs
to `{url}/observe` with Bearer auth, and emits host-envelope stdout — while streaming transcript
deltas on fire-and-forget events so the server's F2 buffer holds the authoritative conversation,
closing the remote PreCompact-restoration gap (#4676).

## 2. Domain Models & Ubiquitous Language

| Term | Definition |
|---|---|
| **Hook event** | A host-CLI lifecycle callback (e.g., UserPromptSubmit, PostToolUse) that spawns the client with the event name as `argv[2]` and a JSON payload on stdin. |
| **Canonical event name** | The normalized event name after mapping host aliases (Gemini `BeforeTool` → PreToolUse, `AfterTool` → PostToolUse, `SessionEnd` → Stop) — port of `normalize_event_name`/`map_to_canonical` in `hook.rs`. |
| **Sync trio** | UserPromptSubmit, PreCompact, SubagentStart — the only events whose response is written to stdout and whose latency the host CLI awaits. All other events are fire-and-forget. |
| **Fire-and-forget event** | An event whose request expects no usable response: 2xx (incl. 204) = success, never produces stdout. SessionRegister / SessionClose / RecordEvent / RecordEvents requests. |
| **HookRequest / HookResponse** | The frozen F1 wire types, authoritative form: committed ts-rs bindings at `crates/unimatrix-engine/bindings/` + 18 contract fixtures. Never hand-mirrored. |
| **Host envelope** | The host-CLI-specific stdout serialization: plain text for UserPromptSubmit/PreCompact; the `hookSpecificOutput` JSON envelope for SubagentStart. The client's only transformation surface. |
| **Transcript delta** | A `RecordEvent` with `event_type: "transcript_delta"` and payload `{ offset, bytes }` carrying transcript bytes `[last_offset, file_len)`. Non-elided frames declare `offset = last_offset`; elided frames are end-anchored, declaring `offset = file_len − bytes.length` (ADR-008). Not a new wire variant (vnc-024 ADR-004, #4720). |
| **last_offset** | Per-session client-persisted byte offset into the transcript file; the high-water mark of bytes already shipped. |
| **State dir** | `~/.unimatrix/{hash}/hook-client/` (subdir of the Rust project-hash scheme, RQ-2; `{hash}` = first 16 hex of SHA-256 of project-root path, identical to `project.rs::compute_project_hash`). Holds `offsets/`, `queue/`, and `health.json`. Dir mode 0700, files 0600 (ADR-003). The client's only durable state. |
| **Event queue** | Minimal disk queue in the state dir: non-delta fire-and-forget frames enqueued on send failure, replayed in order before the next successful send (RQ-1). `transcript_delta` frames are **never** queued — failed deltas re-derive from the offset on the next spawn (ADR-004). |
| **Health breadcrumb** | Content-free `health.json` in the state dir (last_success/last_failure timestamps, failure class, consecutive_failures, queue_depth, url_host — no token, payloads, transcript bytes, or full URL), written best-effort via atomic rename on every spawn that attempts a send (ADR-005). |
| **Parity corpus** | The committed fixture set of inputs + golden Rust-hook outputs used by AC-01/AC-05; doubles as F6 (hook.rs retirement) evidence. |
| **Remote mode** | Configuration produced by `init --remote <url> --token <tok>`: hooks call `node /abs/path/lib/hook-client/index.js <EVENT>`; URL + token in `.claude/settings.local.json` (gitignored) with env-var override. |
| **Ownership patterns** | `UNIMATRIX_PATTERNS` regexes in `merge-settings.js` that identify unimatrix-owned hook entries during idempotent merge; must be extended to recognize the `node …/hook-client/index.js` command form. |
| **Remote event set** | The full hook event list written by `init --remote`: SessionStart, Stop, UserPromptSubmit, PreToolUse, PostToolUse, PostToolUseFailure, SubagentStart, SubagentStop, PreCompact. |
| **Local event set (fixed)** | The existing 7-event `HOOK_EVENTS` list + PreCompact + PostToolUseFailure (RQ-8 fix; 9 events). |

Key relationships: one hook spawn handles exactly one event; a fire-and-forget event with a
`transcript_path` may trigger one additional, independent delta POST (RQ-5); all session keying
is the `session_id` string from the event's stdin — the server namespaces it (`http-{session_id}`).

## 3. Functional Requirements

Every FR is testable; verification lives in the AC table (§5) and the parity corpus.

### 3.1 Client core (`lib/hook-client/`)

- **FR-01** — Entry: `node lib/hook-client/index.js <EVENT>` reads stdin synchronously via
  file descriptor 0 (`fs.readFileSync(0)`, 1 MiB cap matching the Rust hook) — never
  `'/dev/stdin'`, which throws on Windows (R-14) — parses defensively
  (malformed/missing stdin never throws an unhandled error), normalizes the event name via the
  canonical map (incl. Gemini aliases; unknown events pass through as generic observation), and
  dispatches per the Rust `run()` split: fire-and-forget vs sync (`hook.rs:244-251`).
- **FR-02** — Request building: `build-request.js` produces `HookRequest` JSON equal to Rust
  `build_request` output for all 13 canonical events, including: the `MIN_QUERY_WORDS = 5` gate
  (UserPromptSubmit ≥5 words → ContextSearch, else generic RecordEvent), PostToolUse
  rework/failure extraction (`is_bash_failure`, `extract_file_path`, MultiEdit handling), and the
  SubagentStart fallback query derived from the transcript JSONL tail + `agent_type` role —
  ported as-is from Rust (RQ-6). Edge-case behavior (malformed JSONL, UTF-8 boundaries, missing
  fields) must match the Rust hook; the architect enumerates the exact ported `hook.rs` functions
  and their edge-case inventory (SR-01).
- **FR-03** — HTTP transport: POST `HookRequest` JSON to `{url}/observe` with
  `Authorization: Bearer <token>` using Node built-in `http`/`https` only. Sync requests add
  `Accept: text/plain`. Timeouts (ADR-005): connect 750 ms; sync request total 2,000 ms;
  fire-and-forget request total 3,000 ms — config-overridable via the `settings.local.json`
  block. Sync expiry → no stdout, exit 0. The client sends the raw `session_id`; the
  server mints `http-{session_id}`.
- **FR-04** — Host envelope output (`transform.js`): 200 `text/plain` body → stdout per envelope
  (plain text for UserPromptSubmit/PreCompact; for SubagentStart the `hookSpecificOutput`
  envelope `{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"<text>"}}`
  + trailing newline, byte-identical to `write_stdout_subagent_inject`). 204 or empty → no
  stdout. **The SubagentStart envelope is produced from a literal template, not generic object
  serialization** — field order and escaping pinned to the Rust output (SR-02); the committed
  fixtures are the only serialization authority.
- **FR-05** — Exit discipline: exit code 0 on every path — success, parse failure, unreachable
  server, timeout, non-2xx, queue failure, delta failure. No stdout on any failure path.
  Failures emit (a) a stderr one-liner `unimatrix: <class>: <message>` and (b) a best-effort,
  content-free health breadcrumb update (§2; failure classes
  `auth | connect | timeout | http_4xx | http_5xx`, 401/403 classified `auth`); both never
  block and never violate exit-0/no-stdout (ADR-005). `init --remote` Ping (FR-19) is the only
  loud checkpoint.

### 3.2 Configuration resolution (spawn time)

- **FR-06** — The client resolves URL + token at spawn, first hit wins (ADR-006):
  1. Env vars `UNIMATRIX_REMOTE_URL` + `UNIMATRIX_REMOTE_TOKEN` (canonical names; to be
     confirmed against F5 #681 naming before delivery). Exactly one of the pair set →
     misconfiguration (breadcrumb class `auth`, exit 0).
  2. `{project_root}/.claude/settings.local.json`, key
     `{ "unimatrix": { "remote": { "url", "token" } } }`. `project_root` is resolved as the
     Rust hook does: `stdin.cwd` if non-empty else `process.cwd()`, walk up to the first
     directory containing `.git` (`detectProjectRoot` port; no `.git` → resolved cwd). One
     stat-walk, one file read — no multi-location search. The same `project_root` string
     feeds the state-dir hash (ADR-003), so config identity and state identity never disagree.
  3. Neither present → breadcrumb + stderr line, exit 0, no stdout, no network.
  Resolution works from any subdirectory cwd (SR-09); test obligations include stdin-cwd ≠
  process-cwd, missing file, file without `unimatrix.remote`, env override beating a present
  file, and a partial env pair.

### 3.3 Transcript delta streaming (`delta.js`)

- **FR-07** — On fire-and-forget events carrying a `transcript_path`: stat the file; if
  `file_len > last_offset`, read `[last_offset, file_len)`, ship it as a `transcript_delta`
  `RecordEvent` `{ offset, bytes }` in a **separate second POST** (RQ-5, confirmed ADR-007),
  issued **concurrently** with the carrying event's POST (`Promise.allSettled`) with
  independently tracked outcomes. Offset advance on success is **uniform** (ADR-004/ADR-008):
  `last_offset` advances to `declared offset + bytes.length` — no elision special case. For
  normal frames that is the UTF-8 boundary-trimmed span end; for elided frames (end-anchored
  per FR-08) it is exactly `file_len`. On send failure, `last_offset` does **not** advance
  (see FR-12 carve-out). Unchanged file → no delta, no POST.
- **FR-08** — Truncation/elision (end-anchored, ADR-008): a delta whose raw span exceeds
  64 KiB ships a **single frame** (ADR-007 intact) with
  `bytes = head(48 KiB) ++ "…[N bytes elided]…" ++ tail(12 KiB)` and a declared
  `offset = file_len − bytes.length` — the frame **ends exactly at `file_len`**; the declared
  offset is NOT the span start `last_offset`. The cap is enforced against the
  **serialized frame** — after JSON string-escaping, the request body must stay under the 1 MiB
  guard, verified by a post-serialization size check (SR-04). The persisted offset advances per
  the uniform rule (FR-07) to `offset + bytes.length = file_len`; elided bytes are never re-sent.
  Pinned merged-F2 consequences, testable server-side assertions for the Layer 2 helper
  (ADR-008, against vnc-025 `session_transcript.rs` as merged in PR #692):
  - the hole forms **behind** the content at apply time:
    `holes == [(last_offset, file_len − bytes.length)]` — a true record of the elided region;
  - `high_water == file_len` — server coverage and client `last_offset` agree;
  - subsequent deltas extend contiguously at `file_len` (no further holes) and
    `contiguous_tail` windows cross the elision seam naturally — the client's tail bytes sit
    at their true file offsets `[file_len − 12,288, file_len)`;
  - NUL (hole) bytes are never served (vnc-025 invariant: holes never served — metadata-only
    elision, ADR-002 vnc-025).
- **FR-09** — Sync-path isolation: no transcript delta is ever produced on the sync trio, and
  the sync request path performs no transcript file I/O. (Exception by design: the SubagentStart
  fallback query reads the JSONL tail per FR-02/RQ-6 — that is query derivation, not delta
  streaming, and matches existing Rust behavior.)
- **FR-10** — Delta independence: a delta-send failure never fails, delays, or alters the
  carrying event's outcome, exit code, or stdout.
- **FR-11** — Offset integrity: offsets are tracked per `session_id`, monotonic. Rewrite guard
  (risk A-4, ADR-004/ARCHITECTURE): if `file_len < last_offset` (truncation/rewrite of the
  transcript), reset `last_offset = file_len` and ship nothing this spawn — never read a
  negative span, never re-ship old content, never mis-attribute. (Reset-to-`file_len` is safe
  because F2's merge is offset-bounded and idempotent — ass-069 Q1; content loss over
  mis-attribution per the degradation contract.) Offset files are written via temp file +
  atomic rename; concurrent spawns are last-writer-wins, worst case a re-shipped span deduped
  by F2's idempotent merge (ADR-003). Session keys used in offset filenames are sanitized:
  `session_id` if it matches `^[A-Za-z0-9_-]{1,64}$`, else `sha256(session_id).slice(0,16)`.

### 3.4 Disk event queue (`queue.js`)

- **FR-12** — Enqueue-on-failure: non-delta fire-and-forget frames that fail to send are
  enqueued to the state dir. Two exemptions, both absolute:
  - Sync-trio requests are **never** queued.
  - `transcript_delta` frames are **never** queued (ADR-004 carve-out): on delta send failure,
    `last_offset` does not advance and the next fire-and-forget spawn re-derives
    `[last_offset, file_len)` — a fresh, possibly larger span — from the transcript file
    itself (the transcript is the queue). Raw conversation bytes therefore have zero at-rest
    footprint on the client. The carrying event's own frame still queues normally; the two
    POSTs' outcomes are independent (ADR-007, FR-10).
  Accepted losses (degradation contract, ass-069 Q1/Q6): a failed delta on the session's final
  event leaves the tail unshipped (server-side observation reconstruction is the floor); an
  outage spanning heavy growth yields a catch-up delta that elides per FR-08.
- **FR-13** — Replay-before-send: on a subsequent spawn with a reachable server, queued frames
  are replayed in order before the new event's frame, then removed from the queue
  (purge-on-successful-replay).
- **FR-14** — Queue bounds and safety (SR-05, values per ADR-003):
  - Layout: one `HookRequest` frame per file at `queue/{ts_ms}-{pid}-{seq}.json`, created with
    `fs.writeFileSync(path, data, { flag: "wx" })` (O_CREAT|O_EXCL) — no shared mutable file,
    no locking; same-ms/same-pid collision bumps `seq`. Lexicographic order on the `{ts_ms}`
    prefix is age order.
  - Bounds (checked at enqueue): max **500 files** AND max **5 MiB** total; beyond either →
    drop-oldest. Age prune: queue files older than **24 hours** deleted, not replayed.
  - Permissions: dir 0700, files 0600.
- **FR-15** — Replay is bounded per spawn — at most **32 frames or 256 KiB** per
  fire-and-forget spawn (ADR-003), oldest first; each file deleted only after a 2xx; stop at
  first failure (leave remainder for the next spawn). Replay never runs on the sync-trio path
  (SR-03). Queue failures (full disk, corrupt frame) never affect exit code or stdout — every
  queue operation is wrapped, errors go to the breadcrumb + stderr only; a corrupt frame file
  (unparseable JSON) is deleted and replay continues (poison-pill immunity).
- **FR-16** — Queue retention/at-rest posture (SR-06, per ADR-003/ADR-004): the queue persists
  **only non-delta** fire-and-forget frames — raw transcript bytes never reach disk (FR-12
  carve-out eliminates, rather than mitigates, SR-06 for delta content). What is queued
  (tool_input/tool_response excerpts inside RecordEvent payloads) matches the existing Rust
  queue's exposure, tightened by 0600/0700 modes and the 24 h prune (vs Rust's 7 days —
  deliberate, secrets-adjacent payloads). Lifecycle: purge-on-successful-replay (FR-13); 24 h
  age expiry (deleted, not replayed); offset files prune at 7 days since `updated` and the
  session's offset file is deleted on successful SessionClose send. No encryption at rest in
  F3 — same posture as the shipped Rust queue; enterprise at-least-once/encrypted delivery is
  the named ass-069 Q7 gap (NOT in scope). An outage longer than 24 h loses queued telemetry —
  accepted under the degradation contract (content loss, never mis-attribution).

### 3.5 `init --remote` (RQ-4 — minimal boundary)

- **FR-17** — `npx @dug-21/unimatrix init --remote <url> --token <tok>` writes hook entries
  `node /abs/path/lib/hook-client/index.js <EVENT>` for the full remote event set (§2), merges
  idempotently preserving non-unimatrix hooks, and recognizes its own entries on re-run —
  `UNIMATRIX_PATTERNS` extended to match the node command form, with re-run coverage over
  configs containing **old-style** (`unimatrix hook`) entries (SR-08).
- **FR-18** — Token + URL stored in `.claude/settings.local.json` under the
  `unimatrix.remote` key (gitignored, per-project; mode 0600; merge-preserving — only the
  `unimatrix` subtree is touched; init warns if the file is not gitignore-covered, ADR-006);
  env-var override (`UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN`) takes precedence; the
  token never appears in `.claude/settings.json`, `.mcp.json`, or the hook command line (RQ-3).
- **FR-19** — Connectivity is validated at init via a `Ping` request to the server; failure is
  reported to the user (init is the interactive checkpoint — the one place misconfiguration is
  loud, SR-10).
- **FR-20** — Remote mode skips `.mcp.json` with an informative message and skips
  binary-dependent steps (DB pre-creation). Skills copying, CLAUDE.md block, mode selection
  remain F5 (#681).

### 3.6 Local-mode HOOK_EVENTS fix (RQ-8 — only local-mode change)

- **FR-21** — `HOOK_EVENTS` in `init.js` and `merge-settings.js` (incl. `EVENT_MATCHERS`) gains
  PreCompact and PostToolUseFailure — list + matcher entries only, no other local-mode behavior
  change (SR-07 blast-radius limit) — with a regression test asserting the full 9-event local
  set is written and recognized on re-run over pre-existing local configs.

### 3.7 Parity & contract verification (RQ-7)

- **FR-22** — Parity corpus: committed inputs + golden Rust-hook outputs covering all 13
  canonical events, Gemini aliases, unknown-event passthrough, and adversarial/defensive cases
  (missing stdin, malformed JSON, malformed transcript JSONL, UTF-8 boundary content, missing
  fields) — generated with the Rust hook as oracle **before** client modules are implemented
  (SR-01, design recommendation 1).
- **FR-23** — Layer 1 parity: deterministic server-buffer pre-population → byte-identical stdout
  vs the Rust hook for all event types against identical server responses (PreCompact compared
  against an identically populated buffer). Buffer pre-population is isolated behind a single
  test helper so vnc-025 internal changes localize (SR-11).
- **FR-24** — Layer 2 parity: integration test with streamed deltas + injected drops →
  content-equivalence modulo elision markers. The Layer 2 helper asserts the pinned
  post-elision server state per FR-08/ADR-008: hole behind the content
  `(last_offset, file_len − bytes.length)`, `high_water == file_len`, seam-crossing
  `contiguous_tail`, and no NUL bytes in any served restoration block.
- **FR-25** — Contract round-trip: client-built request JSON validates against the committed
  fixtures (`crates/unimatrix-engine/bindings/fixtures/*.json`), extending the
  `contract.test.mjs` style to client-produced frames including `transcript_delta_payload.json`.
- **FR-26** — Concurrency attribution test: ≥8 interleaved sessions with injected drops (modeled
  on the ass-069 PoC) show each server buffer containing only its own session's bytes.

## 4. Non-Functional Requirements

| ID | Requirement | Target |
|---|---|---|
| NFR-01 | Per-event spawn latency (entry + parse + build + transform + state-dir hash, server stubbed) | p50 ≤ ~12 ms, p95 ≤ 20 ms on the reference environment (AC-13); validates RQ-2 state-dir choice; cwd-keying only as measured fallback |
| NFR-02 | Sync trio latency ceiling | 500 ms (ass-068 budget); no added file I/O, no extra requests, no queue replay on the sync path |
| NFR-03 | Payload size | `lib/hook-client/` total < 100 KB |
| NFR-04 | Dependencies | Zero runtime dependencies; Node built-ins only (`http`, `https`, `fs`, `path`) |
| NFR-05 | Node compatibility | CI on Node 18, 20, 22, 24 (`engines: node >=18`) |
| NFR-06 | Code form | Plain CommonJS matching existing `lib/`; tests extend `packages/unimatrix/test/` (`node:test`) — cumulative test infrastructure |
| NFR-07 | Reliability posture | Fail-open: exit 0 always, host CLI never blocked or failed by the hook |
| NFR-08 | Frame size | Serialized request body < 1 MiB (server 413 guard); delta soft cap 64 KiB measured post-serialization |
| NFR-09 | Client state footprint | Bounded: per-session `last_offset` files + bounded event queue (≤500 files / 5 MiB / 24 h) + `health.json` breadcrumb, all under `~/.unimatrix/{hash}/hook-client/`; everything else stateless |
| NFR-10 | Secrets posture | No token in any checked-in file or process argv; queued transcript bytes governed by FR-16 retention |

## 5. Acceptance Criteria (binding, from SCOPE.md)

| AC | Summary | Verification method |
|---|---|---|
| AC-01 | Client `HookRequest` JSON-equal to Rust `build_request` for every parity-corpus case (13 events + Gemini aliases + unknown-event + malformed-stdin defensive cases) | Automated parity test: run both clients over the corpus, compare JSON (structural equality); corpus per FR-22 incl. adversarial cases |
| AC-02 | Fire-and-forget POSTs to `{url}/observe` with Bearer header; 2xx incl. 204 = success; no stdout | Unit/integration test against a stub server asserting method, path, headers, and empty stdout |
| AC-03 | Sync requests send `Accept: text/plain`; 200 body → stdout per host envelope; 204 → no stdout | Stub-server test per sync event type; header + stdout assertions |
| AC-04 | SubagentStart `hookSpecificOutput` envelope byte-identical to `write_stdout_subagent_inject` (field order, serialization, trailing newline) | Byte-compare vs golden Rust output; envelope pinned to literal template (FR-04/SR-02) |
| AC-05 | Two-layer parity suite passes (Layer 1 byte-identical w/ pre-populated buffer; Layer 2 content-equivalence modulo elision under drops) | FR-23 + FR-24 test runs in CI; PreCompact compared against identically populated server buffer; Layer 2 helper asserts the pinned post-elision server state (hole behind content, `high_water == file_len`, seam-crossing `contiguous_tail`, no NUL bytes served — FR-08/ADR-008) |
| AC-06 | Deltas ship `[last_offset, file_len)` as `transcript_delta` `{offset, bytes}` (non-elided: declared `offset = last_offset`); offset advances; no delta when unchanged | Integration test: grow/hold transcript file across spawns; assert POST presence/absence, declared offset, and persisted offset |
| AC-07 | >64 KiB delta truncated head 48 KiB + tail 12 KiB with `…[N bytes elided]…` in a single end-anchored frame: declared `offset = file_len − bytes.length` (frame ends at `file_len`, ADR-008); serialized frame < 1 MiB; offset advances to `file_len` per the uniform rule | Unit test with oversized + escape-heavy (worst-case inflation) content; assert declared offset is end-anchored (NOT the span start) and `offset + bytes.length == file_len`; post-serialization size assertion (SR-04) |
| AC-08 | No delta on sync trio; sync path performs no transcript file I/O | Test: sync events with `transcript_path` present → single POST only; fs-call interception/spy asserting no transcript reads on sync path |
| AC-09 | Unreachable/timeout/non-2xx → exit 0, no stdout, all event types; delta failure doesn't affect carrying event | Failure-matrix test (ECONNREFUSED, timeout, 401, 413, 500) × event types; delta-failure injection |
| AC-10 | No mis-attribution by construction; ≥8 interleaved sessions with drops → each buffer holds only its own bytes | FR-26 concurrency test against an F2 server; per-session byte tagging per ass-069 PoC method |
| AC-11 | `init --remote` writes idempotent node-command hooks for the full remote event set (incl. PreCompact, PostToolUseFailure), preserves foreign hooks, recognizes own entries on re-run, Ping-validates, stores token/URL in settings.local.json with env override, no token on argv, skips `.mcp.json` with message | Init test matrix: fresh config, re-run, config with foreign hooks, config with old-style `unimatrix hook` entries (SR-08); file-content + argv assertions |
| AC-12 | CI on Node 18/20/22/24; zero runtime deps; payload < 100 KB | CI matrix config; `package.json` dependency audit; size check in CI |
| AC-13 | Spawn p50 ≤ ~12 ms, p95 ≤ 20 ms (server stubbed) on reference env, recorded in testing artifacts | Benchmark harness (≥50 iterations, warmup) modeled on ass-068 Q1 method; results committed under `product/features/vnc-026/testing/` |
| AC-14 | Client-built JSON round-trips against committed contract fixtures incl. `transcript_delta_payload.json` | FR-25 contract test extension in CI |
| AC-15 | Send failure → enqueue (non-delta fire-and-forget frames only); next reachable spawn → in-order replay (bounded 32 frames / 256 KiB) before new event, queue drained; queue failures never affect exit/stdout; sync trio never queued; `transcript_delta` never queued (ADR-004) | Queue lifecycle test: fail → enqueue → recover → replay-order assertion → drain; queue-failure injection; sync-trio non-enqueue assertion; delta-failure case asserts **offset-non-advance + no queue file** (re-derive on next spawn), not queue presence |
| AC-16 | Local `HOOK_EVENTS` includes PreCompact + PostToolUseFailure with regression test (full local set written + recognized on re-run) | FR-21 regression test over fresh and pre-existing local configs |

**Verification note (stdin, R-14)**: Stdin reading (FR-01, `fs.readFileSync(0)`) must be verified
on Linux, macOS, and Windows. The AC-12 CI matrix covers Node versions; OS coverage comes from the
risk strategy's R-14 scenario.

## 6. User Workflows

### W1 — Remote install (operator)
1. Operator runs `npx @dug-21/unimatrix init --remote https://uni.example.com --token <tok>`
   in a project root.
2. Init detects the project root, Pings the server (loud failure if unreachable/unauthorized),
   merges hook entries into `.claude/settings.json`, writes URL + token to
   `.claude/settings.local.json`, prints the `.mcp.json` skip message.
3. No binary, no ONNX model downloaded; works on macOS/Windows/Linux with Node ≥18.

### W2 — Sync injection (host CLI, per event)
1. Host CLI fires UserPromptSubmit/PreCompact/SubagentStart → spawns
   `node …/hook-client/index.js <EVENT>` with JSON on stdin.
2. Client resolves config, builds the sync request (ContextSearch / CompactPayload /
   ContextSearch-with-SubagentStart-source), POSTs with `Accept: text/plain`.
3. 200 → host-envelope stdout (injection text); 204 → silent. Always exit 0 within budget.

### W3 — Fire-and-forget observation + delta (host CLI, per event)
1. Host CLI fires e.g. PostToolUse → client builds and POSTs the observation frame (no stdout).
2. If stdin carried `transcript_path` and the file grew: second POST ships the delta; offset
   persisted. Server merges into the F2 per-session buffer.

### W4 — Outage and recovery
1. Server unreachable: non-delta fire-and-forget frames are enqueued; delta sends fail without
   advancing `last_offset` (never enqueued — ADR-004); sync events silently degrade (no
   injection); the health breadcrumb records the failure class; host CLI is never blocked;
   every spawn exits 0.
2. Server returns: the next fire-and-forget spawn replays the queue in order (≤32 frames /
   256 KiB) before its own frame, then drains replayed frames; its delta covers the full
   accumulated span `[last_offset, file_len)` (eliding per FR-08 if oversized). Expired frames
   (>24 h) are deleted unreplayed.

### W5 — Remote PreCompact restoration (the #4676 closure)
1. Throughout the session, W3 deltas keep the server's F2 buffer current.
2. On PreCompact, the server builds the transcript block from its buffer and returns formatted
   text — the client just prints it. Remote reaches local fidelity.

## 7. Constraints

- **C-01 Frozen wire contract**: F1 types as-is from committed ts-rs bindings; no wire changes;
  fixtures are the only serialization authority (vnc-024 ADR-001 #4726; SR-02).
- **C-02 1 MiB body guard**: enforced post-serialization on the client (FR-08/NFR-08).
- **C-03 Sync budget**: 500 ms ceiling, ~12 ms spawn floor; sync path gains no file I/O, extra
  requests, or queue work (FR-09, FR-15).
- **C-04 Zero runtime deps / Node built-ins only**; < 100 KB; plain CommonJS; cumulative test
  infra under `packages/unimatrix/test/`.
- **C-05 Exit 0 always**; no stdout on failure.
- **C-06 No secrets in checked-in files or argv** (RQ-3); queue at-rest posture per FR-16.
- **C-07 No server-side changes**: gaps found in the F1/F2 surface are F2 rework, not F3.
- **C-08 Delivery gate**: vnc-025 (#670) must merge before F3 delivery gates run (AC-05/AC-10
  need the F2 buffer); design proceeds against the frozen wire contract, not F2 internals
  (SR-11, A-2).
- **C-09 Bounded client state**: `last_offset` files + bounded queue + health breadcrumb in
  `~/.unimatrix/{hash}/hook-client/` (RQ-1, RQ-2, ADR-003); all else stateless. No transcript
  bytes at rest (ADR-004).
- **C-10 RQ-8 blast radius**: local-mode change limited to the event list + matchers (SR-07).

## 8. Dependencies

| Dependency | Status | What F3 consumes |
|---|---|---|
| vnc-024 / #672 (F1) | Shipped, closed | ts-rs bindings + 18 fixtures; `/observe` content negotiation (`Accept: text/plain`); `transcript_delta` event type; `http-{session_id}` namespacing |
| vnc-025 / #670 (F2) | In flight (waves 0–3b) | Per-session buffer, offset-bounded idempotent merge, server-side PreCompact transcript block — **delivery gate only** |
| `packages/unimatrix/` | Existing | `lib/init.js` (`detectProjectRoot`, init flow), `lib/merge-settings.js` (ownership patterns, merge), `files: lib/` packaging, `node:test` suite |
| `crates/unimatrix-server/src/uds/hook.rs` | Existing (untouched) | Parity oracle: `run()`, `build_request`, `normalize_event_name`, `write_stdout*`, JSONL tail-parse, MIN_QUERY_WORDS, rework extraction |
| Research | Done | ass-068 (spawn GO, architecture, queue), ass-069 (attribution GO, delta mechanism), ass-067 Q3 (init --remote steps) |
| Runtime | — | Node ≥18, built-ins only |

## 9. NOT in Scope

- UDS transport for the TS client (F4 / ass-068 Chunk 3) — HTTP only.
- Full `init` unification: local-mode TS client selection, local transcript reader, macOS
  platform packages, skills copying, CLAUDE.md auto-append, mode selection (F5 #681).
  Sole exception: the RQ-8 `HOOK_EVENTS` fix (FR-21).
- Rust `hook.rs` retirement (Chunk 5); the local Rust path is untouched.
- Any server-side change (C-07).
- Subagent sidechain transcript capture (`subagent_transcript`, ass-071) — main
  `transcript_path` only.
- Distillation / buffer readers (crt-052 #689).
- Enterprise acknowledged-delivery / at-least-once audit path (ass-069 Q7 named gap).
- MCP-over-HTTP remote registration; `.mcp.json` is not pointed at the remote server.
- Codex/Gemini transcript-format delta validation (Gemini event-name mapping IS ported; delta
  streaming validated against Claude Code JSONL only — server is content-opaque).
- Bun runtime / Node SEA / WASM.
- Delta batching with the carrying event (RQ-5: separate POST; architect may revisit with
  burden of proof on batching).

## 10. Open Questions

All five original OQs were resolved by the architecture ADRs and folded into the FRs above:

| OQ | Resolution | Resolving ADR | Folded into |
|---|---|---|---|
| OQ-1 (SR-10) | Fail-open stays; stderr one-liner + content-free `health.json` breadcrumb; init Ping is the only loud checkpoint | ADR-005 | FR-05, FR-19, §2 |
| OQ-2 (FR-11/A-4) | `file_len < last_offset` → reset `last_offset = file_len`, ship nothing this spawn (safe via F2's idempotent offset-bounded merge, ass-069 Q1) | ADR-004 + ARCHITECTURE delta mechanics | FR-11 |
| OQ-3 (FR-14) | 500 files / 5 MiB / 24 h drop-oldest; O_EXCL one-frame-per-file `{ts_ms}-{pid}-{seq}.json`; atomic-rename offsets; sanitized session keys; 0600/0700 | ADR-003 | FR-14, FR-16 |
| OQ-4 (FR-03/FR-06) | Timeouts 750 ms connect / 2,000 ms sync / 3,000 ms fire-and-forget (config-overridable); env vars `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN`; root-anchored single-location resolution | ADR-005 + ADR-006 | FR-03, FR-06 |
| OQ-5 (SR-03/A-1) | Replay capped at 32 frames / 256 KiB per fire-and-forget spawn, stop-at-first-failure | ADR-003 | FR-15 |

Genuinely open:

- **OQ-6** — Env-var names `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN` are canonical for F3
  but must be confirmed against F5 (#681) naming before delivery (ADR-006 carries the same
  caveat). A rename before F3 ships is a find-replace; after, a compat shim.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced vnc-024 ADRs #4714 (text/plain negotiation scope), #4720 (transcript_delta as event_type value), #4726 (ts-rs drift-checked bindings) and vnc-025 ADRs #4741/#4743 (delta tee/merge, shared transcript-block core); all incorporated as frozen-surface dependencies (§8) and constraints (C-01, C-07).
- Reconciliation pass (agent-2-spec): briefing surfaced vnc-026 ADR entries #4754 (ADR-004), #4753 (ADR-003), #4757 (ADR-007); spec aligned to ADR-003..007 — delta never-queued carve-out, queue mini-spec values, fail-open breadcrumb, config precedence, offset reset semantics. OQ-1..5 resolved (§10).
- ADR-008 fold-in pass (agent-2-spec): briefing surfaced #4758 (ADR-008 end-anchored elision frame) and #4740 (vnc-025 ADR-002 server buffer); FR-07/FR-08 rewritten to end-anchored elision + uniform offset-advance rule; pinned merged-F2 server assertions added to FR-08/FR-24/AC-05; AC-06/AC-07 declared-offset wording corrected.
