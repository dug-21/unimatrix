# vnc-027 Implementation Brief — TS UDS Hook Client + Hook-Set Reduction (F4a)

GH Issue: [#680](https://github.com/dug-21/unimatrix/issues/680). Delivery order pinned: **vnc-027 → vnc-030 → crt-052**.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-027/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-027/SCOPE-RISK-ASSESSMENT.md |
| Architecture | product/features/vnc-027/architecture/ARCHITECTURE.md |
| Specification | product/features/vnc-027/specification/SPECIFICATION.md |
| Risk Strategy | product/features/vnc-027/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-027/ALIGNMENT-REPORT.md |
| Acceptance Map | product/features/vnc-027/ACCEPTANCE-MAP.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| size-gate (`test/check-hook-client-size.js` rewrite) | pseudocode/size-gate.md | test-plan/size-gate.md |
| wire-accept-text (`wire.rs` additive: `accept` field + `HookResponse::Text`) | pseudocode/wire-accept-text.md | test-plan/wire-accept-text.md |
| listener-preformatted (`listener.rs` + `observe.rs` shared injection core) | pseudocode/listener-preformatted.md | test-plan/listener-preformatted.md |
| transport-uds (`lib/hook-client/transport-uds.js`, NEW) | pseudocode/transport-uds.md | test-plan/transport-uds.md |
| config-transport-selection (`lib/hook-client/config.js`) | pseudocode/config-transport-selection.md | test-plan/config-transport-selection.md |
| index-dispatch (`lib/hook-client/index.js`) | pseudocode/index-dispatch.md | test-plan/index-dispatch.md |
| build-request-sentinel (`lib/hook-client/build-request-tools.js`) | pseudocode/build-request-sentinel.md | test-plan/build-request-sentinel.md |
| merge-settings-reduction (`lib/merge-settings.js`) | pseudocode/merge-settings-reduction.md | test-plan/merge-settings-reduction.md |
| state-offset-rekey (`lib/hook-client/state.js`) | pseudocode/state-offset-rekey.md | test-plan/state-offset-rekey.md |
| parity-corpus-uds (UDS layer: framing/hash fixtures, round-trip, stdout goldens, cross-transport replay) | pseudocode/parity-corpus-uds.md | test-plan/parity-corpus-uds.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

Note: pseudocode and test-plan files are produced in Session 2 Stage 3a; the map lists expected components from the architecture.

## Goal

Add a local UDS transport to the F3 TS hook client (`packages/unimatrix/lib/hook-client/`) with full transport/framing parity to the Rust hook, so local users no longer depend on the compiled `unimatrix hook` binary — unblocking the dogfooding switchover and F6 (hook.rs retirement). Simultaneously reduce the registered hook set to the minimal-necessary set (retire standalone PreToolUse observation, make SubagentStop opt-in) and land two carry-items: the C-04 size-gate redefinition and the FR-16 offset-delete rekey.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| UDS sync-response formatting (OQ2) | Server-side preformatted: additive `accept: Option<String>` on `ContextSearch`/`CompactPayload` + new `HookResponse::Text { body }`; ONE shared injection-text core for HTTP and UDS; no `format_injection` JS port. `Text` returned ONLY to callers that sent `accept` (frozen-hook safety coupling). | SCOPE OQ2, SR-02/SR-03 | architecture/ADR-001-uds-sync-preformatted.md |
| Transport adapter + selection (OQ1) | `transport-uds.js` honors the exact `post(config, frame, opts) -> Promise<SendResult>` contract; normative HookResponse→SendResult mapping table (ADR-002 §2); remote config → HTTP unconditionally, else UDS; no override knob (F5 owns it); 40 ms fixed parity timeouts; connection-per-frame, no probe connection. | SCOPE OQ1, SR-10 | architecture/ADR-002-transport-adapter-sendresult.md |
| Node socket lifecycle | Flush-before-FIN: FNF = `socket.end(frame)`, resolve on `'finish'`, never `destroy()` unflushed; sync = half-close + accumulate read loop to declared length; settle-once, all timers `unref()`d, no `process.exit()` anywhere; enqueue only on `ok:false` (at-least-once, duplicates accepted). | SR-01, SR-05 | architecture/ADR-003-node-socket-lifecycle.md |
| Hook-set reduction (OQ3) | Two-level PreToolUse reduction: matcher narrowed to `context_cycle\|mcp__unimatrix__context_cycle` at install level + `null` no-send sentinel at client level (F-02 exact-equality gate preserved as defense-in-depth). SubagentStop opt-in via `unimatrix.hooks.subagent_stop: true` in settings.local.json, default off. **Amended**: SubagentStop server-side independence stated with code evidence (listener.rs:2919 all-None fallthrough). | SCOPE OQ3, ass-069 Q3, SR-06/SR-09, R-12 | architecture/ADR-004-hook-set-reduction.md |
| C-04 size gate | Comment-stripped ≤ 100,000 B primary + raw ≤ 160,000 B backstop (decimal); dependency-free character-level state-machine stripper with embedded self-test corpus run on every invocation; header documents cap changes are human decisions on the feature issue. **Merges as the literal FIRST commit.** | C-04 human decision 2026-06-08, SR-02 | architecture/ADR-005-size-gate-c04.md |
| FR-16 offset-delete rekey | **Amended (authoritative over spec FR-30/AC-10 "and/or" wording)**: SessionClose delete removed; age-prune (7-day `pruneOffsets`, newly wired on the FNF path) is the sole effective mechanism. TaskCompleted branch retained as a zero-cost forward provision keyed by **canonical event name, never frame type** (Stop and TaskCompleted both build SessionClose frames) — unreachable under current registrations, pinned by unit test, NOT registered in HOOK_EVENTS. | #680 carry-item, SR-08, R-04 | architecture/ADR-006-offset-delete-rekey.md |
| Socket path derivation | Single derivation: `socketPath` from the SAME `walkToProjectRoot` + `computeProjectHash` as `stateDir`. SR-12 settled empirically (main/worktree/deep-subdir all = `0d62f3bf1bf46a0a`, matching live daemon). Rust-generated hash-fixture corpus binding (5 healthy layouts + 1 pinned corrupt-worktree divergence). | SR-12, OQ5, #679 | architecture/ADR-007-projecthash-socket-path.md |

**ADR-004 and ADR-006 carry post-risk-review amendments and are authoritative where spec text is looser** (notably FR-30/AC-10: delivery implements age-prune-only per ADR-006, not TaskCompleted-primary keying).

## Merge Sequencing (binding — SR-02 / R-02 Critical)

1. **size-gate rewrite — LITERAL FIRST COMMIT.** The client sits at 99,997/100,000 raw bytes (3 bytes of headroom); any client byte added before the gate rewrite fails CI deterministically. **vnc-030 also depends on this redefinition** (cross-feature contract, #680 comment). Merge order must be auditable in git history.
2. Wire contract additions (`wire.rs` + listener/observe shared core) — server side, independently testable.
3. `transport-uds.js` + `config.js` mode + `index.js` selection + parity-corpus UDS layer.
4. Hook-set reduction (`build-request-tools.js`, `merge-settings.js`) + FR-16 rekey.
5. Dogfood switchover (post-merge, with drop detector).

## Files to Create/Modify

| File | Change |
|------|--------|
| `test/check-hook-client-size.js` | REWRITE (first commit): C-04 gate, state-machine stripper + embedded self-test (stripper does NOT count against client budget) |
| `crates/unimatrix-engine/src/wire.rs` | MOD additive: `accept: Option<String>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) on `ContextSearch`/`CompactPayload`; `HookResponse::Text { body: String }`; ts-rs bindings regenerate additively |
| `crates/unimatrix-server/src/uds/listener.rs` | MOD: `handle_connection` extracts `wants_text` pre-dispatch; converts `Entries`/`BriefingContent` responses post-dispatch via the shared injection core; allowlist hard contract (`Ack`/`Error`/`Pong` always JSON) |
| `crates/unimatrix-server/src/http/router/observe.rs` | MOD: response→text mapping factored into ONE `pub(crate)` shared core consumed by both HTTP and UDS paths |
| `crates/unimatrix-server/src/uds/hook.rs` | MOD mechanical only: compiler-forced `accept: None` at `ContextSearch`/`CompactPayload` construction sites — **approved variance, no other change permitted**, AC-11 proves byte-unchanged |
| `packages/unimatrix/lib/hook-client/transport-uds.js` | NEW: framing (4-byte BE u32 + JSON, 1 MiB caps), connection-per-frame, ADR-003 lifecycle, ADR-002 SendResult mapping; injects `accept: "text/plain"` at serialization time for sync injection-bearing frames |
| `packages/unimatrix/lib/hook-client/config.js` | MOD: `resolve()` gains `mode: "http"\|"uds"` + derived `socketPath`; terminal `missing` breadcrumb path retired; `partial_env`/`malformed` stay terminal |
| `packages/unimatrix/lib/hook-client/index.js` | MOD: transport selected once per spawn from `config.mode`, injected into `queue.replay`; `null` request → immediate return before transport selection; SessionClose delete removed; canonical-event flag to `runFireAndForget`; `pruneOffsets` wired alongside `queue.prune` |
| `packages/unimatrix/lib/hook-client/build-request-tools.js` | MOD: `buildCycleEventOrFallthrough` returns `null` sentinel on every non-cycle path (stderr lines retained); fallthrough RecordEvent observation removed for PreToolUse only |
| `packages/unimatrix/lib/merge-settings.js` | MOD: PreToolUse matcher narrowed; SubagentStop registered only when opt-in key set; all other HOOK_EVENTS/matchers unchanged |
| `packages/unimatrix/lib/hook-client/state.js` | MOD: `deleteOffset` keyed to canonical `TaskCompleted`; `pruneOffsets` (currently caller-less) goes live |
| Parity corpus (test files) | NEW UDS layer: Rust-generated framing fixtures, live-listener round-trip, sync-trio stdout goldens (plain + SubagentStart envelope), hash-fixture corpus, cross-transport replay, FNF truncation assertions |

Untouched by contract: `transform.js`, `queue.js`, `delta.js` (byte-untouched); `transport-http.js` (contract oracle); Rust `transport.rs`.

## Data Structures

- **Wire frame**: 4-byte BE u32 length prefix + JSON payload; `MAX_PAYLOAD_SIZE = 1_048_576`; zero-length and oversized declared lengths rejected on read **before allocating** (`wire.rs:16,349,372` is the byte authority).
- **SendResult**: `{ ok: boolean, status: number, contentType: string|null, body: Buffer|null, failureClass: null|"auth"|"connect"|"timeout"|"http_4xx"|"http_5xx" }` — no new failureClass values; non-HTTP interpretations (FNF success → status 0; `http_4xx` = generic client-side reject) per ADR-002 mapping table (normative).
- **NEW request fields**: `ContextSearch { .., accept: Option<String> }`, `CompactPayload { .., accept: Option<String> }` — set ONLY by transport-uds at serialization time (value `"text/plain"`); builders and queued frames never carry it.
- **NEW response variant**: `HookResponse::Text { body: String }` (serde tag `"type"`); body = exact HTTP text/plain bytes (`--- Unimatrix Context ---\n`-prefixed for Entries; BriefingContent `content` verbatim). Empty injection → existing `Ack` (204-equivalent).
- **Socket path**: `~/.unimatrix/{projectHash}/unimatrix.sock`; `projectHash = sha256(projectRoot).hex[..16]`.
- **Opt-in key**: `unimatrix.hooks.subagent_stop: true` in `{root}/.claude/settings.local.json`; non-boolean values treated as unset.

## Function Signatures

- `transport-uds.js`: `post(config, frame, opts) -> Promise<SendResult>` — never rejects, no stdout/stderr, no retry (identical contract to `transport-http.js:5-7`).
- `config.js`: `resolve(cwd)` → adds `mode: "http"|"uds"` and (UDS) `socketPath`.
- `build-request-tools.js`: `buildCycleEventOrFallthrough(...)` → cycle frame | `null` sentinel.
- `state.js`: `pruneOffsets(stateDir)` — 7-day cutoff, fail-open; `deleteOffset` fires only when carrying send succeeds AND canonical event is `TaskCompleted`.
- Server shared core: `pub(crate)` response→injection-text fn consumed by `observe_response_to_http` and the new UDS branch; `format_injection(&[EntryPayload], MAX_INJECTION_BYTES=1400) -> Option<String>` remains the single formatting truth.
- `transform.writeSyncOutput(reqSource, res)` — unchanged; stdout iff `ok && status===200 && text/plain && body.length>0`; envelope dispatch on `INJECTION_HEADER`.

## Constraints

1. **Frozen F1 wire contract — additive only**: `skip_serializing_if` optionals, no renames/removals, no `deny_unknown_fields`; AC-11 enforces (all pre-existing Rust parity fixtures + ts-rs bindings pass byte-unchanged).
2. **Rust hook behavior-frozen**: zero behavioral changes to `hook.rs`/`transport.rs` until F6. The mechanical `accept: None` construction-site edits are an **approved variance** (see Alignment Status) — nothing else.
3. **Size budget**: AC-09 first; every subsequent addition measured against the comment-stripped budget (NFR-2).
4. **Fail-open (NFR-3)**: never throws to host, exit 0 always, no stdout on failure, no secrets in stderr/breadcrumbs, every fs/network call wrapped — including the UDS read loop and exit sequencing.
5. **No new npm dependencies**: Node `net` core module only. **UDS is Unix-only** — document, don't shim.
6. **Sync-path I/O budget (NFR-4)**: sync trio gains no extra file I/O (`pruneOffsets` runs on the FNF path only).
7. **Socket lifecycle is a contract (§7.7)**: flush/drain-before-close for FNF; full-frame read + stdout flush before exit for sync; server-side truncation detectable in tests.
8. **Latency (NFR-1)**: < 20 ms p95 per invocation incl. project-root detection (F3 AC-13 protocol); 40 ms fixed timeout constants.
9. **Parity bar split (FR-21, binding)**: transport/framing parity full; event-set parity explicitly NOT a goal; corpus = post-reduction event set only. Accepted divergences enumerated (ARCHITECTURE.md): lone-surrogate (FR-22, #4788), no bare probe connection, PreCompact block source, event-set divergence by design. New divergences are fixed or registered by explicit decision — never silently tolerated.

## Dependencies

- F3 TS client (`packages/unimatrix/lib/hook-client/`): transport contract, config, queue, delta, transform, index dispatch, parity corpus + drift mechanism (vnc-026 ADR-001).
- Rust oracle (read-only): `wire.rs:345-400`, `transport.rs` LocalTransport, `hook.rs:174-321` + `format_injection` (hook.rs:963-1034).
- UDS listener `transcript_delta` accept path (`listener.rs:785,1025`); gains the preformatted-response option (additive).
- `merge-settings.js` HOOK_EVENTS; `test/check-hook-client-size.js`.
- #679 worktree-root resolution (fresh, load-bearing for FR-15/hash fixtures).
- Node core modules only: `net` + existing `fs`/`path`/`crypto`.

## NOT in Scope

- Contractual cycle attribution (tracker, `cycle_stamp`, precedence chain, `topic_source`, heuristic demotion) — vnc-030 (F4b, #699).
- Rust `hook.rs` retirement or any behavioral change — F6.
- `init` unification / installer flow, transport-override knobs, SubagentStop opt-in UX — F5 (#681). F4a defines the settings key only.
- Client-side PreCompact transcript prepend over UDS — prohibited by design.
- Windows local mode — remote HTTP is the Windows path.
- Distillation / buffer consumers — crt-052 (#689).
- Lone-surrogate divergence fix — tracked todo, formally excepted (FR-22).
- Mixed-client PreCompact double-prepend mitigation — documented one-client-per-project assumption only.
- Event-set parity with the Rust hook — explicitly not a goal (FR-21).

## Alignment Status (from ALIGNMENT-REPORT.md)

Counts: 4 PASS, 1 WARN, 1 VARIANCE, 0 FAIL. Vision alignment PASS (direct `personal-cloud` advance); milestone fit PASS; scope gaps PASS; risk completeness PASS.

- **VARIANCE (requires human approval — record acceptance on #680)**: ADR-001 forces mechanical `accept: None` edits at `hook.rs` construction sites, contradicting SCOPE Non-Goals' "any change to it" (the Constraints section says "behavior" only). Guardian recommendation: **Accept** — compiler-forced by the SCOPE-endorsed OQ2 resolution, non-behavioral by construction (`skip_serializing_if`), proven by AC-11 byte-unchanged fixtures plus an end-to-end run of the real frozen Rust binary (R-08 scenario 4). Recording the acceptance on #680 gives F6 a clean "hook.rs behavior-frozen" claim.
- **WARN (delivery rule)**: spec FR-30/AC-10 retain "TaskCompleted and/or age-prune"; amended ADR-006 decided **age-prune-only** with an unreachable-but-tested TaskCompleted branch. **ADR-006 is authoritative for FR-30/AC-10** — do not implement TaskCompleted-primary keying or register TaskCompleted in HOOK_EVENTS.

Risk-sanctioned scope additions (no approval needed): AC-11/AC-12 (SR-03/SR-08 recommendations); `pruneOffsets` wired live (within FR-16 intent, guarded by R-14 scenario 2).

## Post-Merge Obligations (not F4a gate items)

1. **Dogfooding switchover (FR-32)**: switch this repo's `.claude/settings.json` to the TS client with the drop detector active from day one — daily `state.json` breadcrumb check (failureClass `connect`, queueDepth), queue-residue emptiness, daemon event counts vs pre-switch baseline; rollback trigger = sustained `connect` breadcrumbs, queueDepth growth, or >50% day-over-day event-count drop → one-line settings revert.
2. **UDS-path stamp regression test owed to vnc-030 (#699)** after F4a merges.
3. **OQ5 residual**: capture one live SubagentStop/SubagentStart stderr `cwd` dump from a worktree-isolated subagent during the soak (zero-cost confirmation; hash fixtures already make the answer immaterial).
