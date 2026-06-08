# Risk-Based Test Strategy: vnc-027

TS UDS hook client + hook-set reduction (F4a). GH Issue #680.
Inputs: SCOPE.md, ARCHITECTURE.md, ADR-001..ADR-007, SPECIFICATION.md, SCOPE-RISK-ASSESSMENT.md (SR-01..SR-13), uni-zero re-review residual notes, vnc-030 cross-feature notice (#680 comment).

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | FNF frame loss: Node async write buffer + destroy-before-flush silently drops frames the Rust `write_all` path never loses; fail-open masks the loss | High | Med | High |
| R-02 | Size-gate merge-order violation: any client byte added before the AC-09 gate rewrite fails CI deterministically (3-byte headroom); vnc-030's client additions also depend on the redefinition | High | High | Critical |
| R-03 | Comment-stripper state-machine miscount (regex vs division, `//` in strings, template-literal nesting) admits oversize code or blocks valid code | Med | Med | Medium |
| R-04 | AC-10 TaskCompleted keying is unreachable: `TaskCompleted` is normalized (normalize.js:43) and built (build-request.js:60 — as a **SessionClose** frame, not the RecordEvent ADR-006 claims) but is **not in `HOOK_EVENTS`** and not registered in `.claude/settings.json`. The delete branch is dead code; effective deletion falls to the 7-day age-prune only | Med | High | High |
| R-05 | TS/Rust projectHash divergence in residual layouts (symlinked root, non-git fallback, corrupt-worktree fallback — a documented benign divergence) → silent local-mode failure, enqueue-forever | High | Low | Medium |
| R-06 | Sync read-loop failure: partial frame reads, `'end'` before declared length satisfied, premature event-loop drain → truncated stdout, missing injection, or hang past budget | High | Med | High |
| R-07 | Wire-contract additivity slip: `accept` field / `HookResponse::Text` variant changes serialized bytes of existing frames, breaking F1 parity goldens or ts-rs bindings | High | Low | Medium |
| R-08 | `Text` frame returned to a caller that did not send `accept` — the frozen Rust hook cannot deserialize the new variant; ADR-001's accept↔Text coupling is the only protection | High | Low | Medium |
| R-09 | SubagentStart envelope dispatch over UDS: a `Text` body missing or mangling the `--- Unimatrix Context ---\n` header breaks transform.js discrimination — this exact bug class shipped once (entries #4778/#4783) | Med | Med | Medium |
| R-10 | Cross-transport replay: HTTP `http-{sid}` session-id rewrite vs UDS raw ids splits attribution; auth asymmetry (peer-cred vs token) could reject replayed frames at one ingest point | Med | Med | Medium |
| R-11 | Cycle interception regression: narrowed PreToolUse matcher makes Claude Code regex-matcher semantics load-bearing; matcher or sentinel bug silently kills `cycle_start`/`phase-end`/`stop` or weakens the F-02 exact-equality gate | High | Med | High |
| R-12 | SubagentStop default-off: server-side lifecycle dependency unverified in ADR-004 (code evidence: listener.rs:2919 is an all-None fallthrough — but unasserted) | Med | Low | Low |
| R-13 | No-daemon enqueue-forever unbounded growth (SR-13 / spec-writer flag): code evidence says bounds apply at enqueue time (queue.js MAX_FILES=500, MAX_TOTAL_BYTES=5 MiB, MAX_AGE_MS=24 h, drop-oldest) — but unpinned for this path | Med | Low | Low |
| R-14 | FR-30/FR-31 regress the HTTP delta path: delete-timing change plus newly wired `pruneOffsets` (currently dead code, now live on every FNF spawn) alters F3 remote behavior | Med | Med | Medium |
| R-15 | Latency budget breach: flush-wait FNF (`'finish'` round-trip), per-frame connect, and project-root walk push p95 ≥ 20 ms | Med | Low | Low |
| R-16 | Dogfood switchover silent event loss for the soak window — fail-open by design means a drop bug costs this repo's knowledge capture before detection (entry #4473: warn-continue masks failure paths) | Med | Med | Medium |
| R-17 | Mixed-client PreCompact double-prepend (TS streamed deltas, Rust hook fires PreCompact) | Med | Low | Low |
| R-18 | 1 MiB frame-cap handling: oversized payload build, oversized/zero declared read length, or pre-allocation on a hostile length prefix | Med | Med | Medium |

## Risk-to-Scenario Mapping

### R-01: FNF frame loss via Node socket buffering
**Severity**: High · **Likelihood**: Med
**Impact**: Events vanish with exit 0 and no signal; knowledge capture silently degrades (precisely the failure class #4473 warns about).

**Test Scenarios**:
1. Live-listener FNF of a max-size (1 MiB) frame; assert the daemon recorded the event completely (ADR-003 §6 truncation contract).
2. Kill-the-client-mid-write case: assert either full delivery or a clean server-side frame error — never a silently truncated accepted event.
3. Unit: `destroy()` is never invoked before `'finish'` on the FNF path (instrument the socket; assert call order).
4. Post-connect flush timeout resolves `ok:false` → frame enqueued (at-least-once, duplicate accepted), never dropped.
5. Server EPIPE on Ack write to FIN'd socket stays DEBUG-classified (#3448) — assert no WARN-level noise from a normal TS FNF.

**Coverage Requirement**: Every FNF resolution path (success, connect-fail, flush-timeout, deadline) proven to end in exactly one of {delivered-complete, enqueued}; no third state.

### R-02: Size-gate merge ordering (Critical)
**Severity**: High · **Likelihood**: High
**Impact**: First client addition fails CI; worse, pressure to "trim 3 bytes" reproduces the vnc-026 Gate-3b rework (#4780). vnc-030 is blocked on the redefinition — AC-09 must be the literal first commit (cross-feature contract, #680 comment).

**Test Scenarios**:
1. Gate self-test corpus green (FR-2 string-literal cases: `"// not a comment"`, template literal with `${}` and backticks, regex containing `//` and `/*`, division-vs-regex ambiguity) — runs on every gate invocation.
2. Gate fails non-zero with per-file table when stripped > 100,000 B or raw > 160,000 B (synthetic fixture).
3. Process check at Gate review: `git log` shows the gate rewrite as the first vnc-027 commit touching the client tree, before any `lib/hook-client/` growth.

**Coverage Requirement**: Both limits independently triggerable in tests; merge order auditable in history; header documents the human-decision rule.

### R-03: Stripper correctness
**Severity**: Med · **Likelihood**: Med
**Impact**: Miscount admits unbounded stripped growth (capped only by the 160 KB raw backstop) or falsely blocks merges.

**Test Scenarios**:
1. Embedded self-test corpus (ADR-005 §4) — failure of the self-test fails the gate itself.
2. Differential check: stripped size of the current real client tree is strictly less than raw and greater than zero; stripping is removal-only (output is a byte-subsequence of input).
3. Escape-sequence cases: `\"` inside strings, `\``  inside templates, `\/` and `[/]` inside regex character classes.

**Coverage Requirement**: All six lexer states + transitions exercised; the regex-open heuristic (prev-token rule) covered for at least `(`, `=`, `return`, and identifier-prev (division) cases.

### R-04: TaskCompleted keying unreachable (verification finding — needs spec/human resolution)
**Severity**: Med · **Likelihood**: High
**Impact**: AC-10's TaskCompleted branch ships as dead code: the host never spawns the hook with `TaskCompleted` because it is not in `merge-settings.js HOOK_EVENTS` nor this repo's settings.json. Offset deletion degrades to age-prune-only (7 days) — functionally acceptable (the per-turn re-stream is still fixed by removing the SessionClose delete), but the AC's primary keying is untestable end-to-end, and ADR-006's frame-type claim is wrong (TaskCompleted → SessionClose frame, build-request.js:60).

**Test Scenarios**:
1. Unit: a spawn whose canonical event is `TaskCompleted` deletes the offset file after a successful carrying send (proves the branch works if the event ever arrives).
2. Assertable negative (ADR-006): a `Stop` spawn (also a SessionClose frame) does **not** delete the offset — the keying must discriminate by canonical event, not frame type.
3. Multi-turn integration: offsets persist across N Stop turns; delta sends after turn 1 are true deltas (no re-stream from 0).
4. `pruneOffsets` deletes only files older than 7 days; a mid-session prune degrades to one full re-stream (idempotent merge) — assert no error path.
5. **Decision scenario (blocking for delivery planning)**: either (a) `TaskCompleted` is added to `HOOK_EVENTS` (scope addition — not in the architecture; needs explicit decision), or (b) the spec records age-prune as the sole effective mechanism and the AC-10 "and/or" resolves to age-prune. Silent shipping of an unreachable branch is not acceptable (FR-22's never-silently-tolerate rule, applied here by analogy).

**Coverage Requirement**: The delete trigger discriminates Stop vs TaskCompleted; the dead-registration question resolved by explicit decision before Gate 3.

### R-05: projectHash residual divergence
**Severity**: High · **Likelihood**: Low (post ADR-007 empirical settlement: main, worktree, deep-subdir all = `0d62f3bf1bf46a0a`, matching the live daemon)
**Impact**: Client enqueues forever against a socket path that never exists; fail-open hides it entirely.

**Test Scenarios**:
1. Hash-fixture corpus (ADR-007 §3, Rust-generated goldens through the vnc-026 drift mechanism): main root, deep subdir, linked worktree, symlinked repo path, non-git dir — TS `walkToProjectRoot` + `computeProjectHash` replay must match.
2. Corrupt-worktree fixture (dangling `gitdir:`): assert the documented divergence is exactly as enumerated (Rust raw-cwd vs TS realpath-of-containing-dir) and nothing else.
3. State-dir/socket-path consistency: in UDS mode, `socketPath` dirname == `stateDir` parent for every fixture (single-derivation invariant, ADR-007 §1).
4. Post-merge obligation (OQ5 residual, not an F4a gate): one live SubagentStop/SubagentStart stderr `cwd` dump from a worktree-isolated subagent during the dogfood soak.

**Coverage Requirement**: All five healthy layouts drift-checked; the one accepted divergence pinned by fixture.

### R-06: Sync read-loop and exit sequencing
**Severity**: High · **Likelihood**: Med
**Impact**: Truncated or missing injection stdout (user-visible context loss) or a hung hook process.

**Test Scenarios**:
1. Chunked response delivery: stub listener writes the length prefix and body in 1-byte and split-header chunks; client accumulates to the full declared length.
2. `'end'` before complete frame → `connect`-class failure, no stdout, exit 0.
3. Declared length 0 and > 1 MiB → reject (`connect` class) without allocating/reading the body.
4. Deadline expiry mid-read → `destroy()` + `timeout`, no partial stdout.
5. Settle-once: timer cleared on every resolution path; all timers `unref()`d; no `process.exit()` anywhere in the new module (grep-gate, per the #4768 pattern — grep, not stdout spy).
6. Stdout-flush ordering: sync stdout fully written before process exit (spawn-level test with async spawn — #4774: never `spawnSync` with an in-process stub).

**Coverage Requirement**: Every branch of the ADR-003 §3 read loop reachable in tests; spawn-level proof that the event loop cannot drain before socket settle.

### R-07 / R-08: Wire-contract additivity and the accept↔Text coupling
**Severity**: High · **Likelihood**: Low
**Impact**: R-07 — F1 goldens or ts-rs bindings break, frozen-contract violation. R-08 — a `Text` response to a non-`accept` caller crashes the deserialization of every deployed frozen Rust hook (blast radius: all local production installs).

**Test Scenarios**:
1. AC-11: the entire pre-existing Rust parity fixture suite + ts-rs binding drift check run unmodified and pass byte-unchanged after the `wire.rs` additions (including the mechanical `accept: None` edits at hook.rs construction sites).
2. Listener unit: `ContextSearch`/`CompactPayload` **without** `accept` → typed `Entries`/`Ack` JSON response, never `Text` (the coupling contract, ADR-001 §6).
3. Listener unit: `accept: "text/plain"` → `Text` only for `Entries`/`BriefingContent` results; `Ack`/`Error`/`Pong` stay JSON regardless (allowlist is a hard contract).
4. Compiled Rust hook end-to-end against the updated daemon: full sync trio unchanged (the strongest R-08 proof, runs the real frozen binary).
5. Queue frames never carry `accept` (transport adds it at serialization) — HTTP frame goldens byte-unchanged; a queued sync frame is impossible by construction (only FNF enqueues) — assert.

**Coverage Requirement**: Additivity proven by unmodified old suites; the Text-only-when-asked invariant asserted at the listener seam and end-to-end with the frozen binary.

### R-09: Injection-header discrimination over UDS
**Severity**: Med · **Likelihood**: Med
**Impact**: SubagentStart envelope vs plain injection misdispatched — the exact vnc-026 bug class (#4778, fixed per #4783: the header IS the discriminator and a wire contract).

**Test Scenarios**:
1. Stdout goldens for the sync trio over UDS vs Rust-hook stdout: ContextSearch plain, ContextSearch-under-SubagentStart envelope, CompactPayload, Ping/briefing.
2. `Text{body}` for Entries starts with `--- Unimatrix Context ---\n` byte-exactly (server-side unit on the shared injection core); BriefingContent body is `content` verbatim (no header added).
3. Single-formatting-truth check: HTTP text/plain body and UDS `Text` body for the same request are byte-identical (same shared core — parity by construction, vnc-025 ADR-005).

**Coverage Requirement**: Envelope and non-envelope dispatch both golden-pinned over UDS; shared-core equivalence asserted HTTP-vs-UDS.

### R-10: Cross-transport replay
**Severity**: Med · **Likelihood**: Med
**Impact**: Replayed frames rejected (data loss) or attribution split surprises consumers.

**Test Scenarios**:
1. AC-04 both directions: enqueue under UDS (no daemon) → next spawn resolves HTTP config → replay accepted at `/observe` with the fresh token; enqueue under HTTP (server down) → next spawn resolves UDS → replay accepted by the live listener.
2. Assert the documented session-id consequence: HTTP-ingested replays land under `http-{sid}`; UDS replays under raw `{sid}` — accepted, but pinned so a future change is deliberate.
3. Replay-before-send order preserved per transport; a failed replay leaves the frame queued (best-effort, FR-26).
4. Poison-pill in the queue (malformed JSON file) does not abort replay of subsequent frames.

**Coverage Requirement**: Both replay directions green against real ingest points; consequences pinned, not just tolerated.

### R-11: Cycle interception through the narrowed matcher
**Severity**: High · **Likelihood**: Med
**Impact**: `cycle_start`/`phase-end`/`stop` silently stop flowing (Unimatrix cycle tracking degrades) or the F-02 security gate weakens.

**Test Scenarios**:
1. `merge-settings.js` output snapshot: PreToolUse matcher is exactly `context_cycle|mcp__unimatrix__context_cycle`; all other events' matchers unchanged.
2. Sentinel matrix on `buildCycleEventOrFallthrough`: non-cycle tool name → `null` (no frame, no queue entry, exit 0); missing `tool_input` → `null` + retained stderr; failed `validateCycleParams` → `null` + retained stderr; valid cycle → frame identical to the Rust hook's (cycle frames stay fully parity-tested).
3. F-02 defense-in-depth: `evil_context_cycle_bypass` (regex-substring match) spawns the hook but sends nothing — exact-equality gate holds.
4. index.js: `null` request returns before transport selection (no network, no stdout, exit 0).
5. Pre-existing-install simulation: a PreToolUse `*` spawn (stale settings.json) for an ordinary tool is a clean no-op.
6. Regression guard: PostToolUse/PostToolUseFailure fallthrough observation untouched (only PreToolUse gets the sentinel).

**Coverage Requirement**: Matcher narrowed at install level AND sentinel proven at client level — both layers independently asserted; cycle frames remain in the byte-parity corpus.

### R-12: SubagentStop server-side independence (uni-zero residual)
**Severity**: Med · **Likelihood**: Low
**Impact**: If any server lifecycle (session close, buffer finalization) awaited SubagentStop, default-off installs would leak sessions/buffers. ADR-004 does not state independence explicitly; code evidence (listener.rs:2919 all-None fallthrough) says there is none.

**Test Scenarios**:
1. Integration: full session lifecycle (SubagentStart → events → Stop) with SubagentStop never sent — session closes normally, buffers finalize, no leaked state.
2. Opt-in matrix (AC-08): key absent → SubagentStop not in generated settings; key `true` → registered with matcher `*`; an arriving SubagentStop event is handled unchanged (client logic untouched).

**Coverage Requirement**: One explicit no-SubagentStop lifecycle test converts the ADR's silent assumption into an asserted contract.

### R-13: No-daemon queue retention (spec-writer flag, SR-13)
**Severity**: Med · **Likelihood**: Low
**Impact**: Unbounded disk growth for local users with no daemon.

**Finding (explicit, per spec FR-16)**: the existing bounds DO cover this path — eviction runs inside `enqueue` itself (queue.js: age-prune of >24 h files, then drop-oldest while count > MAX_FILES=500 or bytes > 5 MiB), so a client that only ever enqueues is still bounded. No blocking architecture finding.

**Test Scenarios**:
1. No-listener loop: enqueue past MAX_FILES; assert count ≤ 500, total ≤ 5 MiB, oldest evicted first.
2. Age case: pre-seed a 25 h-old frame; next spawn (still no daemon) prunes it.
3. AC-04 UX: exactly one stderr line per failed spawn, sync silent, exit 0, no stdout.

**Coverage Requirement**: Bounds pinned on the enqueue-only path specifically (no successful spawn in the test).

### R-14: HTTP-path regression from FR-30 + pruneOffsets wiring
**Severity**: Med · **Likelihood**: Med
**Impact**: F3 remote behavior regresses inside a "local transport" feature (SR-08's exact fear).

**Test Scenarios**:
1. AC-12: full F3 parity + delta suites pass; changed assertions limited to delete-timing, diff-reviewed against FR-31.
2. `pruneOffsets` now live on every FNF spawn: assert it is fail-open (unreadable dir, ENOENT) and adds no I/O to the sync trio (NFR-4 — prune runs on the FNF path only).
3. Offset write cadence, delta frame format, 1 MiB caps, never-queue-delta rule: pinned unchanged (existing tests must still pass byte-identical).

**Coverage Requirement**: The only externally visible HTTP change is delete timing.

### R-15: Latency budget
**Severity**: Med · **Likelihood**: Low
**Scenarios**: AC-05 — F3 AC-13 measurement protocol against the UDS path (live local listener), p95 < 20 ms including project-root detection; FNF and sync measured separately (FNF pays the `'finish'` flush wait).
**Coverage Requirement**: One reproducible perf check in the suite; 40 ms timeout constants asserted (not load-bearing for p95).

### R-16: Dogfood switchover silent loss (post-merge)
**Severity**: Med · **Likelihood**: Med
**Scenarios** (FR-32, zero new code): record pre-switch daily event counts from `unimatrix.db`; daily check of `state.json` breadcrumbs (failureClass `connect`, queueDepth) and queue-residue emptiness while the daemon runs; rollback trigger = sustained `connect` breadcrumbs, queueDepth growth, or >50% day-over-day event-count drop → one-line settings revert.
**Coverage Requirement**: Documented procedure with concrete thresholds exists before the switchover lands.

### R-17: Mixed-client PreCompact (accepted)
**Severity**: Med · **Likelihood**: Low
**Scenarios**: AC-06 covers the supported matrix row (TS-only: stream deltas, fire PreCompact, exactly one server-built block in the golden). The double-prepend row is a documented unsupported configuration (one project, one client) — no F4a test; F5's installer makes selection explicit.
**Coverage Requirement**: AC-06 golden; the binding assumption stated in the spec.

### R-18: Frame-cap handling (also a security surface)
**Severity**: Med · **Likelihood**: Med
**Scenarios**:
1. Write path: building a > 1 MiB payload → fail-open reject (`http_4xx` class), never sent, never thrown.
2. Read path: hostile length prefix (0, 1 MiB+1, 0xFFFFFFFF) → reject **before** allocating the declared size.
3. Boundary: exactly 1,048,576-byte payload accepted both directions (matches `wire.rs` round-trip tests).

**Coverage Requirement**: Both caps tested at boundary and beyond, byte-compared against `wire.rs` fixture behavior.

## Integration Risks

- **Transport seam** (index.js ↔ transport-uds.js): SendResult mapping table (ADR-002 §2) is the contract — every row needs a unit test, because `transform.writeSyncOutput`, `state.recordSendOutcomes`, and queue enqueue all key off it. Non-HTTP interpretations (status 0 on FNF success, `http_4xx` as generic reject) must be pinned so breadcrumb consumers don't assume HTTP.
- **Listener seam** (ADR-001 §5): `wants_text` extracted pre-dispatch, conversion post-dispatch, ONE shared injection core for HTTP and UDS — the shared-core equivalence test (R-09 scenario 3) is the parity backbone.
- **Delta-over-UDS** (AC-07): `transcript_delta` frames from the TS client through `listener.rs:785,1025` into the F2 buffer — end-to-end, asserting buffer content, not just acceptance.
- **Queue ↔ transports** (R-10): frames carry no transport state and no `accept`; both ingest points deserialize the same serde enum — proven by the bidirectional replay tests.
- **merge-settings shared install surface** (ADR-004 §5): the reduced set applies to Rust-hook re-inits too — snapshot test covers both command shapes.
- **vnc-030 cross-feature**: AC-09 first-commit ordering is a dependency contract; additionally, a **UDS-path stamp regression test is owed to vnc-030 after F4a merges** — post-merge obligation, explicitly NOT an F4a test.

## Edge Cases

- Empty injection over UDS sync: `format_injection` → `None` → `Ack` → 204-equivalent SendResult, no stdout (ADR-001 §4).
- Zero-length and exactly-1 MiB frames both directions (R-18).
- Response header split across TCP-equivalent chunk boundaries; 1-byte chunk delivery (R-06).
- Socket file exists but no listener (stale socket after daemon crash): ECONNREFUSED → `connect` class → enqueue.
- Socket path dir absent (daemon never ran): ENOENT → `connect` class → enqueue.
- `EACCES` on the socket (peer-cred/permission failure) → `connect` class, no throw.
- Lone-surrogate stdin: formally excepted (FR-22, #4777/#4788); corpus must contain no lone-surrogate inputs in byte-compare cases; the node:test todo remains tracked.
- Stale settings.json (pre-reduction install): PreToolUse `*` spawns are sentinel no-ops; SubagentStop arrivals handled unchanged.
- Concurrent spawns (two hook events near-simultaneously): connection-per-frame means no shared socket state; queue uses one-file-per-frame — assert no cross-spawn interference in the replay test.
- Abandoned session (TaskCompleted never fires): offset file survives until 7-day prune, then one full re-stream — asserted safe (R-04 scenario 4).

## Security Risks

| Surface | Untrusted input | Damage potential | Blast radius | Mitigation / test |
|---------|----------------|------------------|--------------|-------------------|
| Hook stdin (host-controlled JSON) | event name, cwd, tool_input, prompt | Malicious `cwd` steers project-root walk → socket path under attacker-chosen hash | Confined to `~/.unimatrix/{hash}/` — hash is sha256-derived, no path traversal possible (hex[..16] cannot contain `/` or `..`); non-git fallback fixture pins behavior | Hash-fixture corpus (R-05); fail-open on malformed stdin |
| UDS response frames (daemon → client) | declared length, JSON body | Hostile length → memory pre-allocation DoS; malformed JSON → throw in a never-throw client | Single hook spawn (exit 0 contract) | R-18 scenario 2; malformed-JSON-response → `connect`-class failure, wrapped |
| `accept` field (client → daemon) | arbitrary string | Unexpected response format negotiation | Listener allowlist: only `Entries`/`BriefingContent` ever convert; unknown accept values behave as absent | R-07/R-08 scenarios 2–3 |
| `Text` variant (daemon → frozen Rust hooks) | n/a (protocol coupling) | Deserialization failure in every deployed frozen hook | All production local installs | R-08 scenario 4 (real frozen binary end-to-end) |
| Queue directory (disk) | attacker-writable only if user-level compromise; poison-pill files | Replay abort or crash | Single spawn | R-10 scenario 4 (existing poison-pill immunity re-asserted for UDS replay) |
| `settings.local.json` opt-in key | `unimatrix.hooks.subagent_stop` value | Type confusion (non-boolean) | Install surface only | AC-08 matrix includes non-boolean value → treated as unset |
| PreToolUse matcher (regex in settings) | tool names from host | Substring-named tool (`evil_context_cycle_bypass`) reaching the cycle path | Forged cycle events into the knowledge engine | F-02 exact-equality gate test (R-11 scenario 3) |
| UDS peer-credential auth | local processes | Any local user process with socket access can post frames | Pre-existing daemon posture, unchanged by F4a | Out of scope (no change); noted for F6 |

## Failure Modes

| Failure | Required behavior | Verified by |
|---------|------------------|-------------|
| Daemon absent / stale socket / EACCES | FNF: enqueue + one stderr line; sync: silent, no stdout; exit 0 | AC-04, R-13 |
| Timeout (connect, write-flush, read) | `timeout` SendResult; FNF enqueues (duplicate accepted: at-least-once); no hang past 40 ms | R-01 s4, R-06 s4 |
| Truncated/short response frame | `connect`-class failure, no partial stdout | R-06 s2 |
| Oversized request payload | Client-side reject, never sent, exit 0 | R-18 s1 |
| Malformed queued frame | Skipped, remaining frames replay | R-10 s4 |
| Stripper self-test failure | Gate fails closed (blocks merge) — never measures with a broken stripper | R-03 s1 |
| `pruneOffsets` I/O error | Fail-open, spawn proceeds | R-14 s2 |
| Drop bug during dogfood soak | Detected within one day via breadcrumbs/queue-residue/count-drop; one-line rollback to Rust hook | R-16 |

Invariant across all of the above (NFR-3): never throws to the host, exit 0 always, no stdout on failure, no secrets in stderr/breadcrumbs.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (FNF destroy drops frames) | R-01 | ADR-003: flush-before-FIN, never destroy unflushed, `'finish'`-event resolution; server-side truncation test contract |
| SR-02 (size budget, 3-byte headroom) | R-02, R-03 | ADR-005 gate redefinition merges first (Critical); ADR-001 removes the `format_injection` port — the largest budget driver — entirely |
| SR-03 (wire-contract additivity) | R-07 | ADR-001: `skip_serializing_if` optional + new variant only; AC-11 byte-unchanged proof |
| SR-04 (lone-surrogate parity ceiling) | — (accepted-divergence register) | FR-22 formally excepts it; corpus excludes lone-surrogate inputs; residual discrimination risk covered by R-09 |
| SR-05 (sync read/exit sequencing) | R-06 | ADR-003 §3/§5: read loop, settle-once, no process.exit, unref'd timers |
| SR-06 (parity vs reduction contradiction) | R-11 | FR-21 binding parity-bar split; corpus excludes retired events; cycle frames stay byte-parity-tested |
| SR-07 (dogfood silent loss) | R-16 | Drop detector + concrete rollback triggers (ARCHITECTURE.md); post-merge |
| SR-08 (FR-16 rekey regresses HTTP) | R-14 | ADR-006 minimal key swap + AC-12 regression guard |
| SR-09 (SubagentStop semantics border F5) | R-12 | ADR-004: settings key default-off, F5 owns UX; residual server-independence test added here |
| SR-10 (cross-transport replay) | R-10 | Frames transport-agnostic by construction; bidirectional replay AC; session-id split accepted and pinned |
| SR-11 (mixed-client double-prepend) | R-17 | Documented one-client-per-project assumption; AC-06 covers supported row; no code mitigation (Rust frozen) |
| SR-12 (projectHash divergence) | R-05 | ADR-007 empirical settlement + drift-checked hash-fixture corpus; severity retained, likelihood downgraded |
| SR-13 (no-daemon unbounded queue) | R-13 | Verified: bounds enforced inside `enqueue` (500 files / 5 MiB / 24 h) — covers the enqueue-only path; pinned by test |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-02) | 3 (gate self-test, limit triggers, merge-order audit) |
| High | 4 (R-01, R-04 incl. decision item, R-06, R-11) | 22 |
| Medium | 9 (R-03, R-05, R-07, R-08, R-09, R-10, R-14, R-16, R-18) | 27 |
| Low | 4 (R-12, R-13, R-15, R-17) | 9 |

Post-merge obligations (not F4a gate items): UDS-path stamp regression test owed to vnc-030; OQ5 live worktree-cwd stderr dump during soak; dogfood drop-detector procedure active from switchover day one.

Items requiring human/spec attention before delivery:
1. **R-04**: TaskCompleted is not a registered hook event — decide register-it vs age-prune-only, and correct ADR-006's frame-type claim (SessionClose, not RecordEvent).
2. **R-12**: ADR-004 should state SubagentStop server-side independence explicitly (one sentence); the lifecycle test converts it to an asserted contract either way.
