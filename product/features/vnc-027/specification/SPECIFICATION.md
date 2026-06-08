# SPECIFICATION: vnc-027 — TS UDS Hook Client + Hook-Set Reduction (F4a)

GH Issue: #680. Source: `product/features/vnc-027/SCOPE.md` (approved), `SCOPE-RISK-ASSESSMENT.md`.
Predecessors: F1 vnc-024, F2 vnc-025, F3 vnc-026 (all merged). Sibling: vnc-030 (F4b, contractual attribution — out of scope here).

---

## 1. Objective

Give the TS hook client (`packages/unimatrix/lib/hook-client/`) a local UDS transport with full transport/framing parity to the Rust hook, so local users no longer depend on the compiled `unimatrix hook` binary — unblocking the dogfooding switchover and F6 (hook.rs retirement). Simultaneously reduce the registered hook set to the minimal-necessary set (ass-069 Q3) and land two carry-items: the C-04 size-gate redefinition and the F3 FR-16 offset-delete rekey.

## 2. Domain Models & Ubiquitous Language

| Term | Definition |
|------|------------|
| **HookRequest frame** | The transport-agnostic JSON payload one hook invocation sends. Identical bytes regardless of transport; the queue stores these raw, one file per frame. |
| **Frame (wire)** | 4-byte big-endian u32 length prefix + JSON payload, per `crates/unimatrix-engine/src/wire.rs` (`write_frame`/`read_frame`). `MAX_PAYLOAD_SIZE` = 1,048,576 bytes; zero-length and oversized frames rejected on read. |
| **Transport** | A module exposing `post(config, frame, opts) -> Promise<SendResult>`; never rejects. Two implementations: `transport-http.js` (F3, unchanged) and `transport-uds.js` (this feature). |
| **SendResult** | `{ok, status, contentType, body, failureClass}` — the uniform resolution contract both transports honor. |
| **Sync event / sync trio** | Events whose response is printed to stdout for context injection: ContextSearch (UserPromptSubmit), CompactPayload (PreCompact), Ping/briefing (SessionStart, SubagentStart). |
| **FNF (fire-and-forget)** | Events sent without reading a response: write frame, flush, disconnect. Rust semantics: write, **no read**, disconnect immediately. |
| **Transport/framing parity** | Byte-identical frames, identical sync stdout, identical fail-open and FNF semantics versus the Rust hook **for events both clients send**. This is the F4a parity bar. |
| **Event-set parity** | Both clients registering/sending the same event set. **Explicitly NOT a goal** (SR-06): the hook-set reduction is a deliberate divergence — the Rust hook keeps PreToolUse observation and SubagentStop; the TS client retires/optionalizes them. |
| **Parity corpus** | The F3 committed-golden suite (Rust-hook-as-oracle, drift check — vnc-026 ADR-001), extended here with a UDS layer. The corpus covers the **post-reduction TS event set only**; retired events are excluded by design. |
| **projectHash** | Hash identifying the project; derives both the state dir and the socket path `~/.unimatrix/{projectHash}/unimatrix.sock`. Must equal the Rust daemon's `{project_hash}` in all layouts, including worktrees (#679 resolution rules). |
| **Preformatted sync response** | OQ2 default position: the UDS listener returns sync responses already formatted as the injection text (parity-by-construction with `format_injection`), instead of typed frames the client must format. Wire change must be additive-only against the frozen F1 contract. |
| **Size gate** | `test/check-hook-client-size.js`. Redefined by C-04 (human decision 2026-06-08): comment-stripped ≤ 100 KB primary + raw ≤ 160 KB backstop. |
| **HOOK_EVENTS** | The event registration table in `merge-settings.js` that drives `.claude/settings.json` hook entries. |

## 3. Functional Requirements

Each FR is testable; verification methods appear in §5 ACs that bind them.

### 3.1 Size-gate redefinition (carry-item C-04 — merge-ordered FIRST)

