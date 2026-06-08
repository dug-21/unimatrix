# Risk Coverage Report: vnc-027

TS UDS hook client + hook-set reduction (F4a). GH #680. Stage 3c execution.
Scope of this report: the integration/live layers deferred from Stage 3b (live UDS
listener round-trip, FNF truncation, cross-transport replay, delta-over-UDS buffer
merge, frozen-hook end-to-end, UDS latency) plus full regression of the Stage 3b
unit/fixture suites and the infra-001 MCP harness.

Binary: `target/release/unimatrix` rebuilt from the Stage 3b branch (`feature/vnc-027`).
UDS is Unix-only; this is Linux, so the live-listener layers ran in full.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | FNF frame loss / silent truncation (Node write-buffer drop, destroy-before-flush) | `transport-uds.test.js` (destroy-after-finish, flush-timeout, never-reads); LIVE `test_fnf_large_frame_recorded_complete`, `test_fnf_truncated_frame_not_silently_recorded` | PASS | Full |
| R-02 | Size-gate merge-order violation (3-byte headroom; vnc-030 depends on redefinition) | `size-gate.test.js` self-test corpus + dual-limit triggers; git-log merge-order = process check (Gate 3c) | PASS | Full (test) / process-pinned |
| R-03 | Comment-stripper state-machine miscount | `size-gate.test.js` six-state lexer + regex-vs-division + string-embedded `//` | PASS | Full |
| R-04 | AC-10 TaskCompleted keying unreachable (age-prune authoritative per ADR-006) | `state.test.js` deleteOffset Stop-negative / TaskCompleted-positive; `index.test.js` canonical-event keying; multi-turn persist | PASS | Full |
| R-05 | TS/Rust projectHash divergence across layouts | `config.test.js` + `parity-uds-framing`/hash-fixture corpus (5 healthy layouts + pinned corrupt-worktree); socketPath-dirname==stateDir-parent invariant | PASS | Full |
| R-06 | Sync read-loop / exit sequencing (partial frames, premature drain) | `transport-uds.test.js` chunked/split-header reads, declared-len reject, settle-once, all timers unref, no `process.exit` | PASS | Full |
| R-07 | Wire-contract additivity slip (serialized bytes of existing frames) | `cargo -p unimatrix-engine wire` (101) round-trip + None-vs-omission dual-direction + ts-rs export; `scripts/regen-parity.sh` zero-diff | PASS | Full |
| R-08 | `Text` frame to a frozen hook that did not send `accept` (deser crash) | listener units (accept↔Text coupling, allowlist); LIVE `test_frozen_rust_hook_precompact_byte_identical_to_ts_client`, `test_frozen_rust_hook_userpromptsubmit_empty_db_parity` (R-08 s4) | PASS | Full |
| R-09 | SubagentStart envelope / injection-header discrimination over UDS | `parity-uds-sync-stdout.test.js` (plain + envelope + briefing goldens, header byte-exact); LIVE PreCompact body equivalence (shared core) | PASS | Full |
| R-10 | Cross-transport replay (session-id split, auth asymmetry) | LIVE `test_replay_uds_origin_frames_over_http`, `test_replay_http_origin_frames_over_uds`, `test_replay_poison_pill_does_not_abort_subsequent`; session-id split pinned (raw vs http-) | PASS | Full |
| R-11 | Cycle interception regression through narrowed PreToolUse matcher | `build-request*.test.js` sentinel matrix (null paths + valid cycle frame); `merge-settings.test.js` matcher snapshot; F-02 exact-equality gate | PASS | Full |
| R-12 | SubagentStop default-off server-side independence | LIVE `test_no_subagentstop_full_lifecycle` (register→deltas→close, SubagentStop never sent, buffer finalizes, no ERROR/panic); `merge-settings.test.js` opt-in matrix | PASS | Full |
| R-13 | No-daemon enqueue-forever unbounded growth | `queue.test.js` enqueue-only bounds (500/5 MiB/24 h, drop-oldest, age-prune) | PASS | Full |
| R-14 | FR-30/FR-31 + live `pruneOffsets` regress the HTTP delta path | `state.test.js` pruneOffsets 7-day fail-open FNF-only; `delta.test.js` + existing F3 delta suites byte-unchanged; infra-001 tools/protocol no regression | PASS | Full |
| R-15 | Latency budget breach over UDS | LIVE `test_uds_fnf_and_sync_p95_under_20ms` (p95 sync≈0.14 ms / fnf≈0.08 ms, n=60); `test_timeout_constant_is_40ms` | PASS | Full |
| R-16 | Dogfood switchover silent loss (post-merge) | Documented drop-detector procedure (FR-32) — post-merge obligation, NOT an F4a gate (no code/test) | N/A | Deferred (post-merge, by design) |
| R-17 | Mixed-client PreCompact double-prepend | LIVE `test_uds_precompact_single_server_built_block` (TS never client-prepends; one server block); mixed-client row documented unsupported (no test, by design) | PASS | Supported row full |
| R-18 | 1 MiB frame-cap handling (oversized build, hostile declared length) | `parity-uds-framing.test.js` boundary fixtures; `transport-uds.test.js` declared-len 0/>1 MiB reject-before-alloc; LIVE `test_frame_cap_boundary_exact_and_over` | PASS | Full |

