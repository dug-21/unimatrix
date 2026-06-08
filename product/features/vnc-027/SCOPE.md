# vnc-027 — TS UDS Hook Client + Hook-Set Reduction (F4a)

GH Issue: #680. Fourth feature of the OSS-cloud finalization sequence (F1 vnc-024, F2 vnc-025, F3 vnc-026 — all merged). Split (uni-zero decision, 2026-06-08): the contractual-attribution half of the former F4 bundle is now vnc-030 (F4b, #699) — wire-independent of this feature. Delivery order pinned (human decision, 2026-06-08): **vnc-027 → vnc-030 → crt-052**; this feature delivers first and owns the C-04 gate redefinition (AC-09).

## Problem Statement

Two problems, one feature:

1. **Two hook clients, two languages.** The TS hook client (F3, `packages/unimatrix/lib/hook-client/`) is HTTP/remote-only. Local users still depend on the compiled Rust hook (`unimatrix hook`, crates/unimatrix-server/src/uds/hook.rs) over UDS. This contradicts the single-edge-language vision (JS/TS only) and blocks F6 (hook.rs retirement). With no remote config, the TS client today drops every event (breadcrumb + exit 0) — it has no local path at all.
2. **Hook-set noise.** Standalone PreToolUse observation duplicates the PostToolUse signal (every tool fires both); SubagentStop is mandatory. ass-069 Q3: retire the former's observation role (keep `cycle_start`/`cycle_stop` interception), make the latter optional.

