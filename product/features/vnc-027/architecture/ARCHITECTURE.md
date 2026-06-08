# vnc-027 Architecture — TS UDS Hook Client + Hook-Set Reduction (F4a)

GH Issue: #680. Inputs: `SCOPE.md`, `SCOPE-RISK-ASSESSMENT.md` (SR-01..SR-13).
Decisions: ADR-001..ADR-007 in this directory (all stored in Unimatrix).

## System Overview

The F3 TS hook client (`packages/unimatrix/lib/hook-client/`) is HTTP-only. This
feature adds a second transport — Unix domain socket to the local daemon — behind
the existing narrow transport seam, plus the ass-069 Q3 hook-set reduction and two
carry-items (C-04 size-gate redefinition, FR-16 offset-delete rekey).

The single load-bearing idea: **the UDS transport is an adapter onto the existing
HTTP transport contract.** `transport-uds.js` exposes the same
`post(config, frame, opts) -> Promise<SendResult>` as `transport-http.js` and maps
`HookResponse` frames to HTTP-equivalent `SendResult`s (ADR-002). Everything
downstream of the transport — `transform.js`, `state.js`, `queue.js`, `index.js`
dispatch — is byte-untouched. Sync-response formatting moves server-side for UDS
(ADR-001), eliminating the transport asymmetry recorded in Unimatrix #4798 and the
largest size-budget risk (SR-02).

The Rust hook (`crates/unimatrix-server/src/uds/hook.rs`) is the parity oracle and
is behavior-frozen until F6. Mixed clients coexist against one daemon by
construction.

## Component Breakdown

| # | Component | Status | Responsibility |
|---|-----------|--------|----------------|
| 1 | `lib/hook-client/transport-uds.js` | NEW | UDS framing (4-byte BE u32 + JSON, 1 MiB cap), connection-per-frame, FNF/sync socket lifecycle (ADR-003), HookResponse→SendResult mapping (ADR-002) |
| 2 | `lib/hook-client/config.js` | MOD | `resolve()` gains `mode: "http"\|"uds"`; no-remote-config → UDS with derived `socketPath` (ADR-002, ADR-007); the terminal `missing` breadcrumb path is retired |
| 3 | `lib/hook-client/index.js` | MOD | Selects transport once per spawn from `config.mode`; injects `post` into `queue.replay`; PreToolUse no-send sentinel handling (ADR-004); FR-16 rekey (ADR-006) |
| 4 | `crates/unimatrix-engine/src/wire.rs` | MOD (additive) | Optional `accept` field on `ContextSearch`/`CompactPayload`; new `HookResponse::Text { body }` variant (ADR-001). ts-rs bindings regenerate additively |
| 5 | `crates/unimatrix-server/src/uds/listener.rs` + `http/router/observe.rs` | MOD | `handle_connection` reads `accept` pre-dispatch; response converted via ONE shared injection-text core used by both HTTP and UDS paths (ADR-001) |
| 6 | `lib/hook-client/build-request-tools.js` | MOD | `buildPreToolUse` returns `null` sentinel for non-cycle PreToolUse (observation retired, ADR-004) |
| 7 | `lib/merge-settings.js` | MOD | `HOOK_EVENTS`/`EVENT_MATCHERS`: PreToolUse matcher narrowed to cycle tools; SubagentStop opt-in via settings key, default off (ADR-004) |
| 8 | `test/check-hook-client-size.js` | REWRITE (first merge) | C-04 gate: comment-stripped ≤ 100,000 B + raw ≤ 160,000 B backstop (ADR-005) |
| 9 | `lib/hook-client/state.js` | MOD | `deleteOffset` rekeyed to `TaskCompleted`; `pruneOffsets` (currently caller-less) wired into the FNF path (ADR-006) |
| 10 | Parity corpus (UDS layer) | NEW tests | Framing fixtures vs `wire.rs`, live-listener round-trip, sync-trio stdout goldens, hash fixtures (ADR-007), cross-transport replay (SR-10) |

## Component Interactions / Data Flow

Per-spawn pipeline (unchanged shape):