## Test Results

### Unit Tests
- **Rust `cargo test -p unimatrix-server --lib`**: 3617 passed, 0 failed, 1 ignored.
- **Rust `cargo test -p unimatrix-engine --lib wire`** (AC-11 additivity + ts-rs): 101 passed, 0 failed.
- **Node hook-client offline/unit suites** (`node --test`, default + fixture layers): part of the 559-pass aggregate below; 0 failed, 1 skipped (FR-22 lone-surrogate `node:test` todo — formally excepted, #4788).
- **Parity regen drift** (`scripts/regen-parity.sh`): zero git diff under `test/fixtures/parity/` (AC-11 frozen-contract + ts-rs binding drift gate).

### Integration Tests
- **Node Layer 2 (live binary)** via `node test/run-hook-client.js --include-layer2`: aggregate **559 passed, 0 failed, 1 skipped**.
  - NEW live UDS Layer 2 (this stage, `parity-layer2-uds.test.js`): **16 passed** — round-trip, FNF large/truncation, cross-transport replay (both directions), delta-over-UDS merge, PreCompact single block, no-SubagentStop lifecycle, frozen-hook e2e, latency.
  - Existing Layer 2 (HTTP live, unmodified): green.
- **infra-001 MCP harness** (compiled-server regression for the additive wire change):
  - `smoke` (mandatory gate): **23 passed**.
  - `protocol`: **13 passed**.
  - `tools`: **185 passed, 3 xfailed** (pre-existing: GH#405, GH#305, GH#575 — none caused by vnc-027).
  - Integration totals: **221 passed, 3 xfailed, 0 failed**.

### Triage / GH Issues
- No new integration failures. The 3 `tools` xfails are pre-existing markers (GH#405 deprecated-vs-active confidence timing; GH#305 baseline_comparison null; GH#575 error-message wording) carried in from prior features — **no new GH Issues filed, no new xfail markers added**. The additive `accept` field + `HookResponse::Text` variant produced zero MCP-surface regression.
- No integration tests were deleted, skipped, or commented out.

## Test Infrastructure Changes (cumulative, per CLAUDE.md)
- Extended `test/helpers/real-server.js` (NOT a parallel scaffold): polls the hook socket `{dataDir}/unimatrix.sock` into existence (hard-fail-never-skip, #4452) and adds `socketPath`, `udsPost(frame, opts)` (drives the SHIPPED `transport-uds` module), and `udsConnectRaw()` (adversarial truncation framing).
- Added `test/hook-client/parity-layer2-uds.test.js` (Layer 2; excluded from the cross-OS matrix by the existing `parity-layer2*` rule, scoped to the Linux live-binary job).

## Gaps

- **R-16 (dogfood drop-detector)** is a post-merge obligation by design (FR-32) — a documented daily procedure with rollback thresholds, not an F4a gate item. No code/test; tracked in ACCEPTANCE-MAP Post-Merge Obligations.
- **R-17 mixed-client double-prepend**: only the supported one-client-per-project row is tested (single server-built block). The unsupported mixed-client row is documented, not tested — by SCOPE design (Rust hook frozen until F6).
- **R-04 TaskCompleted branch** is unreachable-but-tested by design (ADR-006 age-prune authoritative); its end-to-end keying cannot fire under current `HOOK_EVENTS` registration — covered by unit test of the branch + the assertable Stop-negative, as the architecture mandates.
- No coverage gaps on any High/Critical risk.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `parity-uds-framing.test.js` byte-compare vs Rust `wire.rs` fixtures; 1,048,576 B boundary both directions; LIVE `test_frame_cap_boundary_exact_and_over` |
| AC-02 | PASS | `config.test.js` mode matrix; hash-fixture corpus (5 layouts + pinned corrupt-worktree); socketPath-dirname==stateDir-parent invariant |
| AC-03 | PASS | LIVE `test_uds_ping_sync_roundtrip_pong`, `test_uds_fnf_session_register_status0`, `test_uds_context_search_empty_db_no_injection`; sync-trio stdout goldens (`parity-uds-sync-stdout.test.js`); FNF truncation contract live |
| AC-04 | PASS | `transport-uds.test.js` fail-open classes (ENOENT/ECONNREFUSED/EACCES→connect, exit 0); LIVE bidirectional replay + poison-pill; queue enqueue-only bounds; session-id split pinned |
| AC-05 | PASS | LIVE `test_uds_fnf_and_sync_p95_under_20ms` (p95 sync≈0.14 ms / fnf≈0.08 ms over the live socket, FNF vs sync separate); 40 ms timeout constants asserted |
| AC-06 | PASS | LIVE `test_uds_precompact_single_server_built_block` — client stdout == server block + newline; TS never client-side-prepends |
| AC-07 | PASS | LIVE `test_transcript_delta_over_uds_merges_into_f2_buffer` — buffer CONTENT asserted (UDS delta → CompactPayload Text body), not just acceptance |
| AC-08 | PASS | `build-request*`/`merge-settings` sentinel + matcher snapshot + opt-in matrix; LIVE `test_no_subagentstop_full_lifecycle` (R-12) |
| AC-09 | PASS | `size-gate.test.js` embedded self-test corpus + dual independent limit triggers; merge-order = git-log process check at Gate 3c |
| AC-10 | PASS | `state.test.js`/`index.test.js` canonical-event keying — Stop does NOT delete, TaskCompleted does; multi-turn persist; pruneOffsets 7-day fail-open FNF-only |
| AC-11 | PASS | `cargo engine wire` (101) additive None-vs-omission + ts-rs; `regen-parity.sh` zero-diff; LIVE `test_frozen_rust_hook_precompact_byte_identical_to_ts_client` + empty-DB parity (R-08 s4); infra-001 protocol/tools no regression |
| AC-12 | PASS | F3 delta/HTTP suites byte-unchanged; `pruneOffsets` fail-open FNF-only (NFR-4); infra-001 tools/protocol regression clean — only externally visible change is delete-timing |

## Self-Check
- Unit tests executed (cargo + node summaries captured) — PASS.
- Integration smoke tests passed (`pytest -m smoke`, 23) — PASS.
- Relevant suites run per selection table (protocol, tools for server-tool/wire change) — done.
- All `xfail` markers correspond to pre-existing GH Issues (#405/#305/#575); none added this stage.
- No integration tests deleted or commented out.
- Every R-01..R-18 mapped to tests + results; AC-01..AC-12 verified.
