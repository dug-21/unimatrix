# vnc-026 Implementation Brief — TS HTTP Hook Client + Client-Streamed Transcript Deltas (F3)

GH Issue: #679 | Branch dependency: vnc-025 (#670) MERGED via PR #692 — C-08 delivery gate satisfied.

## Delivery Notes / Gate Decisions (human-approved, design gate cleared)

1. **AC-15 variance ACCEPTED.** SCOPE.md AC-15 is amended to the ADR-004 letter: `transcript_delta`
   frames are NEVER queued — send failure means `last_offset` does not advance and the next FNF
   spawn re-derives `[last_offset, file_len)`. Delivery gates evaluate the **amended** AC-15
   (assert offset-non-advance + NO queue file for deltas), not the original queue-on-failure letter.
2. **Timeout defaults ACCEPTED as designed** (ADR-005): connect 750 ms / sync 2,000 ms /
   fire-and-forget 3,000 ms. Human reviewed the "why not shorter" rationale (cost asymmetry,
   ass-068 WAN p99 ~477 ms, 750 ms connect fast-fail) and accepted 2,000 ms sync. The NFR-02
   500 ms ceiling is the **normal-operation** budget; 2,000 ms is the **degraded-path** deadline —
   different regimes. Do NOT flag this as a conflict during delivery or review.
3. **Gate-note 1 (FR-01 `/dev/stdin`) is CLOSED.** Spec already mandates `fs.readFileSync(0)`;
   the tester must NOT reopen it. The fd-0-on-Windows test obligation stands under R-14.