```
stdin → parseHookInput → normalize → resolveCwd
  → config.resolve(cwd)            # NEW: yields mode + socketPath OR url
  → buildRequest                   # NEW: may yield null sentinel (PreToolUse non-cycle)
  → transport = mode === "uds" ? transportUds : transportHttp   # selected once
  → FNF:  queue.prune → queue.replay(config, transport.post) → carrying post (+ delta post)
  → sync: transport.post(config, frame, {sync:true}) → transform.writeSyncOutput
```

UDS sync wire exchange (ADR-001 + ADR-003):

```
client                                  daemon (listener.rs)
------                                  --------------------
connect(socketPath)
end(frame{...,"accept":"text/plain"})   read_exact(4) → len → read_exact(len)
  (write + FIN; flush guaranteed)       dispatch_request → HookResponse
read 4-byte header + body  ◄──────────  Entries→ injection core → Text{body} (or Ack if empty)
destroy(); resolve SendResult           BriefingContent → Text{body: content}
```

FNF: `connect → end(frame) → resolve on flush; no read; server EPIPE on Ack write
is expected and already DEBUG-classified (#3448)`.

Replay is connection-per-frame on both clients (Rust `fire_and_forget` disconnects
per frame; the listener handles exactly one frame per connection).

## Technology Decisions

| Decision | ADR |
|----------|-----|
| UDS sync responses server-side preformatted: additive `accept` request field + `HookResponse::Text`; no `format_injection` JS port | ADR-001 |
| `transport-uds.js` adapts to the existing SendResult contract; transport selection = remote config wins, else UDS; no override knob in F4a (OQ1 confirmed) | ADR-002 |
| Node socket flush/drain/exit lifecycle contract (SR-01/SR-05) | ADR-003 |
| PreToolUse: narrowed matcher + client no-send sentinel; SubagentStop opt-in settings key, default off (OQ3 confirmed) | ADR-004 |
| C-04 size gate: stripped ≤ 100,000 B, raw ≤ 160,000 B, dependency-free state-machine stripper | ADR-005 |
| FR-16 offset delete keyed to `TaskCompleted` + wired age-prune | ADR-006 |
| Socket path derived from existing `config.js` projectHash; SR-12 settled empirically | ADR-007 |

No new npm dependencies (`net`, `crypto`, `fs`, `path` core modules only). UDS is
Unix-only; Windows remains HTTP-remote (documented, not shimmed).

## Parity Bar (SR-04, SR-06 — binding definition)

**Full parity (byte-level, tested):**
- Frame encoding: 4-byte BE u32 + JSON, 1 MiB cap, zero-length reject — against committed `wire.rs`-generated fixtures (AC-01).
- HookRequest JSON bodies for all NON-retired events (extends F3 corpus, vnc-026 ADR-001 oracle/golden/drift mechanism).
- Sync-trio stdout: ContextSearch (plain + SubagentStart envelope), CompactPayload, Ping — goldens vs Rust hook stdout. The `--- Unimatrix Context ---\n` header is a load-bearing wire contract (vnc-024 ADR-003 amendment); `Text` bodies carry it unchanged.
- Fail-open behavior: exit 0 always, no stdout on failure, enqueue only on connect failure, sync silent-drop when daemon absent.