- **FR-1**: `check-hook-client-size.js` gates `lib/hook-client/` on comment-stripped size ≤ 100,000 bytes, with a raw-size ≤ 160,000 bytes backstop. Both limits enforced; either breach fails the gate.
- **FR-2**: The comment stripper is dependency-free and string-literal-safe: it never removes `//` or `/* */` sequences occurring inside string or template literals, and is simple enough to audit by reading (no parser dependency).
- **FR-3**: The gate file header documents that cap changes are human decisions recorded on the feature issue.
- **FR-4**: No client addition (FR-5 onward) merges before FR-1–FR-3 (SR-02: client is at 99,997/100,000 raw bytes).

### 3.2 UDS transport (`transport-uds.js`)

- **FR-5**: `transport-uds.js` exposes `post(config, frame, opts) -> Promise<SendResult>` — the same contract as `transport-http.js` — and never rejects.
- **FR-6**: Wire framing is byte-identical to `wire.rs`: 4-byte BE u32 length prefix + JSON payload. The write path rejects (fails open, never sends) frames whose payload exceeds 1,048,576 bytes. The read path rejects declared lengths of zero or > 1,048,576.
- **FR-7**: Sync path: connect to the socket, write one frame, read exactly one response frame, resolve SendResult. Partial reads are accumulated until the declared length is satisfied or the connection ends (SR-05: the read loop and `process.exit(0)` sequencing must guarantee the full response is consumed and stdout flushed before exit — exact pattern is an architecture deliverable).
- **FR-8**: FNF path: write one frame, guarantee the kernel has accepted the full payload before disconnecting (flush/drain-before-close; `socket.destroy()` without drain is prohibited — SR-01), read nothing, disconnect. Server-side truncation must be detectable in tests (a truncated FNF frame is a test failure, not silent loss).
- **FR-9**: Connection, read, and write operations carry timeouts; any timeout resolves a failed SendResult (fail-open), never a hang past the latency budget.
- **FR-10**: Connect failure or server-absent: FNF frames are enqueued via the existing `queue.js`, one stderr one-liner is emitted, sync events fail silent (no stdout), exit code 0.
- **FR-11**: Uses only the Node `net` core module — no new npm dependencies.

### 3.3 Transport selection (`config.js`)

- **FR-12**: `config.resolve()` returns a `mode` of `"http"` or `"uds"`. Remote config present (env pair or `settings.local.json` `unimatrix.remote`) → `"http"`, behavior unchanged from F3. Remote config absent → `"uds"` with socket path derived from the same `projectHash` used for the state dir. The former terminal `{ok:false, reason:"missing"}` breadcrumb path is removed.
- **FR-13**: When remote config exists, HTTP wins unconditionally — no probing for a live local socket, no local-override key (OQ1 resolution; F5 owns any override knob).
- **FR-14**: `index.js` selects the transport once per spawn and injects it; dispatch (sync vs FNF), queue, replay, delta, and state modules are otherwise unchanged.
- **FR-15**: Socket-path derivation produces hashes identical to the Rust daemon's `{project_hash}` for: plain repo, git worktree, and symlinked-root layouts (SR-12). Verified by committed TS-vs-Rust hash fixtures.
- **FR-16**: No-daemon local UX: with no remote config and no running daemon, the client enqueues FNF frames (bounded by the existing queue age-prune/retention — SR-13), emits the FR-10 stderr one-liner, and exits 0. Queue retention bounds must be confirmed to apply to this path; if the existing age-prune does not cover it, that is a blocking architecture finding, not a silent acceptance.

### 3.4 Sync-response formatting over UDS (OQ2)

