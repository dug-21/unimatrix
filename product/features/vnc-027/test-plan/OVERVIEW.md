# vnc-027 Test Plan — OVERVIEW (Stage 3a)

TS UDS hook client + hook-set reduction (F4a). GH #680. Mixed Rust + JS/TS feature.
Test plans trace to RISK-TEST-STRATEGY.md (R-01..R-18) and ACCEPTANCE-MAP.md (AC-01..AC-12).
Conventions: `node --test` (CommonJS, `node:test`), goldens-only (no hand-written expected — #2984),
hard-fail-never-skip on missing fixtures (#4452), Rust `cargo test` for wire/listener parity.

## Test Strategy by Layer

| Layer | Tooling | Scope |
|-------|---------|-------|
| Rust unit | `cargo test -p unimatrix-engine` / `-p unimatrix-server` | wire.rs round-trip + additivity; listener `handle_connection` `wants_text`/allowlist; shared injection core |
| Rust parity (frozen contract) | `cargo test -p unimatrix-server --lib parity` + `scripts/regen-parity.sh` zero-diff | AC-11 byte-unchanged goldens + ts-rs drift; R-08 frozen-binary end-to-end |
| JS unit | `npm run test:hook-client` (excludes layer2/benchmark) | transport-uds framing/lifecycle/SendResult, config mode matrix, sentinel, state rekey, merge-settings snapshot |
| JS Layer 1 (golden) | parity-layer*.test.js | framing fixtures, sync-trio stdout goldens, hash-fixture corpus replay |
| JS Layer 2 (live binary) | `npm run test:hook-client --include-layer2` (cargo build first) | live UDS listener round-trip, cross-transport replay, delta-over-UDS buffer merge, FNF truncation |
| Perf | `benchmark-spawn` job | AC-05 p95 < 20 ms over UDS |

**Merge-order gate (R-02 Critical):** the size-gate rewrite (AC-09) is the LITERAL FIRST commit. `git log`
must show `test/check-hook-client-size.js` rewrite preceding any `lib/hook-client/` byte growth. This is a
process check at Gate 3c, additionally pinned by the gate's embedded self-test running green on commit 1.

## Risk → AC → Component → Test-Plan Mapping

| Risk | Pri | AC | Component test-plan | Primary test artifact |
|------|-----|----|--------------------|----------------------|
| R-02 | Crit | AC-09 | size-gate.md | embedded self-test corpus + dual-limit triggers + git-log audit |
| R-01 | High | AC-03,AC-04 | transport-uds.md, parity-corpus-uds.md | FNF flush-before-FIN order; live 1 MiB truncation; enqueue-on-failure |
| R-06 | High | AC-03 | transport-uds.md | chunked read loop, declared-length reject, settle-once, no process.exit |
| R-11 | High | AC-08 | build-request-sentinel.md, merge-settings-reduction.md | sentinel matrix + matcher snapshot + F-02 exact-equality gate |
| R-04 | High | AC-10 | state-offset-rekey.md | TaskCompleted deletes / Stop must-NOT; multi-turn persist; pruneOffsets |
| R-03 | Med | AC-09 | size-gate.md | six lexer states, regex-vs-division, string-embedded `//` |
| R-05 | Med | AC-02 | config-transport-selection.md, parity-corpus-uds.md | hash-fixture corpus 5 layouts + corrupt-worktree divergence |
| R-07 | Med | AC-11 | wire-accept-text.md, parity-corpus-uds.md | unmodified old Rust suites green; ts-rs drift |
| R-08 | Med | AC-11 | wire-accept-text.md, listener-preformatted.md | accept↔Text coupling; frozen-binary end-to-end (R-08 s4) |
| R-09 | Med | AC-03 | listener-preformatted.md, parity-corpus-uds.md | `--- Unimatrix Context ---\n` header; HTTP-vs-UDS body equivalence |
| R-10 | Med | AC-04 | parity-corpus-uds.md | bidirectional cross-transport replay; session-id split pinned; poison-pill |
| R-14 | Med | AC-12 | state-offset-rekey.md, index-dispatch.md | full F3 delta suite green; pruneOffsets fail-open + FNF-only |
| R-18 | Med | AC-01 | transport-uds.md | 1 MiB write/read caps at boundary + hostile length prefix |
| R-12 | Low | AC-08 | merge-settings-reduction.md, parity-corpus-uds.md | no-SubagentStop full-lifecycle; opt-in on/off/non-boolean |
| R-13 | Low | AC-04 | transport-uds.md, config-transport-selection.md | enqueue-only queue bounds (500/5 MiB/24 h) |
| R-15 | Low | AC-05 | transport-uds.md | p95 < 20 ms; 40 ms timeout constants asserted |
| R-16,R-17 | — | AC-06 | parity-corpus-uds.md | AC-06 single server-built block; mixed-client documented unsupported (no test) |

## Cross-Component Dependencies

- **wire-accept-text** (commit 2) is prerequisite for **listener-preformatted** and **transport-uds** (the `accept` field + `Text` variant).
- **config-transport-selection** feeds **index-dispatch** (`mode`) and **transport-uds** (`socketPath`).
- **parity-corpus-uds** consumes the live listener (listener-preformatted) and transport-uds; it is the integration backbone for AC-03/AC-04/AC-11.
- **build-request-sentinel** + **index-dispatch** + **merge-settings-reduction** together satisfy AC-08 (two-level reduction).
- **state-offset-rekey** + **index-dispatch** together satisfy AC-10/AC-12.

## Integration Harness Plan

### infra-001 (compiled-server MCP regression — minimum gate)
The wire.rs `accept` field + `HookResponse::Text` variant and listener/observe shared-core are additive,
server-side changes. To prove the live binary's MCP surface is unregressed, run from `product/test/infra-001`:
- **Smoke (MANDATORY minimum):** `python -m pytest suites/ -v -m smoke --timeout=60`
- **`protocol`** (handshake/JSON-RPC/tool discovery — wire-adjacent) and **`tools`** (all 9 tools, response formats — exercises the same engine that gained the additive field) per the suite-selection table ("any server tool logic" → tools, protocol). These are regression-only; no new infra-001 tests are needed (UDS hook transport is not on the MCP JSON-RPC surface infra-001 drives).

### node:test Layer 2 — primary feature integration harness (NEW UDS layer)
Reuse `test/helpers/real-server.js` (spawns the cargo-built binary under isolated `$HOME`). Extend it with a
UDS connect helper (socket path = `{home}/.unimatrix/{hash}/unimatrix.sock`) — additive to the existing helper,
never a parallel scaffold (CLAUDE.md cumulative-infra rule). New integration scenarios:
1. **Live UDS listener round-trip** (AC-03): full post-reduction corpus framed by transport-uds → live listener → identical decode; sync trio stdout byte-identical to Rust-hook goldens.
2. **FNF truncation contract** (AC-03/R-01): max-size 1 MiB FNF frame recorded complete server-side; kill-mid-write → full delivery or clean server frame error, never silent truncation.
3. **Cross-transport replay both directions** (AC-04/R-10): UDS-enqueue → HTTP `/observe` replay with fresh token; HTTP-enqueue → live UDS replay. Session-id split (`http-{sid}` vs raw) pinned; poison-pill immunity.
4. **Delta-over-UDS buffer merge** (AC-07): `transcript_delta` frames via UDS into the F2 buffer; assert PreCompact block CONTENT (reuse `precompact()`/`prepopulateBuffer()`).
5. **No-SubagentStop lifecycle** (AC-08/R-12): full SubagentStart→events→Stop with SubagentStop never sent; session closes, buffers finalize.
6. **PreCompact single-block** (AC-06): stream deltas over UDS, fire PreCompact, exactly one server-built block.

### Rust cargo integration
- wire.rs round-trip + additivity unit tests (`-p unimatrix-engine`).
- listener `handle_connection` `wants_text`/allowlist units (`-p unimatrix-server`).
- AC-11: existing parity fixture suite + `scripts/regen-parity.sh` zero-diff + ts-rs binding drift, run UNMODIFIED.
- R-08 s4: compiled frozen Rust hook end-to-end against the updated daemon — full sync trio byte-unchanged.

## Accepted Divergences (corpus must except EXACTLY these — FR-22)
Lone-surrogate stdin (#4788, node:test todo); no bare probe connection; PreCompact client-block source (Rust-only);
event-set divergence by design (retired PreToolUse obs + default-off SubagentStop). Any new divergence is fixed or
registered by explicit decision — never silently tolerated.

## Open Questions
1. UDS Layer 2 helper placement — extend `real-server.js` with a `udsConnect()`/`socketPath` accessor (recommended, cumulative) vs a sibling `real-server-uds.js`. Stage 3b architect call; the test plan assumes extension.
2. AC-05 p95 over UDS: confirm `benchmark-spawn` can target a live local socket in CI (Linux-only job, matching Layer 2 scoping). If the soak machine is required, AC-05 may degrade to a documented local-run check.