Why now: F3 shipped the TS client framework, client-streamed deltas, and (ahead of schedule) the worktree gitdir-resolution port — every F4a precondition that belonged to F3 is met. The C-04 size-gate redefinition (human decision, 2026-06-08) unblocks the client additions. F4a merging early unblocks the dogfooding switchover and de-risks F5 (#681).

## Goals

1. **`transport-uds.js`** — local UDS transport for the TS hook client: Node `net.connect` to `~/.unimatrix/{projectHash}/unimatrix.sock`, 4-byte BE u32 length-prefix framing byte-identical to `wire.rs` (`write_frame`/`read_frame`: 1 MiB `MAX_PAYLOAD_SIZE`, zero-length reject). Transport selected by config: socket path → UDS, server URL → HTTP. Event queue shared across transports (queue stores transport-agnostic HookRequest JSON frames).
2. **Round-trip parity with the Rust hook over UDS** — same HookRequest frames (extend the F3 parity corpus / goldens), same sync request/response semantics (typed HookResponse frames; sync-response formatting per OQ2), same FNF write-then-disconnect, same fail-open guarantees (exit 0, no stdout on failure, enqueue on connect failure).
3. **Hook-set reduction** — retire standalone PreToolUse *observation* (keep cycle-event interception); make SubagentStop optional. Keep sync-injection (SessionStart, UserPromptSubmit, SubagentStart, PreCompact), discrete signals (PostToolUse, PostToolUseFailure), and lifecycle events. Update `merge-settings.js` `HOOK_EVENTS` accordingly.
4. **Carry-items (#680 comment, 2026-06-08)** — (a) C-04 size gate redefinition: comment-stripped ≤ 100 KB + raw ≤ 160 KB backstop, trivially auditable stripper, cap changes are human decisions on the feature issue; (b) FR-16 offset delete keyed to `TaskCompleted`/age-prune instead of per-turn Stop→SessionClose.

## Non-Goals

- **Contractual cycle attribution** (cycle tracker, `cycle_stamp`, server precedence chain, `topic_source`, heuristic demotion, protocol re-declaration line) — vnc-030 (F4b, #699).
- **Rust `hook.rs` retirement or any change to it** — F6. Mixed clients coexist by construction; the Rust hook needs zero changes.
- **Full `init` unification / local-mode installer flow** (TS-client selection at init, platform packages, mode selection UX) — F5 (#681). F4a delivers the transport and selection mechanics; F5 owns the installer story.
- **Client-side PreCompact transcript-block prepend over UDS** — explicitly prohibited (see Constraints), not a missing feature.
- **Windows local mode** — UDS is Unix-only, parity bar is the Unix-only Rust hook. Remote HTTP remains the Windows path.
- **Distillation / buffer consumers** — crt-052 (#689).

## Background Research

All claims verified in this workspace 2026-06-08 (branch `main`, post-F3-merge).

### Rust side (the parity oracle)
- **Framing** (`crates/unimatrix-engine/src/wire.rs:345-400`): `write_frame` = 4-byte BE u32 length + JSON payload, rejects payload > `MAX_PAYLOAD_SIZE` (1,048,576); `read_frame` rejects zero-length and oversized frames. Existing round-trip tests at wire.rs:630+ are the byte authority.
- **Transport** (`crates/unimatrix-engine/src/transport.rs`): `LocalTransport` — `UnixStream::connect` with read/write timeouts; `request()` = write frame, read one response frame, `Error` response → `Rejected`; `fire_and_forget()` = write frame, **no read**, disconnect immediately.
- **Hook flow** (`crates/unimatrix-server/src/uds/hook.rs:174-321`): socket path = `~/.unimatrix/{project_hash}/unimatrix.sock`; connect → replay queue (best-effort) → disconnect → reconnect → send; `Unavailable` → enqueue FNF, silent-drop sync; always exit 0. **Sync responses are formatted client-side**: `write_stdout` → `format_injection(items, MAX_INJECTION_BYTES)` (hook.rs:963-1034) — unlike HTTP, where vnc-024 moved formatting server-side (content negotiation, `Accept: text/plain`). The TS UDS sync path therefore needs a `format_injection` parity port **or** a server-side formatting option for UDS — a real architecture decision with size-budget impact (see OQ2).
- **PreCompact**: the Rust hook prepends the transcript block client-side (hook.rs Step 5d). The server also builds a block from the F2 session buffer (`listener.rs:1650`, shared `extract_transcript_block_from_bytes` core, vnc-025 ADR-005); its empty-buffer guard only protects clients that never stream deltas.

### TS client (F3, what F4a extends)
- `transport-http.js` is the only network module; resolves a `SendResult{ok, status, contentType, body, failureClass}`, never rejects. `index.js` dispatches sync vs FNF identically to hook.rs and routes everything through `transport.post` — the seam for transport selection is narrow and clean.
- `config.js::resolve()` currently knows only remote config (env pair → `settings.local.json` → `{ok:false}`); a config miss is a terminal breadcrumb. F4a changes this: no remote config → local UDS mode (socket path derived from the same `projectHash` the state dir already uses — no new config needed for the default local case).
- `queue.js` stores raw HookRequest JSON frames, one file per frame, replay-before-send — already transport-agnostic; only the replay sender changes per transport. Note: layout is deliberately distinct from the Rust hook's `event-queue/` (no cross-format reads) — the TS UDS client replays its own queue only.
- `delta.js` ships `transcript_delta` RecordEvents; the UDS listener already accepts them (`listener.rs:785,1025`, vnc-024 ADR-004 accept-and-drop + vnc-025 buffer merge) — delta streaming over UDS is server-ready.
- **Size**: `lib/hook-client/` is at **99,997 / 100,000 bytes — 3 bytes of headroom**. The C-04 redefinition is load-bearing and must land before or with the first client addition.
- `index.js` already does **no client-side PreCompact transcript prepend** (F3 design) — the UDS path must preserve this.

### Hook set today
- Nine events registered (`.claude/settings.json`, `merge-settings.js::HOOK_EVENTS`): SessionStart, Stop, UserPromptSubmit, PreToolUse(*), PostToolUse(*), PostToolUseFailure(*), SubagentStart(*), SubagentStop(*), PreCompact. ass-069 Q3: minimal-necessary set keeps everything except standalone PreToolUse observation and mandatory SubagentStop.

## Proposed Approach

1. **Transport layer**: add `transport-uds.js` exposing the same `post(config, frame, opts) -> Promise<SendResult>` contract as `transport-http.js` (sync = write frame + read response frame; FNF = write + destroy socket, no read). `config.resolve()` gains a `mode` (`"uds"`/`"http"`): remote config present → HTTP (unchanged); absent → UDS with derived socket path (replacing today's terminal `{ok:false, reason:"missing"}`). `index.js` picks the transport once per spawn; queue/replay/delta/state are untouched except for transport injection. Sync stdout for UDS routes typed HookResponse frames through the OQ2 formatting mechanism.
2. **Parity**: extend the F3 parity corpus (Rust-hook-as-oracle, committed goldens, drift check — vnc-026 ADR-001) with a UDS layer: byte-identical frames against `wire.rs` fixtures, round-trip against a real listener, stdout goldens for the sync trio.
3. **Hook-set reduction**: `buildPreToolUse` returns the cycle event on interception and a no-send sentinel otherwise (today's fallthrough RecordEvent observation retired); `HOOK_EVENTS`/matcher updates in `merge-settings.js`; SubagentStop behind an opt-in.
4. **Carry-items**: rewrite `check-hook-client-size.js` (comment-stripped ≤ 100 KB primary + raw ≤ 160 KB backstop, string-literal-safe stripper, no dependency) as the **first** change; key `state.deleteOffset` to `TaskCompleted` + age-prune.

## Acceptance Criteria

Transport / parity:
- AC-01: `transport-uds.js` frames are byte-identical to `wire.rs`: 4-byte BE u32 length prefix + JSON payload; rejects building frames > 1 MiB; read path rejects zero-length and > 1 MiB declared lengths. Verified against committed Rust-generated framing fixtures.
- AC-02: Transport selection — remote config (env pair or `settings.local.json unimatrix.remote`) → HTTP, unchanged behavior; no remote config → UDS to `~/.unimatrix/{projectHash}/unimatrix.sock`. The former `missing`-config breadcrumb path is replaced by local mode.
- AC-03: Round-trip parity with the Rust hook over UDS against a live listener: identical HookRequest frames for the full parity corpus; sync trio (ContextSearch / CompactPayload / Ping) returns and prints byte-identical stdout (Entries formatting per OQ2 mechanism, BriefingContent, SubagentStart envelope); FNF events write-then-disconnect without reading.
- AC-04: Fail-open parity on UDS: server absent → FNF enqueued + stderr one-liner, sync silent, exit 0, no stdout; queued frames replay over whichever transport the next successful spawn selects (shared queue).
- AC-05: Latency < 20 ms per invocation (p95, server stubbed/local listener) including project-root detection — same measurement protocol as F3 AC-13.
- AC-06: The TS UDS client never client-side-prepends the PreCompact transcript block; with streamed deltas in the buffer, exactly one server-built block appears in CompactPayload output (no double-prepend).
- AC-07: `transcript_delta` events stream over UDS as RecordEvent frames and merge into the F2 session buffer (existing listener path exercised end-to-end from the TS client).

Hook-set reduction:
- AC-08: Standalone PreToolUse observation retired in the TS client — non-`context_cycle` PreToolUse spawns send nothing; `cycle_start`/`phase-end`/`stop` interception fully preserved; SubagentStop install is optional (default per OQ3); `merge-settings.js` HOOK_EVENTS and matchers updated; sync-injection + PostToolUse/PostToolUseFailure + lifecycle events untouched.

Carry-items:
- AC-09: `check-hook-client-size.js` gates comment-stripped size ≤ 100 KB with a raw ≤ 160 KB backstop; the stripper is dependency-free and string-literal-safe; the gate header documents that cap changes are human decisions recorded on the feature issue.
- AC-10: The offset-file delete (FR-16) no longer fires on per-turn Stop→SessionClose; it is keyed to `TaskCompleted` and/or the existing age-prune, eliminating the full re-stream from offset 0 every turn.

## Constraints

- **Frozen F1 wire contract — additive only.** No field renames/removals; `skip_serializing_if` on new optionals; no `deny_unknown_fields`; existing parity fixtures and ts-rs bindings must pass byte-unchanged.
- **Rust hook untouched.** Zero changes to `hook.rs`/`transport.rs` behavior until F6; mixed clients against one server, no feature flag.
- **Size budget**: client sits at 99,997/100,000 bytes — AC-09 must land first; even after redefinition, additions compete for comment-stripped budget (a `format_injection` port is the biggest single risk here — see OQ2).
- **Fail-open client contract** (F3 C-05): never throws, exit 0 always, no stdout on failure paths, no secrets in stderr/breadcrumbs, every fs/network call wrapped.
- **No new npm dependencies** (F3 pure-TS architecture decision stands); UDS via Node `net` core module only.
- **UDS is Unix-only** — local mode unavailable on Windows; document, don't shim.
- **Sync-path budget**: the sync trio gains no extra file I/O.

## Open Questions

1. **Transport selection edge cases**: when remote config exists AND a local socket is live, HTTP wins (proposed) — confirm. uni-zero review recommendation: no explicit local-override key in F4a; derived-path-on-missing-remote is sufficient; F5 (#681) owns init UX and any override knob.
2. **UDS sync-response formatting**: port `format_injection` to JS (byte parity, costs comment-stripped budget) vs. add a server-side "preformatted" option for UDS sync responses. uni-zero review recommendation: **server-side preformatted** — (a) vnc-024 already moved HTTP formatting server-side; (b) one formatting implementation is parity by construction (vnc-025 ADR-005 shared-core lesson); (c) removes the largest size-budget risk; (d) a client-side formatter is dead weight at F6. Parity bar (byte-identical stdout goldens) is testable regardless. Architecture phase confirms the wire mechanism.
3. **SubagentStop "optional" semantics**: uni-zero review recommendation: **omitted-by-default with opt-in**, as a settings key (durable, user-visible) rather than a merge-settings flag — default-off is what "minimal-necessary set" means.
4. **Dogfooding switchover**: uni-zero review recommendation: **F4a** — switch this repo's `.claude/settings.json` to the TS client after the transport merges; strongest pre-F6 evidence generator. Waiting for F5 wastes the soak window.
5. **Worktree cwd dump** (ass-072 unanswered item, shared with vnc-030): does hook stdin `cwd` for worktree-isolated subagent events carry the worktree path? One stderr dump settles the live exposure rate. **Owner assigned (human decision, 2026-06-08): the vnc-030 design session runs it, first task, and cross-posts the result to #680** — this feature consumes it for socket-path/projectHash resolution.

## Tracking

GH Issue: https://github.com/dug-21/unimatrix/issues/680 (updated with design-session results 2026-06-08).

Post-merge: switch this repo's `.claude/settings.json` to the TS client — dogfooding per review OQ4.