- **FR-17**: Default position (per scope risk SR-02 recommendation and uni-zero review): the UDS listener returns **server-side preformatted** sync responses, making `format_injection` formatting single-sourced (vnc-025 ADR-005 parity-by-construction). The TS client prints the preformatted body via the existing transform path (envelope-vs-plain dispatch on the `--- Unimatrix Context ---\n` header, per the vnc-026 fix — entry #4788).
- **FR-18**: The wire mechanism for FR-17 is **additive-only** against the frozen F1 contract: new optional field(s) with `skip_serializing_if`, no renames/removals, no `deny_unknown_fields`. Existing Rust-hook parity fixtures and ts-rs bindings pass byte-unchanged (SR-03).
- **FR-19**: If architecture rejects server-side preformatting (fallback path), a `format_injection` JS port must achieve byte-identical stdout and fit the FR-1 comment-stripped budget. Either way, the parity bar of AC-03 (byte-identical stdout goldens) is the binding test — the mechanism is the architect's decision, the bytes are not.

### 3.5 Round-trip parity (UDS layer of the parity corpus)

- **FR-20**: The F3 parity corpus gains a UDS layer: (a) framing fixtures — TS-built frames byte-compared against committed Rust-generated `wire.rs` fixtures; (b) round-trip — full corpus against a live listener; (c) stdout goldens for the sync trio.
- **FR-21**: **Parity bar split** (SR-06, binding): transport/framing parity is **full** — frames, sync stdout, FNF semantics, fail-open behavior must match the Rust hook byte-for-byte for every event the TS client sends. Event-set parity is **not a goal** — the UDS parity corpus contains the post-reduction event set only; PreToolUse standalone observation and (default-off) SubagentStop are excluded from the corpus, and their absence is never a parity failure.
- **FR-22**: **Accepted divergence register** (SR-04): the lone-surrogate divergence (Node `JSON.parse` accepts `\uD800` escapes that serde rejects → Rust falls back to empty input + ppid session_id; tracked as a node:test todo, entry #4788) is inherited and **formally excepted** from AC-03. The corpus must not contain lone-surrogate inputs in byte-compare cases; the todo remains tracked. Any new divergence discovered during F4a is either fixed or added to this register by explicit decision — never silently tolerated.
- **FR-23**: PreCompact: the TS UDS client never client-side-prepends the transcript block (preserving the F3 design). With streamed deltas in the F2 buffer, exactly one server-built block appears in CompactPayload output. Stated assumption (SR-11): one project uses one client; the mixed-client double-prepend matrix is documented by architecture but not solved here.
- **FR-24**: `transcript_delta` RecordEvents stream over UDS as frames and merge into the F2 session buffer via the existing listener path (`listener.rs:785,1025`), exercised end-to-end from the TS client.

### 3.6 Queue and replay

- **FR-25**: The queue remains transport-agnostic: frames enqueued under one transport replay successfully over the other on the next spawn (SR-10). Replay-before-send order is preserved. The TS client replays only its own queue layout (no cross-format reads of the Rust hook's `event-queue/`).
- **FR-26**: Replay over UDS uses the FNF socket semantics of FR-8 per frame; a failed replay leaves the frame queued (best-effort, matching hook.rs behavior).

### 3.7 Hook-set reduction

- **FR-27**: `buildPreToolUse` returns the cycle event when intercepting `context_cycle` (`cycle_start`/`phase-end`/`stop`) and a no-send sentinel otherwise; the fallthrough RecordEvent observation is removed. A non-`context_cycle` PreToolUse spawn sends nothing (no frame, no queue entry).
- **FR-28**: SubagentStop is omitted by default and opt-in via a settings key (durable, user-visible — OQ3 resolution), not a merge-settings flag. F5 owns any installer/UX surface around the key (SR-09); F4a defines only the key and its default-off behavior.
- **FR-29**: `merge-settings.js` `HOOK_EVENTS` and matchers updated: PreToolUse matcher narrowed to the cycle-interception tool(s); SubagentStop registered only when the opt-in key is set. Sync-injection events (SessionStart, UserPromptSubmit, SubagentStart, PreCompact), discrete signals (PostToolUse, PostToolUseFailure), and lifecycle events remain registered and unchanged.

### 3.8 Offset-delete rekey (carry-item, F3 FR-16)

- **FR-30**: The per-turn Stop→SessionClose offset delete is removed; the 7-day age-prune (`state.pruneOffsets`, wired into `runFireAndForget`) is the effective deletion mechanism. Per ADR-006 (authoritative): `TaskCompleted` is registered nowhere (`HOOK_EVENTS`, `.claude/settings.json`), so its delete branch is unreachable — it is retained only as a zero-cost provision keyed by canonical event equality, never frame type (Stop and TaskCompleted share the SessionClose frame), pinned by a unit test. The change is the keying only — no delta-streaming redesign (SR-08).
- **FR-31**: HTTP-path delta streaming behavior is externally unchanged except for the intended effect of FR-30 (no full re-stream from offset 0 every turn); existing F3 delta tests pass with at most the keying-related assertions updated.

### 3.9 Dogfooding switchover (post-merge, OQ4 resolution)

- **FR-32**: After the transport merges, this repo's `.claude/settings.json` switches to the TS client. The switchover ships with a cheap drop-detector (SR-07): a documented procedure or script comparing daemon-side event counts and/or queue residue before/after switchover, plus a stated rollback trigger (sustained event-count drop or queue growth → revert settings to the Rust hook).

## 4. Non-Functional Requirements

- **NFR-1 (Latency)**: < 20 ms per hook invocation, p95, server stubbed/local listener, **including project-root detection** — same measurement protocol as F3 AC-13.
- **NFR-2 (Size)**: `lib/hook-client/` ≤ 100,000 bytes comment-stripped and ≤ 160,000 bytes raw at every merge after FR-1 lands.
- **NFR-3 (Fail-open, F3 C-05)**: the client never throws to the host, always exits 0, writes nothing to stdout on any failure path, leaks no secrets to stderr or breadcrumbs, and wraps every fs/network call.
- **NFR-4 (Sync-path I/O budget)**: the sync trio gains no additional file I/O over F3.
- **NFR-5 (Platform)**: UDS local mode is Unix-only; Windows local mode is documented as unavailable (remote HTTP remains the Windows path). No shims.
- **NFR-6 (Dependencies)**: zero new npm dependencies; `net` core module only.
- **NFR-7 (Compatibility)**: mixed clients (Rust hook + TS client in different projects) coexist against one server with no feature flag; zero changes to `hook.rs`/`transport.rs`.

## 5. Acceptance Criteria (binding)

AC-01..AC-10 from SCOPE.md, verbatim in intent, with verification methods. AC-11/AC-12 added per scope-risk recommendations (SR-03, SR-08).

| AC | Criterion | Verification | Binds |
|----|-----------|--------------|-------|
| AC-01 | `transport-uds.js` frames byte-identical to `wire.rs`: 4-byte BE u32 prefix + JSON payload; write rejects > 1 MiB payloads; read rejects zero-length and > 1 MiB declared lengths. | node:test byte-compare against committed Rust-generated framing fixtures (corpus layer a). | FR-5, FR-6 |
| AC-02 | Transport selection: remote config → HTTP, behavior unchanged; no remote config → UDS at `~/.unimatrix/{projectHash}/unimatrix.sock`; `missing`-config breadcrumb path removed. Socket-path hash matches Rust for plain-repo, worktree, and symlink layouts. | node:test on `config.resolve()` mode matrix; TS-vs-Rust hash fixtures (FR-15). | FR-12–FR-16 |
| AC-03 | Round-trip **transport/framing** parity with the Rust hook over UDS against a live listener: identical HookRequest frames for the post-reduction parity corpus; sync trio (ContextSearch / CompactPayload / Ping) prints byte-identical stdout (formatting per the OQ2 mechanism); FNF events write-then-disconnect without reading. **Scope of "parity"**: per FR-21 the corpus excludes retired/optional events; per FR-22 the lone-surrogate divergence is formally excepted. | Integration suite against a live listener; committed stdout goldens (corpus layers b, c); server-side frame-truncation assertion for FNF (FR-8). | FR-7, FR-8, FR-17–FR-24 |
| AC-04 | Fail-open parity on UDS: server absent → FNF enqueued + one stderr line, sync silent, exit 0, no stdout. Queued frames replay over whichever transport the next successful spawn selects — verified **cross-transport both directions** (enqueue under UDS → replay over HTTP, and the reverse), with both ingest points accepting the replayed frames. | node:test with no listener; cross-transport replay integration test (SR-10). | FR-10, FR-16, FR-25, FR-26 |
| AC-05 | Latency < 20 ms per invocation, p95, server stubbed/local listener, including project-root detection. | Same measurement protocol as F3 AC-13, run against the UDS path. | NFR-1, FR-9 |
| AC-06 | The TS UDS client never client-side-prepends the PreCompact transcript block; with streamed deltas in the buffer, exactly one server-built block appears in CompactPayload output. | Integration test: stream deltas, fire PreCompact, assert single block in golden. | FR-23 |
| AC-07 | `transcript_delta` events stream over UDS as RecordEvent frames and merge into the F2 session buffer. | End-to-end test from TS client through `listener.rs` buffer merge; buffer content asserted. | FR-24 |
| AC-08 | PreToolUse standalone observation retired: non-`context_cycle` PreToolUse spawns send nothing; cycle interception fully preserved; SubagentStop optional, default-off via settings key; `HOOK_EVENTS`/matchers updated; sync-injection + PostToolUse/PostToolUseFailure + lifecycle untouched. | node:test on `buildPreToolUse` sentinel and cycle paths; merge-settings output snapshot diff; opt-in key on/off matrix. | FR-27–FR-29 |
| AC-09 | Size gate: comment-stripped ≤ 100 KB + raw ≤ 160 KB backstop; stripper dependency-free and string-literal-safe; gate header documents the human-decision rule. **Merges before any client addition.** | Gate self-tests incl. string-literal cases (`"// not a comment"`, template literals); CI gate green; merge order auditable in history. | FR-1–FR-4, NFR-2 |
| AC-10 | Offset-file delete no longer fires on per-turn Stop→SessionClose; the 7-day age-prune is the effective deletion mechanism per ADR-006 (`TaskCompleted` branch retained but unreachable, keyed by canonical event name — never frame type); no full re-stream from offset 0 every turn. | node:test on state keying, including the pinning test (TaskCompleted deletes; Stop must NOT) per ADR-006; multi-turn integration asserting offset persistence across turns. | FR-30 |
| AC-11 | Frozen F1 wire contract preserved: any OQ2 wire change is additive-only; all pre-existing Rust parity fixtures and ts-rs bindings pass **byte-unchanged**. | Existing Rust fixture suite + ts-rs binding check run unmodified; green = pass. | FR-18, NFR-7 |
| AC-12 | HTTP path regression guard: F3 remote behavior externally unchanged except the intended AC-10 keying effect; existing F3 parity and delta suites pass with at most keying-assertion updates. | Full F3 test suite on the merged branch; diff of changed assertions reviewed against FR-31. | FR-14, FR-31 |

## 6. User Workflows

### W1 — Local hook invocation (default local user)
1. Claude Code fires a hook event; spawn reads stdin, resolves config → no remote config → `mode: "uds"`, socket path derived from projectHash.
2. Sync event: connect, replay queue (best-effort), write frame, read one response frame, print preformatted injection to stdout, exit 0.
3. FNF event: connect, replay queue, write frame, drain, disconnect, exit 0 — no read, no stdout.

### W2 — Local outage and recovery
1. Daemon down: connect fails → FNF frame enqueued, one stderr line, exit 0; sync events silent, exit 0.
2. Daemon returns: next spawn replays queued frames before sending its own, over whatever transport that spawn selects.

### W3 — Remote user (unchanged)
1. Remote config present → `mode: "http"`; everything behaves exactly as F3 shipped, including offset handling per AC-10's new keying.

### W4 — Operator enables SubagentStop
1. Operator sets the opt-in settings key; `merge-settings.js` registers SubagentStop on next merge. Without the key, SubagentStop is absent from `.claude/settings.json`.

### W5 — Dogfooding switchover (this repo, post-merge)
1. Switch `.claude/settings.json` to the TS client; record pre-switch daemon event counts.
2. Soak with the drop-detector; on a rollback trigger, revert to the Rust hook and file a bug.

## 7. Constraints

1. **Frozen F1 wire contract — additive only** (SCOPE): no renames/removals; `skip_serializing_if` optionals; no `deny_unknown_fields`; AC-11 enforces.
2. **Rust hook untouched**: zero changes to `hook.rs`/`transport.rs` until F6 (also the parity-oracle stability assumption — any concurrent Rust-side change invalidates goldens mid-feature).
3. **Size budget**: AC-09 merges first; every subsequent addition is measured against the comment-stripped budget. The OQ2 server-side-preformatted default exists primarily to keep a `format_injection` port out of this budget.
4. **Fail-open contract** (NFR-3) applies to every new code path, including the UDS read loop and exit sequencing.
5. **No new npm dependencies**; **UDS is Unix-only** — document, don't shim.
6. **Sync-path budget**: no extra file I/O on the sync trio (NFR-4).
7. **Socket lifecycle is a contract** (SR-01/SR-05): flush/drain-before-close for FNF; full-frame read + stdout flush before `process.exit(0)` for sync. Architecture must specify the exact Node mechanics; the spec binds the observable guarantees (no truncated frames server-side, no truncated stdout).

## 8. Dependencies

- **F3 TS client** (`packages/unimatrix/lib/hook-client/`): `transport-http.js` contract, `config.js`, `queue.js`, `delta.js`, `transform.js`, `index.js` dispatch, parity corpus + drift check.
- **Rust oracle (read-only)**: `wire.rs:345-400` framing + round-trip tests (byte authority), `transport.rs` LocalTransport semantics, `hook.rs:174-321` flow + `format_injection` (hook.rs:963-1034) as the stdout-golden source.
- **UDS listener** (`crates/unimatrix-server/src/uds/listener.rs`): existing `transcript_delta` accept path (785, 1025); will gain the OQ2 preformatted-response option (additive).
- **`merge-settings.js`** HOOK_EVENTS table; **`test/check-hook-client-size.js`** (rewritten by FR-1).
- **#679 worktree-root resolution** (fresh, load-bearing for FR-15).
- Node core modules only: `net`, plus existing `fs`/`path`/`crypto` usage.

## 9. NOT in Scope

- **Contractual cycle attribution** (cycle tracker, `cycle_stamp`, precedence chain, `topic_source`, heuristic demotion) — vnc-030 (F4b, #699).
- **Rust `hook.rs` retirement or any change to it** — F6.
- **`init` unification / local-mode installer flow, transport-override knobs, SubagentStop opt-in UX** — F5 (#681). F4a defines the settings key only.
- **Client-side PreCompact transcript prepend over UDS** — prohibited by design, not missing.
- **Windows local mode** — remote HTTP is the Windows path.
- **Distillation / buffer consumers** — crt-052 (#689).
- **Lone-surrogate divergence fix** — remains a tracked todo (FR-22 excepts it); fixing it here is scope creep unless the human pulls it in.
- **Mixed-client PreCompact double-prepend mitigation** — documented assumption only (FR-23); any server-side guard change is out of scope.
- **Event-set parity with the Rust hook** — explicitly not a goal (FR-21).

## 10. Open Questions (for architecture)

1. **OQ2 mechanism confirmation**: the spec defaults to server-side preformatted UDS sync responses (FR-17/FR-18); architecture must confirm the additive wire shape (which optional field(s) on HookResponse, listener dispatch changes) or invoke the FR-19 fallback with a size-budget accounting.
2. **Node socket lifecycle pattern** (SR-01/SR-05): exact drain/close sequence for FNF (`end()` + drain event vs write-callback) and the sync read-loop/exit sequencing that satisfies §7.7 and NFR-1 together.
3. **OQ5 / SR-12 empirical check**: run the worktree-cwd stderr dump at design time — does hook stdin `cwd` for worktree-isolated subagent events carry the worktree path? Result feeds the FR-15 fixture set.
4. **Queue retention bound for the no-daemon path** (FR-16/SR-13): confirm the existing age-prune bounds indefinite enqueueing; if not, architecture proposes a minimal bound.
5. **Drop-detector shape** (FR-32/SR-07): script vs documented manual procedure, and the concrete rollback trigger thresholds.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — found #4798 (sync-formatting transport asymmetry; drove FR-17–FR-19), #4788 (lone-surrogate open divergence + envelope-header dispatch fix; drove FR-22 and FR-17's transform note), #4780 (size-gate Gate-3b rework lesson; reinforced FR-4 merge ordering), #4743 (vnc-025 ADR-005 shared-core parity-by-construction; supports the OQ2 default).