**Accepted divergences (enumerated — the corpus must except exactly these):**
1. **Lone-surrogate stdin** (Unimatrix #4788, pre-existing, open): Node accepts `\uD800` escapes serde rejects. Inherited; remains a tracked `node:test` todo, not a UDS regression.
2. **No bare probe connection**: the Rust hook's connect→replay→disconnect→reconnect produces an empty first connection (the #3448 EOF noise); the TS client posts connection-per-frame with no empty probe. Process-level only; wire frames and observable behavior identical.
3. **PreCompact block source**: Rust prepends client-side from the transcript file; TS never does (F3 design) — the server builds the block from the F2 buffer. Byte-identical CompactPayload stdout only when deltas were streamed (AC-06). Mixed-client matrix below.
4. **Event-set divergence by design**: retired PreToolUse observation and default-off SubagentStop (ADR-004). Transport parity is full; event-set parity is explicitly not a goal. The parity corpus excludes retired events.

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|----------------|--------|
| Transport contract | `post(config, frame, opts) -> Promise<SendResult>`; `SendResult = { ok: boolean, status: number, contentType: string\|null, body: Buffer\|null, failureClass: null\|"auth"\|"connect"\|"timeout"\|"http_4xx"\|"http_5xx" }`; never rejects | `lib/hook-client/transport-http.js:5-7` (oracle for transport-uds.js) |
| Framing | `write_frame(w, payload)` / `read_frame(r, max)`: 4-byte BE u32 length + payload; `MAX_PAYLOAD_SIZE = 1_048_576`; zero-length reject | `crates/unimatrix-engine/src/wire.rs:16,349,372` |
| Socket path | `~/.unimatrix/{projectHash}/unimatrix.sock`; `projectHash = sha256(projectRoot).hex[..16]` | `uds/hook.rs:180-184`; `config.js::computeProjectHash` (oracle: `project.rs::compute_project_hash`) |
| NEW request fields | `ContextSearch { .., accept: Option<String> }`, `CompactPayload { .., accept: Option<String> }` — `#[serde(default, skip_serializing_if = "Option::is_none")]`; set ONLY by transport-uds at serialization time, value `"text/plain"` | ADR-001; `wire.rs:139,165` |
| NEW response variant | `HookResponse::Text { body: String }` (serde tag `"type"`); body = exact HTTP text/plain body (header-prefixed for Entries) | ADR-001; `wire.rs:193` |
| Injection formatting truth | `format_injection(&[EntryPayload], MAX_INJECTION_BYTES=1400) -> Option<String>` — single implementation; shared injection-text core consumed by `observe_response_to_http` AND the new UDS branch | `uds/hook.rs:1034,30`; `http/router/observe.rs:25` |
| SendResult mapping (UDS) | `Text{body}` → `{ok:true,200,"text/plain",Buffer(body)}`; `Ack` (sync, empty injection) → `{ok:true,204,null,null}`; `Ack` impossible on FNF (no read) — FNF flush success → `{ok:true,0,null,null}`; `Pong` → `{ok:true,200,"application/json",Buffer(json)}`; `Error{code}` → `{ok:false,code, failureClass: code>=500?"http_5xx":"http_4xx"}`; connect err → `"connect"`; deadline → `"timeout"` | ADR-002 |
| stdout writer | `transform.writeSyncOutput(reqSource, res)` — stdout iff `ok && status===200 && text/plain && body.length>0`; SubagentStart envelope dispatch on `INJECTION_HEADER = "--- Unimatrix Context ---\n"` | `lib/hook-client/transform.js:30,74` |
| Queue replay seam | `queue.replay(config, post)` — transport-injected; frames stored WITHOUT `accept` (transport adds it), so the queue stays transport-agnostic | `lib/hook-client/queue.js`; `index.js:249` |
| HTTP ingest session rewrite | `prefix_session_id` prepends `"http-"` to all session ids on /observe ingest; UDS does not | `http/router/observe.rs` (SR-10 consequence, see below) |
| Hook install surface | `HOOK_EVENTS`, `EVENT_MATCHERS` in `lib/merge-settings.js:29-52`; SubagentStop opt-in key `unimatrix.hooks.subagent_stop` in `{root}/.claude/settings.local.json` | ADR-004 |
| Cycle event constants | `CYCLE_START_EVENT`/`CYCLE_PHASE_END_EVENT`/`CYCLE_STOP_EVENT` frames from `buildCycleEventOrFallthrough`; exact tool-name equality gate (`context_cycle`, `mcp__unimatrix__context_cycle`) preserved | `build-request-tools.js:314-398` |
| Timeouts | UDS parity constants 40 ms (oracle `HOOK_TIMEOUT`, `uds/hook.rs:27`); HTTP keeps ADR-005(F3) defaults 750/2000/3000 | ADR-002 |

## Cross-Transport Replay (SR-10)

Both ingest points deserialize the same `HookRequest` serde enum: UDS
`listener.rs:469` and HTTP `/observe`. Queued frames carry no transport state and
no `accept` field, so replay over either transport is accepted by construction.
Two consequences are accepted and documented:

1. **Session-id split**: HTTP ingest rewrites ids to `http-{sid}`; UDS keeps raw
   ids. Frames enqueued under one transport and replayed under the other land in a
   differently-named session. No rejection, no data loss; attribution splits.
   Transport flips mid-project require a config change and are rare — accepted.
2. **Auth asymmetry**: UDS uses peer-credential auth (no token); HTTP replay uses
   the freshly resolved config token. A frame queued under UDS replays over HTTP
   only when remote config has since appeared — the same config supplies the token.

Spec must add the cross-transport replay AC (both directions, live listener +
stubbed HTTP).

## Mixed-Client PreCompact Matrix (SR-11)

| Client that streamed deltas | Client firing PreCompact | Result |
|---|---|---|
| TS | TS | One server-built block (AC-06) — correct |
| none (Rust only) | Rust | Client-prepended block; server's empty-buffer guard holds — correct |
| TS | Rust (mixed install) | Client prepend + server block = **double block** |

Stated assumption (binding, documented in spec): one project uses one client.
Mixed installs are not a supported configuration; F5's installer makes the
selection explicit. No code mitigation in F4a (the Rust hook is frozen).

## No-Daemon UX (SR-13)

No remote config + no daemon: FNF frames enqueue (stderr one-liner per AC-04);
queue bounds already cap exposure — 24 h age prune + drop-oldest eviction
(`queue.js MAX_AGE_MS`, `MAX_FILES`, `MAX_TOTAL_BYTES`). Sync paths are silent.
The former `missing`-config breadcrumb is retired by design; `state.js` send-outcome
breadcrumbs (failureClass `connect`, queueDepth) remain the observable signal.
F5 owns making "no daemon, no remote" loud at init time.

## Dogfooding Switchover + Drop Detector (SR-07, OQ4)

Post-merge, this repo's `.claude/settings.json` switches to the TS client (uni-zero
OQ4: F4a, not F5). Drop detector — zero new code:

- `~/.unimatrix/0d62f3bf1bf46a0a/hook-client/state.json` breadcrumbs (`recordSendOutcomes`: failureClass, queueDepth) checked daily during the soak.
- Queue residue: `hook-client/queue/` should be empty while the daemon runs.
- Server-side: daily event counts in `unimatrix.db` compared to the pre-switchover baseline.

**Rollback trigger**: any sustained `connect` breadcrumbs or queueDepth growth
while the daemon is up, or a >50% day-over-day event-count drop → revert
`settings.json` to the Rust hook (one-line change; no daemon impact).

## Merge Sequencing (SR-02)

1. ADR-005 size gate rewrite (`check-hook-client-size.js`) — FIRST; the client is at 99,997/100,000 bytes.
2. Wire contract additions (`wire.rs` + listener/observe shared core) — server side, independently testable.
3. `transport-uds.js` + `config.js` mode + `index.js` selection + parity corpus UDS layer.
4. Hook-set reduction (`build-request-tools.js`, `merge-settings.js`) + FR-16 rekey.
5. Dogfood switchover (post-merge, with drop detector).

## Open Questions (for spec / human)

1. **OQ5 residual**: hash parity for worktrees is settled empirically (ADR-007) — TS and Rust resolve worktree cwds to the main-repo root and identical hashes. The literal stderr dump of live SubagentStop `cwd` content was not capturable in a design session; the hash-fixture corpus makes the answer immaterial. Spec should still capture one live dump during the dogfood soak as confirmation (zero cost: stderr is already visible in hook debug output).
2. **SubagentStop opt-in key name**: `unimatrix.hooks.subagent_stop` proposed (snake_case matches `unimatrix.remote.*` keys). Spec confirms; F5 owns any UX around it (SR-09).
3. **Lone-surrogate divergence** (#4788): architecture keeps it as a formally excepted divergence. If the human prefers scoping the fix into F4a, it is independent of the transport work.
