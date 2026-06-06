# vnc-026: TS HTTP Hook Client + Client-Streamed Transcript Deltas (F3 — OSS-cloud remote MVP)

## Problem Statement

Remote (HTTP) deployments have no hook client. The server side is ready: `/observe` accepts
`HookRequest` JSON with Bearer auth (vnc-022), returns pre-formatted `text/plain` injection
text under content negotiation (vnc-024 / F1, shipped, #672 closed), and — once vnc-025 (F2,
in flight on `feature/vnc-025`, waves 0–3b complete) merges — buffers client-streamed
`transcript_delta` events into a per-session in-memory transcript. But nothing on the client
edge speaks this protocol: today's only hook client is the Rust binary (`unimatrix hook`,
UDS-only), which requires the 31 MB platform binary + ONNX model — unavailable on macOS/Windows
and pointless for a remote server.

vnc-026 is Chunk 2 / F3 of the ass-068 migration: the pure-JS hook client over HTTP, bundled in
the existing `@dug-21/unimatrix` npm package. It is the moment remote reaches local fidelity —
including the remote PreCompact-restoration gap (#4676), closed by streaming transcript deltas
so the server's F2 buffer holds the authoritative conversation.

Who is affected: every remote install (the OSS-cloud MVP path); the self-learning pipeline,
which today loses all conversational context for remote sessions.

Why now: F1 shipped and verified in code; F2 is in late-stage delivery; the two gating spikes
both passed — per-event Node spawn is GO (~12 ms, ass-068 Q1) and concurrent attribution is GO
(0 mis-attribution across 128 concurrent mixed-transport sessions, ass-069 Q1 PoC).

## Goals

1. **HTTP hook client**: `lib/hook-client/` in `@dug-21/unimatrix` — reads hook stdin JSON,
   normalizes the event name (canonical map incl. Gemini `BeforeTool`/`AfterTool`/`SessionEnd`),
   builds the same `HookRequest` the Rust hook builds, POSTs to `{url}/observe` with
   `Authorization: Bearer <token>`, and writes the host-envelope response to stdout. Zero
   runtime dependencies; Node built-ins only.
2. **Server-formatted sync path**: sync requests send `Accept: text/plain`; the server runs
   `format_injection` (F1). The client's transformation surface is host-envelope serialization
   only: plain text for UserPromptSubmit/PreCompact, the `hookSpecificOutput` JSON envelope for
   SubagentStart — byte-identical to the Rust hook's stdout.
3. **Client-streamed transcript deltas**: per-session `last_offset` persisted client-side;
   on fire-and-forget events carrying `transcript_path`, ship `[last_offset, file_len)` as a
   `transcript_delta` `RecordEvent` `{ offset, bytes }` in a **separate second POST** (RQ-5 —
   AC-09 independence by construction). Soft-cap 64 KiB per delta with
   head+tail truncation (keep head 48 KiB + tail 12 KiB, insert `…[N bytes elided]…` marker)
   under the existing 1 MiB frame/body guard. **Never** on the sync trio
   (UserPromptSubmit / PreCompact / SubagentStart) — no sync-budget regression (ass-068 Q1).
4. **Graceful degradation + minimal disk event queue**: server unreachable, timeout, or non-2xx
   → exit 0, no stdout, never blocks the host CLI. A minimal disk queue (RQ-1: enqueue
   fire-and-forget frames on send failure, replay-before-send on the next successful spawn)
   lives in the client state dir beside `last_offset`; F4 inherits it. A dropped delta loses
   content but never mis-attributes (offsets are per-session and monotonic; the server merge
   is offset-bounded and idempotent — ass-069 Q1).
5. **`init --remote <url> --token <tok>` — minimal boundary (RQ-4)**: configures
   `.claude/settings.json` hooks to `node /abs/path/lib/hook-client/index.js <EVENT>` for the
   full remote event set — including PreCompact and PostToolUseFailure, which the current
   `HOOK_EVENTS` list omits (known bug, ass-068 OOS-4; the local-mode list is also fixed here,
   RQ-8). Idempotent merge preserving non-unimatrix hooks (extend `merge-settings.js`
   ownership patterns). Token + URL stored in `.claude/settings.local.json` (gitignored,
   per-project) with env-var override; never on the hook command line (RQ-3). Connectivity
   validated via a `Ping` request at init time. `.mcp.json` is skipped in remote mode with an
   informative message. Skills copying, CLAUDE.md block, and mode selection are F5 (#681).
6. **Parity suite — two layers (RQ-7)**: Layer 1 — deterministic buffer pre-population →
   byte-identical stdout vs the Rust hook for all event types (incl. PreCompact); Layer 2 —
   integration test with streamed deltas + injected drops → content-equivalence modulo elision
   markers. Client-built `HookRequest` JSON round-trips against the committed contract fixtures
   (`crates/unimatrix-engine/bindings/fixtures/`). The parity corpus is a first-class design
   artifact doubling as F6 (hook.rs retirement) evidence.

## Non-Goals

- **UDS transport for the TS client** — ass-068 Chunk 3 (F4 track). HTTP only here.
- **Full `init` unification** (local-mode TS client selection, local transcript reader, macOS
  platform packages, skills copying, CLAUDE.md auto-append, mode selection) — F5 (#681).
  Exception (RQ-8): the local-mode `HOOK_EVENTS` PreCompact/PostToolUseFailure gap is fixed
  here with a regression test; F5 drops that deliverable.
- **Rust `hook.rs` retirement** — Chunk 5. The local Rust path is untouched.
- **Any server-side change.** The server surface this feature consumes (content negotiation,
  `transcript_delta` accept + buffer merge, PreCompact server-side transcript block) is
  F1 (shipped) + F2 (vnc-025). If F2 review surfaces server gaps, they are F2 rework, not F3.
- **Subagent sidechain transcript capture** (`subagent_transcript` event, ass-071) — a separate
  vnc-track feature. F3 streams the main `transcript_path` only.
- **Distillation / anything that reads the server buffer** — crt-052 (#689).
- **Enterprise acknowledged-delivery / at-least-once audit path** — named gap (ass-069 Q7).
- **MCP-over-HTTP remote registration** — `init --remote` configures hooks only; remote MCP is
  a future chunk. `.mcp.json` is not pointed at the remote server in F3.
- **Codex/Gemini transcript streaming formats** — event-name normalization for Gemini is ported
  (it is pure string mapping), but delta streaming is validated against Claude Code's JSONL
  only. The buffer is content-opaque server-side, so other providers degrade safely.
- **Bun runtime / Node SEA / WASM** — rejected or deferred by ass-068.

## Background Research

All claims verified in the workspace 2026-06-06 (branch `feature/vnc-025`).

### Primary inputs
- **ass-068 FINDINGS** (`product/research/ass-068/FINDINGS.md`): per-event Node spawn GO
  (~11.9 ms avg, p95 13.7 ms vs 500 ms sync budget); pure-TS architecture selected (Q2a);
  Chunk 2 definition and effort estimate (~730 lines incl. ~150-line event queue); sync I/O
  (`fs.readFileSync('/dev/stdin')`) recommended; hook command form `node /abs/path <EVENT>`.
- **ass-069 FINDINGS** (`product/research/ass-069/FINDINGS.md`): attribution gate GO; delta
  mechanism Q2 (per-session `last_offset` vs `transcript_path`, ship-on-fire-and-forget,
  64 KiB head+tail cap, persist offset "next to the event queue" in `~/.unimatrix/{hash}/`);
  12 KB tail is a PreCompact injection budget, not a transport limit — no leak.
- **ass-067 FINDINGS Q3** (`product/research/ass-067/FINDINGS.md`): `init --remote` step table
  (auth handshake via Ping, hook command shape, token storage options). Note: ass-067 Q4 said
  the thin client needs no event queue; ass-068 Q5 (later, authoritative) recommends a disk
  queue for both transports. Resolved — queue included (RQ-1).
- **ass-070 / ass-071**: distillation extractor quality and sidechain capture — consumers of
  the buffer this feature feeds; both explicitly out of scope here (crt-052 + a future
  vnc feature). ass-071 Q4/Q5 confirm sidechain capture is additive and does not constrain F3.

### Parity target — Rust hook (`crates/unimatrix-server/src/uds/hook.rs`, 4,183 lines)
- `run()` pipeline: read stdin (1 MiB cap) → defensive parse → `normalize_event_name` →
  `build_request` → fire-and-forget vs sync split (`hook.rs:244-251`: SessionRegister /
  SessionClose / RecordEvent / RecordEvents are fire-and-forget; ContextSearch / CompactPayload
  / Ping are sync) → always exit 0.
- Stdout envelopes the TS client must reproduce: `write_stdout` (`hook.rs:963`) — Entries →
  `format_injection` text via `println!` (now produced server-side under `text/plain`);
  BriefingContent → content if non-empty; `write_stdout_subagent_inject` (`hook.rs:994`) —
  `{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":"<text>"}}` +
  trailing newline. Empty content → silent skip, no stdout.
- SubagentStart fallback (`hook.rs:195+`): derives a ContextSearch query from the transcript
  tail + `agent_type` role — the JSONL tail-parse is ported to JS as-is (RQ-6: preserves AC-05
  byte-parity and F6 retirement evidence; needed for F4 anyway).
- `MIN_QUERY_WORDS = 5` gate on UserPromptSubmit → ContextSearch vs generic RecordEvent.
- Rework/failure extraction for PostToolUse (`is_bash_failure`, `extract_file_path`,
  MultiEdit handling) — part of `build_request` parity.

### Delivered F1 surface (vnc-024, #672 closed)
- ts-rs bindings committed at `crates/unimatrix-engine/bindings/` (`HookInput.ts`,
  `HookRequest.ts`, `HookResponse.ts`, `ImplantEvent.ts`, `EntryPayload.ts`,
  `TranscriptDeltaPayload.ts`) + 18 contract fixtures + `contract.test.mjs` (`node --test`).
- `TRANSCRIPT_DELTA_EVENT = "transcript_delta"` (`wire.rs:46`); payload `{ offset, bytes }`
  rides `ImplantEvent.payload` — not a new wire variant (vnc-024 ADR-004, entry #4720).
- Content negotiation in `http/router.rs:202-210` + `http/router/observe.rs:25-93`:
  `Accept: text/plain` → Entries formatted with the production `MAX_INJECTION_BYTES` budget
  (byte-identical to UDS, vnc-024 AC-07) → `200 text/plain`, or `204` when empty/over-budget;
  BriefingContent → `200 text/plain`; Ack → `204`; Pong/Error stay JSON (ADR-003, entry #4714).
- `prefix_session_id` mints `http-{session_id}` — client sends raw session_id; the server
  namespaces it.

### F2 surface (vnc-025, #670 open — in flight, waves 0–3b complete on this branch)
- Per-session in-memory buffer + offset-bounded idempotent merge replacing the F1
  accept-and-drop guard on both transports; accumulated-buffer cap
  (`transcript_buffer_max_bytes`, default 4 MiB); purge lifecycle + content-free audit;
  server-side PreCompact transcript block from the buffer (closes #4676 remote).
- **vnc-026 delivery is gated on vnc-025 merging**; design is not.

### npm package (`packages/unimatrix/`)
- Plain CommonJS (`"use strict"`, `require`), zero runtime deps, `engines: node >=18`,
  `node:test` test suite under `test/`. `files` includes `lib/` — a new `lib/hook-client/`
  ships automatically once added.
- `lib/init.js`: `detectProjectRoot()` (ports directly), `writeMcpJson`, `mergeSettings`
  call, DB pre-creation via the binary (must be skipped/replaced in remote mode — no binary).
- `lib/merge-settings.js`: `UNIMATRIX_PATTERNS` ownership regexes match only
  `unimatrix hook` / `unimatrix-server hook` commands — they will NOT recognize a
  `node …/hook-client/index.js` command as unimatrix-owned. Pattern set must be extended or
  remote-mode merging will duplicate/orphan entries.
- `HOOK_EVENTS` (init.js:9, merge-settings.js:18) omits PreCompact and PostToolUseFailure —
  confirmed bug (ass-068 OOS-4); the remote set must include both (PreCompact is a sync-trio
  member: without it, remote compaction defense never fires).

### Unimatrix knowledge consulted
- #4703 (pattern): HTTP clients bypassing the local binary receive raw JSON envelopes on sync
  events unless content negotiation is used — exactly the gap F1+F3 close.
- #4714 / #4720 / #4726 (vnc-024 ADRs): text/plain scope, accept-and-drop guard, ts-rs
  drift-checked bindings. #4739–4743 (vnc-025 ADRs): buffer placement, contiguous-span merge,
  delta tee, shared transcript-block core.

## Proposed Approach

Build `lib/hook-client/` as plain CommonJS (matching the existing package; the ts-rs `.ts`
bindings are erased types — contract fixtures are the runtime authority), with modules per the
ass-068 Q6 layout: `index.js` (entry: argv event + stdin → dispatch), `normalize.js` (event
canonicalization, ported from `map_to_canonical`/`normalize_event_name`), `build-request.js`
(port of `build_request` incl. PostToolUse rework extraction, the MIN_QUERY_WORDS gate, and
the SubagentStart JSONL tail-parse query derivation — RQ-6), `transport-http.js` (Node
`http`/`https`, Bearer header, `Accept: text/plain` on sync, timeouts), `transform.js`
(host-envelope stdout: plain text + `hookSpecificOutput`), `delta.js` (stat `transcript_path`,
read `[last_offset, file_len)`, truncate at 64 KiB head+tail, persist offset; ships as a
separate second POST — RQ-5), and `queue.js` (minimal disk queue: enqueue-on-failure,
replay-before-send — RQ-1). Client state (`last_offset`, queue) lives in
`~/.unimatrix/{hash}/`, matching the Rust scheme (RQ-2); the hash cost is verified against
AC-13, with cwd-keying as the measured fallback only. `init --remote` extends
`init.js`/`merge-settings.js` with a remote hook command + extended ownership patterns and
skips binary-dependent steps (`.mcp.json`: skip with informative message — RQ-4).

**Design directive (accepted review)**: treat `build-request.js` parity as the primary risk
area; the parity corpus is a first-class design artifact doubling as F6 retirement evidence.

Rationale for key choices:
- **HTTP-only, no UDS**: keeps F3 at the issue's boundary; the transport seam (a module, not an
  abstraction layer) leaves room for F4's `transport-uds.js`.
- **Server-side formatting via `Accept: text/plain`**: shrinks parity-critical client code to
  the ~40-line envelope layer (ass-068 Q4); parity is inherited from vnc-024 AC-07.
- **Deltas as ordinary `RecordEvent` frames**: inherits `SessionWrite` capability, bearer
  gating, the 1 MiB guard, and F2's idempotent merge with zero new wire surface.
- **Fail-open everywhere**: exit 0 always; delta failure must not fail the carrying event.

## Acceptance Criteria

- AC-01: `node lib/hook-client/index.js <EVENT>` with hook JSON on stdin builds a `HookRequest`
  JSON-equal to the Rust `build_request` output for every event type in the parity corpus
  (all 13 canonical events + Gemini aliases + unknown-event passthrough + missing/malformed
  stdin defensive cases).
- AC-02: Fire-and-forget requests POST to `{url}/observe` with `Authorization: Bearer <token>`;
  2xx (incl. 204) is success; no stdout is produced.
- AC-03: Sync requests (UserPromptSubmit ≥5 words → ContextSearch; PreCompact → CompactPayload;
  SubagentStart → ContextSearch with `source: "SubagentStart"`) send `Accept: text/plain`;
  a 200 text body is written to stdout per the host envelope; 204 → no stdout.
- AC-04: SubagentStart output is the `hookSpecificOutput` envelope byte-identical to
  `write_stdout_subagent_inject` (field order, JSON serialization, trailing newline).
- AC-05: Two-layer parity suite passes (RQ-7): Layer 1 — fixture-driven, deterministic buffer
  pre-population → byte-identical stdout vs the Rust hook for all event types against identical
  server responses (PreCompact compared with an identically populated server buffer); Layer 2 —
  integration test with streamed deltas + injected drops → content-equivalence modulo elision
  markers.
- AC-06: On fire-and-forget events with a `transcript_path`, the client ships
  `[last_offset, file_len)` as a `transcript_delta` `RecordEvent` `{ offset, bytes }` and
  advances the persisted per-session `last_offset`; no delta when the file is unchanged.
- AC-07: A delta exceeding 64 KiB is truncated head (48 KiB) + tail (12 KiB) with an
  `…[N bytes elided]…` marker, and the serialized frame stays under the 1 MiB body guard;
  the persisted offset still advances to `file_len` (no re-send of elided bytes).
- AC-08: No transcript_delta frame is ever produced on UserPromptSubmit, PreCompact, or
  SubagentStart; the sync request path performs no transcript file I/O.
- AC-09: Server unreachable, connect/request timeout, or non-2xx → exit code 0 and no stdout,
  on every event type; a delta-send failure does not fail or delay the carrying event.
- AC-10: Mis-attribution is impossible by construction in the client: deltas carry the
  session_id from the triggering event's stdin, offsets are tracked per session_id, and a
  concurrent multi-session test (modeled on the ass-069 PoC, ≥8 interleaved sessions with
  drops) shows each server buffer containing only its own session's bytes.
- AC-11: `npx @dug-21/unimatrix init --remote <url> --token <tok>` writes idempotent hook
  entries `node /abs/path/lib/hook-client/index.js <EVENT>` for the full remote event set
  including PreCompact and PostToolUseFailure, preserves non-unimatrix hooks, recognizes its
  own entries on re-run (ownership patterns extended), validates connectivity via Ping, stores
  token + URL in `.claude/settings.local.json` (gitignored) with env-var override taking
  precedence, never places the token on the hook command line, and skips `.mcp.json` in remote
  mode with an informative message.
- AC-12: CI runs the hook-client test suite on Node 18, 20, 22, and 24; the client has zero
  runtime dependencies; total `lib/hook-client/` payload < 100 KB.
- AC-13: Measured per-event spawn (entry + parse + build + transform + state-dir hash
  derivation, server stubbed) is within the ass-068 budget: p50 ≤ ~12 ms, p95 ≤ 20 ms on the
  reference environment, recorded in the feature's testing artifacts. This measurement
  validates the `~/.unimatrix/{hash}/` state-dir choice (RQ-2); cwd-keying is the fallback
  only if it fails.
- AC-14: Client-built request JSON validates against the committed contract fixtures
  (`bindings/fixtures/*.json`) — extending `contract.test.mjs`-style round-trip coverage to
  client-produced frames, including `transcript_delta_payload.json`.
- AC-15: On send failure, fire-and-forget frames are enqueued to the disk event queue in the
  client state dir — EXCEPT `transcript_delta` frames, which are never queued: on delta send
  failure, `last_offset` does not advance, and the next fire-and-forget spawn re-derives the
  span from `[last_offset, file_len)` (zero transcript bytes at rest client-side; the
  transcript file is the durable queue; F2's idempotent merge makes re-ship free). On delta
  send success, `last_offset` advances uniformly to `declared offset + bytes.length`
  (ADR-008 end-anchoring for elided frames). On a subsequent spawn with a reachable server,
  queued frames are replayed (in order, before the new event) and the queue drained. Queue
  failures never affect exit code or stdout, and sync-trio events are never queued.
  *Provenance: `transcript_delta` carve-out is an accepted variance per ADR-004, approved by
  the human at the design gate, 2026-06-06.*
- AC-16: Local-mode `HOOK_EVENTS` includes PreCompact and PostToolUseFailure
  (init.js + merge-settings.js matchers), with a regression test asserting the full local
  event set is written and recognized on re-run.

## Constraints

- **Wire contract is frozen**: F3 consumes F1's types as-is; no wire changes. Types come from
  the committed ts-rs bindings — never hand-mirrored (vnc-024 ADR-001, entry #4726).
- **1 MiB body guard**: `/observe` rejects larger bodies (413); the 64 KiB soft cap must hold
  after JSON string-escaping overhead.
- **Sync budget**: the sync trio must not gain file I/O or extra requests; ass-068's 500 ms
  budget is the ceiling, ~12 ms Node spawn the floor.
- **Zero runtime dependencies / Node built-ins only** (`http`, `https`, `fs`, `path`):
  supply-chain posture and the < 100 KB size gate.
- **Plain CommonJS** matching the existing `lib/` and `node:test` conventions (test
  infrastructure is cumulative — extend `packages/unimatrix/test/`).
- **Exit 0 always** — the host CLI must never see a failing hook (FR-03.7 inheritance).
- **No secrets in checked-in files**: the bearer token cannot land in `.claude/settings.json`,
  `.mcp.json`, or the hook command line; it lives in gitignored `.claude/settings.local.json`
  or an env var (RQ-3).
- **Dependency**: vnc-025 (#670) must merge before F3 delivery gates run (the delta/PreCompact
  ACs exercise the F2 buffer). Against an F1-only server, deltas are accepted-and-dropped —
  safe, but the parity/attribution gates need F2.
- **Client state is bounded**: the per-session `last_offset` plus the minimal disk event
  queue, both in `~/.unimatrix/{hash}/` (RQ-1, RQ-2). Everything else stays stateless
  (ass-068 Q2 confirmation).

## Resolved Questions

All open questions were resolved by the accepted uni-zero review (GH #679 reviewer comment),
approved by the human 2026-06-06.

- **RQ-1 — Disk event queue: INCLUDE.** ass-068 Q5 is authoritative over ass-067 Q4. Minimal
  enqueue-on-failure / replay-before-send; natural home for `last_offset`; F4 inherits it.
  (Goal 4, AC-15.)
- **RQ-2 — State dir: `~/.unimatrix/{hash}/`**, matching the Rust scheme (the per-spawn hash
  cost concern was overstated; the F4 UDS client shares the state dir, and the queue lives
  beside the offset). Verified against the AC-13 perf check; fall back to cwd-keying only if
  measurement contradicts.
- **RQ-3 — Token/URL: `.claude/settings.local.json`** (gitignored, per-project) with env-var
  override — matches issue #681 (F5). Never on the hook command line. (AC-11.)
- **RQ-4 — `init --remote` boundary: minimal.** F3 ships hooks + token + Ping only. Skills
  copying, CLAUDE.md block, mode selection → F5 (#681). `.mcp.json` in remote mode: skip with
  informative message. (Goal 5, AC-11.)
- **RQ-5 — Delta carrier: separate second POST.** AC-09 independence by construction; burden
  of proof is on batching; the architect may revisit. (Goal 3.)
- **RQ-6 — SubagentStart query: port the Rust JSONL tail-parse to JS.** Preserves AC-05
  byte-parity and F6 retirement evidence; needed for F4 anyway.
- **RQ-7 — Parity harness: two layers.** Layer 1: deterministic buffer pre-population →
  byte-identical PreCompact output (AC-05). Layer 2: integration test with streamed deltas +
  injected drops → content-equivalence modulo elision markers. (Goal 6, AC-05.)
- **RQ-8 — `HOOK_EVENTS` bug: fix the local PreCompact/PostToolUseFailure gap here** with a
  regression test (one-line list change + matchers); F5 (#681) drops that deliverable.
  (Goal 5, AC-16.)

## Open Questions

None — all resolved (see Resolved Questions above).

## Tracking

- GH Issue: #679 (`feat(vnc-026): TS HTTP hook client + client-streamed transcript deltas (F3)`)
- Dependencies: vnc-024 / #672 (closed — shipped), vnc-025 / #670 (open — in flight,
  `feature/vnc-025` waves 0–3b complete)
- Downstream consumers: ass-068 Chunks 3–5 (UDS transport, init unification, hook.rs
  retirement gate); crt-052 (#689) distillation reads the buffer F3 feeds