4. **Env-var names PINNED** (ADR-006): `UNIMATRIX_REMOTE_URL` / `UNIMATRIX_REMOTE_TOKEN` are
   canonical; communicated to F5 (#681) by issue comment. OQ-6 closed.
5. **ass-071 carry-in notes** (context: `product/research/ass-071/FINDINGS.md`; notes, not new scope):
   - **Unknown-stdin-field parity**: verify the client does not drop or reorder stdin fields that
     `hook.rs` preserves via its `extra` flatten (`HookInput`, `wire.rs:71-72`) — unknown fields
     must survive the parity port (ass-071 Q5 relies on this behavior).
   - **SubagentStop stdin debug dump (freebie)**: while running delivery tests, opportunistically
     capture a raw SubagentStop stdin payload to answer ass-071's open question (does it carry the
     subagent's id/transcript path?). Commits to nothing; feeds ass-071/crt-052.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-026/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-026/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-026/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-026/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-026/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-026/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-026/ACCEPTANCE-MAP.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| index (entry/dispatch) | pseudocode/index.md | test-plan/index.md |
| config (resolution + root walk + hash) | pseudocode/config.md | test-plan/config.md |
| normalize (event canonicalization) | pseudocode/normalize.md | test-plan/normalize.md |
| build-request (parity port) | pseudocode/build-request.md | test-plan/build-request.md |
| transcript (JSONL tail-parse) | pseudocode/transcript.md | test-plan/transcript.md |
| transport-http | pseudocode/transport-http.md | test-plan/transport-http.md |
| transform (host envelopes) | pseudocode/transform.md | test-plan/transform.md |
| delta (offset tracking + delta POST) | pseudocode/delta.md | test-plan/delta.md |
| queue (disk event queue) | pseudocode/queue.md | test-plan/queue.md |
| state (state dir, atomic writes, breadcrumb) | pseudocode/state.md | test-plan/state.md |
| init-remote (init.js + merge-settings.js changes) | pseudocode/init-remote.md | test-plan/init-remote.md |
| parity-corpus (Rust generator + Layer 1/2 suites) | pseudocode/parity-corpus.md | test-plan/parity-corpus.md |

Pseudocode and test-plan files are produced in Session 2 Stage 3a; component list reflects
the architecture's module breakdown — actual file paths are filled during delivery.

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

Give remote (HTTP) deployments a hook client: a pure-JS, zero-dependency CommonJS client in
`@dug-21/unimatrix` that reads hook stdin, builds Rust-parity `HookRequest` JSON, POSTs to
`{url}/observe` with Bearer auth, emits host-envelope stdout on sync events, and streams
transcript deltas on fire-and-forget events so the F2 server buffer holds the authoritative
remote conversation — closing the remote PreCompact-restoration gap (#4676). `init --remote
<url> --token <tok>` wires it into `.claude/settings.json` (minimal boundary, RQ-4).

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Parity authority | Rust hook is the oracle; corpus goldens generated by an additive Rust dev-test, committed under `packages/unimatrix/test/fixtures/parity/`, CI drift-checked (regenerate + diff = zero). Mandatory edge-case inventory enumerated in the ADR. | SR-01, SR-02, scope review directive | architecture/ADR-001-parity-corpus-rust-oracle.md |
| Stdout envelope serialization | Literal template strings only; sole serializer call is `JSON.stringify` on the inner text scalar. Never serialize a whole envelope object. Goldens are the byte authority. | SR-02, AC-04 | architecture/ADR-002-envelope-literal-templates.md |
| Client state + disk queue mini-spec | `~/.unimatrix/{hash}/hook-client/{offsets/,queue/,health.json}`; O_EXCL one-frame-per-file enqueue; atomic-rename offsets/breadcrumb; bounds 500 files / 5 MiB / 24 h drop-oldest; replay ≤32 frames / 256 KiB per FNF spawn, stop-at-first-failure, poison-pill delete; 0700 dirs / 0600 files; sanitized session keys | RQ-1, RQ-2, SR-03, SR-05, SR-06 | architecture/ADR-003-client-state-and-disk-queue-mini-spec.md |
| Delta failure handling | `transcript_delta` frames are NEVER queued: on send failure `last_offset` does not advance; next FNF spawn re-derives `[last_offset, file_len)`. Zero transcript bytes at rest. Offset advance on success is **uniform**: `declared offset + bytes.length` (boundary-trimmed end for normal frames; `file_len` for end-anchored elided frames per ADR-008 — no truncation special case). **AC-15 variance ACCEPTED at gate; SCOPE AC-15 amended to this letter (Delivery Notes 1).** | SR-06, ass-069 Q1 | architecture/ADR-004-deltas-never-queued-offset-redrive.md |
| Observability under fail-open | stderr one-liner `unimatrix: <class>: <message>` + content-free `health.json` breadcrumb (atomic, best-effort) on every send-attempting spawn; `init --remote` Ping is the only loud checkpoint. Timeouts: connect 750 ms / sync 2,000 ms / FNF 3,000 ms, config-overridable. | SR-10 | architecture/ADR-005-fail-open-observability-breadcrumb.md |
| Config resolution | Env vars `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN` (partial pair = misconfig) > `{project_root}/.claude/settings.local.json` `unimatrix.remote` key (root = stdin.cwd else process.cwd(), walk to `.git`); single location, no probing; same root string feeds the state-dir hash. Neither → breadcrumb + exit 0, no network. | RQ-3, SR-09 | architecture/ADR-006-config-resolution-precedence.md |
| Delta carrier | Separate second POST confirmed (RQ-5 stands), issued concurrently with the carrying event (`Promise.allSettled`), independent outcomes; delta built only after `fstat` shows growth. Batching/piggyback rejected. | RQ-5, AC-09 | architecture/ADR-007-delta-separate-second-post-confirmed.md |
| Elision frame geometry | Elided frames are **end-anchored**: declared `offset = file_len − bytes.length` (frame ends exactly at `file_len`); `bytes = head(48 KiB) ++ marker ++ tail(12 KiB)`. Never declare span-start offset for an elided frame (phantom unfillable hole + permanent PreCompact starvation against merged F2). Pinned against merged vnc-025 buffer (`session_transcript.rs`, PR #692); no F2 rework (C-07 holds). Resolves R-06 (re-graded High→Medium). | R-06, gate-note 3, merged F2 | architecture/ADR-008-elision-frame-end-anchored-offset.md |
| Stdin read mechanism | `fs.readFileSync(0)` (fd 0), never `'/dev/stdin'` (throws on Windows). Resolved in current SPECIFICATION FR-01; the risk-strategy gate-note 1 referencing `'/dev/stdin'` is STALE — do not treat as open. | R-14, alignment WARN 1 | (spec FR-01; no separate ADR) |
| SubagentStart query derivation | Port the Rust JSONL tail-parse (`transcript_block.rs`) to JS as-is | RQ-6 | (scope-bound) |
| Local HOOK_EVENTS fix | PreCompact + PostToolUseFailure added to `HOOK_EVENTS` + matchers in both init.js and merge-settings.js, local AND remote modes (9 events); no other local-mode change | RQ-8, SR-07 | (scope-bound, FR-21) |
| Language/runtime | Plain CommonJS, zero runtime deps, Node built-ins only (`fs`, `path`, `http`, `https`, `crypto`, `os`, `process`), Node ≥18 | scope constraint | (scope-bound) |

## Files to Create

All under `packages/unimatrix/` unless noted:

| Path | Summary |
|------|---------|
| `lib/hook-client/index.js` | Entry: argv event, `fs.readFileSync(0)` (1 MiB cap), defensive parse, run() pipeline orchestration, top-level try/catch exit-0 guarantee |
| `lib/hook-client/config.js` | URL/token resolution (ADR-006); `detectProjectRoot` port; project hash (first 16 hex of SHA-256, per `project.rs::compute_project_hash`); state-dir path |
| `lib/hook-client/normalize.js` | `mapToCanonical`/`normalizeEventName` incl. Gemini aliases + `__unknown__` sentinel (port of `hook.rs:50-105`) |
| `lib/hook-client/build-request.js` | Full `build_request` port: ppid fallback, MIN_QUERY_WORDS=5, PostToolUse rework extraction, MultiEdit fan-out, PostToolUseFailure arm, `context_cycle` interception (MAX_GOAL_BYTES=1024), topic-signal extraction, SubagentStart prompt_snippet path (port of `hook.rs:440-951`) |
| `lib/hook-client/transcript.js` | JSONL tail-parse: 12,000-byte window, exchange pairing, MAX_PRECOMPACT_BYTES=3000, byte-safe `truncate_utf8` (port of `transcript_block.rs`) |
| `lib/hook-client/transport-http.js` | Node `http`/`https` POST to `{url}/observe`; Bearer + Content-Type headers; `Accept: text/plain` on sync; ADR-005 timeouts; response classification for breadcrumb |
| `lib/hook-client/transform.js` | Host-envelope stdout per ADR-002 literal templates; 204/empty → no stdout |
| `lib/hook-client/delta.js` | fstat + positioned read of `[last_offset, file_len)`; UTF-8 boundary trim; 64 KiB cap → head 48 KiB + tail 12 KiB + elision marker, **end-anchored**: declared `offset = file_len − bytes.length` (ADR-008, never span start); post-serialization 1 MiB assert; rewrite guard (`file_len < last_offset` → reset, ship nothing); uniform offset advance `offset + bytes.length` (ADR-004/ADR-008) |
| `lib/hook-client/queue.js` | ADR-003 mini-spec: O_EXCL enqueue, lexicographic bounded replay-before-send, drop-oldest, 24 h age prune |
| `lib/hook-client/state.js` | State-dir layout, atomic temp+rename writes, offset persistence (7-day prune, delete on SessionClose success), health breadcrumb, session-key sanitization (`^[A-Za-z0-9_-]{1,64}$` else sha256 prefix) |
| `test/fixtures/parity/` | Committed corpus: per-case `stdin.json` (+ optional `transcript.jsonl`) → `expected-request.json` + `expected-stdout.bin`, plus corpus manifest mapping every `build_request` arm to a case (R-02) |
| `test/` new suites | Hook-client unit + Layer 1 parity + Layer 2 integration + queue/config/init-remote/breadcrumb suites (`node:test`, cumulative — extend existing infra) |
| `crates/unimatrix-server` (additive dev-test) | Parity-corpus generator writing goldens into `packages/` via env-var path; CI must FAIL (not skip) if it doesn't run (R-20) |
| CI workflow | Node 18/20/22/24 matrix + Linux/macOS/Windows OS runners (R-14); zero-dep audit; <100 KB size check; corpus drift check |
| `product/features/vnc-026/testing/` | AC-13 benchmark results artifact |

## Files to Modify

| Path | Summary |
|------|---------|
| `packages/unimatrix/lib/init.js` | `--remote <url> --token <tok>` branch: write `settings.local.json` `unimatrix.remote` (0600, merge-preserving, gitignore warning), Ping validation (loud failure), skip `.mcp.json` + binary steps with messages; HOOK_EVENTS fix (FR-21) |
| `packages/unimatrix/lib/merge-settings.js` | `UNIMATRIX_PATTERNS` + node-command ownership pattern (resolve spaced-path `\S*` defect first — see Alignment Status); `mergeSettings` generalized to `commandSource = { commandForEvent(event), events }` with back-compat wrapper; HOOK_EVENTS + matchers gain PreCompact (`""`) and PostToolUseFailure (`"*"`) |

No server-side production changes (C-07). `uds/hook.rs` / `transcript_block.rs` are read-only oracles.

## Data Structures

- **HookRequest** (frozen, `crates/unimatrix-engine/bindings/HookRequest.ts`): discriminated union on `"type"` — `Ping` | `SessionRegister{session_id,cwd,agent_role,feature}` | `SessionClose{session_id,outcome,duration_secs}` | `RecordEvent{event}` | `RecordEvents{events}` | `ContextSearch{query,session_id,source?,role,task,feature,k,max_tokens}` | `CompactPayload{session_id,injected_entry_ids,role,feature,token_limit,transcript_excerpt?}`.
- **ImplantEvent**: `{event_type, session_id, timestamp:u64, payload:Json, topic_signal?, provider?}`.
- **Delta frame**: `RecordEvent` with `event_type: "transcript_delta"`, `payload: {offset: u64, bytes: string}` (`TranscriptDeltaPayload.ts`; not a new wire variant).
- **Offset file** `offsets/{session_key}.json`: `{ "offset": N, "updated": <unix secs> }`.
- **Queue file** `queue/{ts_ms}-{pid}-{seq}.json`: one HookRequest frame, O_EXCL-created.
- **health.json**: `{last_success, last_failure, failure_class: "auth|connect|timeout|http_4xx|http_5xx", consecutive_failures, queue_depth, url_host}` — content-free (no token/payloads/transcript bytes/full URL).
- **Config key** in `settings.local.json`: `{"unimatrix":{"remote":{"url","token"}}}` (+ optional timeout overrides block).

## Function Signatures (key interfaces)

- `index.js`: main pipeline mirrors `hook.rs::run()` step-for-step (read → parse → normalize → resolve cwd/root/config → buildRequest → SubagentStart fallback → sync/FNF dispatch → exit 0).
- `build-request.js`: `buildRequest(effectiveEvent, input) -> HookRequest` — pure, parity-tested.
- `transform.js`: SubagentStart envelope exactly `'{"hookSpecificOutput":{"hookEventName":"SubagentStart","additionalContext":' + JSON.stringify(text) + '}}\n'`; plain path `body + '\n'` iff non-empty.
- `merge-settings.js`: `mergeSettings(filePath, commandSource, options)` where `commandSource = { commandForEvent(event), events }`; back-compat wrapper preserves the local call site byte-identically.
- Sync/FNF split (identical to `hook.rs:244-251`): FNF = SessionRegister, SessionClose, RecordEvent, RecordEvents; sync = ContextSearch, CompactPayload, Ping.
- Hook command: `node /abs/path/lib/hook-client/index.js <EVENT>` (abs path via `require.resolve`).
- Client sends raw `session_id`; server mints `http-{session_id}` — client never prefixes.

## Constraints

- **C-01 Frozen wire contract**: F1 ts-rs bindings + fixtures only; never hand-mirrored.
- **C-02 1 MiB body guard**: enforced post-serialization client-side.
- **C-03 Sync budget**: 500 ms ceiling / ~12 ms spawn floor; sync path gains no file I/O (except the RQ-6 SubagentStart tail read — query derivation, not delta I/O), no extra requests, no queue replay.
- **C-04** Zero runtime deps / built-ins only; <100 KB; plain CommonJS; cumulative `node:test` infra.
- **C-05** Exit 0 always; no stdout on any failure path.
- **C-06** No secrets in checked-in files or argv; queue at-rest posture per FR-16.
- **C-07** No server-side changes (gaps = F2 rework, not F3).
- **C-08 Delivery gate**: SATISFIED — vnc-025 (#670) merged via PR #692; AC-05/AC-10 gates may run against the merged F2 server.
- **C-09** Bounded client state: offsets + queue + breadcrumb only; all else stateless.
- **C-10** RQ-8 blast radius: local-mode change limited to event list + matchers.

Hard limits table: stdin 1 MiB; body 1 MiB; delta soft cap 64 KiB (48+12 head/tail); MIN_QUERY_WORDS 5; MAX_GOAL_BYTES 1024; tail window 12,000 B; MAX_PRECOMPACT_BYTES 3000.

## Dependencies

| Dependency | Status | Consumed |
|---|---|---|
| vnc-024 / #672 (F1) | Shipped | ts-rs bindings + 18 fixtures; `/observe` content negotiation; `transcript_delta` event type; `http-` namespacing |
| vnc-025 / #670 (F2) | **Merged (PR #692)** | Offset-bounded idempotent buffer merge + server-side PreCompact block; elision semantics pinned against `session_transcript.rs` as merged (ADR-008) |
| `packages/unimatrix/` | Existing | `init.js`, `merge-settings.js`, packaging (`files: lib/`), `node:test` suite |
| `uds/hook.rs` + `uds/transcript_block.rs` | Existing, untouched | Read-only parity oracle |
| Runtime | — | Node ≥18, built-ins only |

## NOT in Scope

- UDS transport for the TS client (F4 / #680) — HTTP only.
- Full init unification: local-mode TS client selection, transcript reader, macOS platform packages, skills copying, CLAUDE.md auto-append, mode selection (F5 #681). Sole exception: FR-21 HOOK_EVENTS fix.
- Rust `hook.rs` retirement (F6 #682); local Rust path untouched.
- Any server-side change.
- Subagent sidechain transcript capture (ass-071); main `transcript_path` only.
- Distillation / buffer readers (crt-052 #689).
- Enterprise acknowledged-delivery / encrypted-at-rest path (ass-069 Q7 named gap).
- MCP-over-HTTP remote registration; `.mcp.json` never points at the remote server.
- Codex/Gemini transcript-format delta validation (event-name mapping IS ported).
- Bun / Node SEA / WASM; delta batching with the carrying event.

## Alignment Status (from ALIGNMENT-REPORT.md)

**Verdict: PASS with 1 variance for human approval + 4 WARNs. No FAILs.**

**Variance 1 — RESOLVED: ACCEPTED at gate** (see Delivery Notes 1) — AC-15 carve-out (ADR-004):
SCOPE AC-15's letter said fire-and-forget frames are enqueued on send failure; the spec
exempts `transcript_delta` frames (offset non-advance + re-derive next spawn). Human accepted;
SCOPE.md AC-15 is amended to the ADR-004 letter. The AC-15 verification asserts
offset-non-advance + NO queue file for deltas, not queue presence.

**WARNs (resolve/observe during delivery):**
1. **Gate-note 1: RESOLVED** — RISK-TEST-STRATEGY now marks it resolved; spec FR-01 mandates `fs.readFileSync(0)`, the `/dev/stdin` form is gone. Still standing from this note: AC-12's CI matrix must include OS coverage (R-14 scenario 2).
2. **Ownership regex spaced-path defect — STILL OPEN**: `/(^|\s|\/)node\s+\S*\/hook-client\/index\.js\s/` fails on install paths containing spaces (`C:\Program Files\`, `~/My Projects/`). Must be resolved before the AC-11 pattern freezes; confirm `require.resolve` output shapes on Windows/macOS first. This is the only remaining open gate note.
3. **Env-var names: RESOLVED** — `UNIMATRIX_REMOTE_URL`/`UNIMATRIX_REMOTE_TOKEN` PINNED as canonical at gate (ADR-006); communicated to F5 (#681) by comment. OQ-6 closed (see Delivery Notes 4).
4. **OS CI matrix expansion**: SCOPE AC-12 names only Node versions; R-14 adds Linux/macOS/Windows runners — justified (Windows/macOS support is the feature's stated purpose) but beyond the scoped AC letter.

**Cross-feature coordination (gate-note 3): RESOLVED via ADR-008** — vnc-025 merged (PR #692); elision semantics pinned end-anchored against the merged buffer, no F2 rework. R-06 re-graded High→Medium (likelihood Medium→Low; residual = client span-start regression). The Layer-2 helper must assert the four pinned items (R-06): hole behind the content at `(last_offset, file_len − bytes.length)`; `high_water == file_len`; `contiguous_tail` crosses the elision seam; no NUL bytes ever served.

## Open Questions for Delivery

- ~~OQ-6 (env-var naming vs F5 #681)~~ — CLOSED at gate: names pinned (Delivery Notes 4).
- ~~AC-15 variance approval~~ — CLOSED at gate: variance ACCEPTED, SCOPE AC-15 amended (Delivery Notes 1).
- Ownership-regex spaced-path resolution (WARN 2) — design-level fix expected during Stage 3a pseudocode for `init-remote`. **Only remaining open gate note.**
