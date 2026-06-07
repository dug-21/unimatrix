# Risk Coverage Report: vnc-026

**Phase**: Stage 3c Test Execution | **Branch**: feature/vnc-026 | **Date**: 2026-06-07
**Tester**: vnc-026-agent-21-tester

## Execution Summary

| Suite | Tool | Total | Pass | Fail | Skip | Todo |
|-------|------|-------|------|------|------|------|
| Hook-client unit + Layer 1 parity | `npm run test:hook-client` | 421 | 419 | 0 | 1 | 1 |
| Hook-client Layer 2 (vs merged F2 server) | `npm run test:hook-client:layer2` | 8 | 8 | 0 | 0 | 0 |
| Parity-corpus generator guards | `cargo test -p unimatrix-server --lib parity` | 9 (+1 ignored) | 9 | 0 | 0 | 0 |
| Integration smoke gate (infra-001) | `pytest -m smoke` | 23 | 23 | 0 | 0 | 0 |

- **Unit + Layer 1 + Layer 2 (Node)**: 429 run, 427 pass, 0 fail. Aggregate green.
- **Accepted non-pass markers** (pre-adjudicated, do NOT reopen):
  - **1 todo** — `test_stdout_parity_stdout-subagent-non-entries-fallback_byte_identical`: wire-contract limitation (text/plain erases Entries-vs-BriefingContent per ADR-003 allowlist; client implements ADR-002 always-wrap). Fix is server-side, C-07 out of scope. Adjudicated at Gate 3b.
  - **1 skip** — Windows-only root-walk test (backslash separators); cannot execute on Linux runner. Exercised in CI on the Windows OS matrix (AC-12 / R-14).
- **Parity corpus**: 83 committed cases; MANIFEST.json maps **104 `build_request` arms** (R-02 coverage audit). Generator dev-test is `#[ignore]` by design (runs via CI drift job / `scripts/regen-parity.sh`); its branch-coverage guard `test_generator_branch_coverage` PASSES.
- **Zero runtime deps** confirmed (`dependencies: {}`) — AC-12.

## Pre-existing / Out-of-Scope Failure (triaged, NOT fixed)

`packages/unimatrix/test/init.test.js::test_creates_mcp_json_on_clean_project` fails in the
full package suite (NOT in the hook-client suites this feature owns; NOT smoke-marked).

- **Triage**: stale test assertion. Production `init.js` sets `LD_LIBRARY_PATH` in `.mcp.json`
  env since commit `07062006` (2026-03-13, "fix(init): set LD_LIBRARY_PATH..."); the test still
  asserts `env: {}`. `git diff main` for both `init.js` and `init.test.js` is **empty** — untouched
  by vnc-026. Origin: nan-004 init lineage (#221).
- **Action**: filed **GH#695** ("[test] test_creates_mcp_json_on_clean_project: stale assertion").
  Per USAGE-PROTOCOL.md triage tree (pre-existing → file Issue, do not fix in feature PR). No
  `xfail` marker needed in vnc-026's owned suites (the failing test is outside them and is not
  run by `test:hook-client`). Documented at Gate 3b as out of scope.

## Coverage Summary (R-01..R-20)

| Risk ID | Priority | Test(s) / Evidence | Result | Coverage |
|---------|----------|--------------------|--------|----------|
| R-01 build-request parity divergence | Critical | `build-request.test.js`, `parity-layer1.test.js` (83-case corpus, structural JSON eq after volatile normalization), `normalize.test.js`; cargo generator branch-coverage guard | PASS | Full |
| R-14 cross-platform stdin/path | Critical | `index.test.js` (fd-0 piped/empty/>1 MiB), `config.test.js` (root walk, hash), `state.test.js` (chmod no-op); Windows root-walk test skipped on Linux, covered by CI OS matrix (AC-12) | PASS | Full (Windows arms via CI matrix) |
| R-02 corpus incompleteness | High | MANIFEST.json maps 104 `build_request` arms to cases; `test_generator_branch_coverage` (cargo) fails if a new arm has no case | PASS | Full |
| R-04 UTF-8 boundary trim | High | `delta.test.js` (mid-2/3/4-byte trim, span-inside-one-char, property `sum(shipped)==contiguous prefix`); Layer 2 `test_l2_adversarial_growth_sequence_contiguous_prefix` | PASS | Full |
| R-09 silent config-resolution failure | High | `config.test.js` FR-06 matrix (nested-.git, key-drop, partial env pair, subdir cwd); no-network proof via transport spy | PASS | Full |
| R-10 breadcrumb wrong/missing | High | `state.test.js` failure-class matrix → breadcrumb, content-free scan, read-only state dir; `transport-http.test.js` classification matrix | PASS | Full |
| R-11 ownership-pattern merge corruption | High | `init-remote`/merge-settings tests: regex pos/neg table incl. spaced paths (WARN 2 resolved), 4-shape re-run matrix, double-fire count == 1 | PASS | Full |
| R-03 stdout envelope byte divergence | Medium | `transform.test.js` adversarial-content byte goldens; Layer 1 `expected-stdout.bin` byte-compare; literal-template grep-gate | PASS | Full |
| R-05 concurrent-spawn offset regression | Medium | `delta.test.js`/`state.test.js` concurrent offset race, atomic rename; Layer 2 F2 dedupe end-state | PASS | Full |
| R-06 elision frame geometry (re-graded Med) | Medium | Layer 2 `test_l2_elision_mid_session` asserts the four pinned ADR-008 items (hole behind content at `(last_offset, file_len−bytes.length)`; `high_water==file_len`; `contiguous_tail` crosses seam; no NUL bytes) via observable PreCompact body | PASS | Full |
| R-07 delta livelock | Medium | `delta.test.js` permanent 413/401 stub → offset never advances, no queue file, bounded per-spawn cost | PASS | Full |
| R-08 queue misbehavior | Medium | `queue.test.js` lifecycle, poison-pill, drop-oldest (file/size caps), 24h prune, concurrent double-send, 32/256 KiB budget | PASS | Full |
| R-12 local HOOK_EVENTS regression | Medium | merge-settings/init regression over fresh + pre-existing local configs; 9-event set; diff confined to list+matchers (AC-16) | PASS | Full |
| R-13 spawn budget erosion | Medium | AC-13 benchmark (`benchmark-spawn.test.js`) incl. hash+root-walk+health.json write; sync-path fs-spy zero queue/delta I/O | PASS | Full |
| R-16 token leakage | Medium | Token-leak scans across argv, settings.json, breadcrumb, stderr, queued frames; url_host-only; 0600 mode | PASS | Full |
| R-17 F2 semantic drift | Medium | Pre-population behind ONE helper (`layer2-fixtures.js`); Layer 2 run against merged F2 server (PR #692, C-08) | PASS | Full |
| R-20 drift-check vacuity | Medium | Three non-vacuity guards (generator-ran marker, MANIFEST mtime+case_count>0, zero git diff); CI fails-not-skips | PASS (config + guard) | Full |
| R-15 sync stdout server-controlled | Low | `transport-http.test.js` non-text/plain 200 dropped, oversized 200 body, empty body no-stdout | PASS | Full |
| R-18 init Ping false confidence | Low | `transport-http.test.js` `test_ping_wrong_token_auth_message`, non-Pong rejected, strict Pong parse | PASS | Full |
| R-19 ppid-fallback session collision | Low | `build-request.test.js`/`state.test.js` ppid fallback parity + session-key sanitization (traversal corpus) | PASS | Full |

## Test Results

### Unit Tests (Node hook-client)
- Total: 421 | Passed: 419 | Failed: 0 | Skipped: 1 | Todo: 1
- Layer 1 parity is included in the above (`parity-layer1.test.js`, 83-case corpus).

### Integration Tests
- **Layer 2 (vs merged F2 `unimatrix-server`)**: 8 total / 8 pass / 0 fail — AC-05 drops,
  AC-06 grow/hold offset values, AC-07 elision-mid-session (4 pinned ADR-008), AC-10 ≥8-session
  byte isolation + raw `session_id` on wire.
- **infra-001 smoke gate**: 23 total / 23 pass / 0 fail (199.5 s). No regression from the
  additive Rust generator dev-test (C-07 holds — zero server production changes).
- **Cargo parity guards** (`--lib parity`): 9 pass / 0 fail / 1 ignored (the generator itself,
  by design).

## Gaps

None. Every risk R-01..R-20 has executed test coverage at Full. The Windows-specific arms of
R-14 (root-walk backslash) run only on the CI Windows OS matrix (AC-12 expansion) — exercised
by configuration, skipped on this Linux runner by design, not a gap.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `parity-layer1.test.js` structural JSON equality over 83-case corpus vs Rust goldens; MANIFEST covers 104 arms |
| AC-02 | PASS | `transport-http.test.js` FNF method/path/Bearer header + byte-exact empty stdout; 204=success |
| AC-03 | PASS | `transport-http.test.js` `Accept: text/plain` sync; 200→stdout / 204→none; raw-JSON canary |
| AC-04 | PASS | `transform.test.js` byte-compare vs `expected-stdout.bin`; literal-template grep-gate (no envelope `JSON.stringify`) |
| AC-05 | PASS | Layer 1 PreCompact byte-identity (`test_precompact_stdout_byte_identical_to_server_body`) + Layer 2 `test_l2_drops_content_equivalence`; pinned ADR-008 server-state asserted |
| AC-06 | PASS | Layer 2 `test_l2_grow_hold_grow_offset_values` — declared + persisted offset values; UTF-8 contiguous-prefix property |
| AC-07 | PASS | `delta.test.js` end-anchored frame (`offset==file_len−bytes.length`), <1 MiB post-serialize; Layer 2 `test_l2_elision_mid_session` continues to correct PreCompact restoration |
| AC-08 | PASS | Sync-path fs-spy: single POST, no delta I/O, zero queue I/O (R-13) |
| AC-09 | PASS | `transport-http.test.js` failure matrix (ECONNREFUSED/timeout/401/413/500) × events; independent `Promise.allSettled` delta-failure isolation; breadcrumb class per failure |
| AC-10 | PASS | Layer 2 `test_l2_concurrency_attribution` (≥8 interleaved sessions, byte isolation) + `test_l2_raw_session_id_on_wire_server_mints_http_prefix` |
| AC-11 | PASS | init-remote matrix: fresh / re-run / foreign hooks / old-style entries; spaced-path regex table; double-fire==1; argv+file+0600+gitignore; Ping wrong-token loud |
| AC-12 | PASS (config) | `dependencies: {}` (zero deps); CI matrix Node 18/20/22/24 × Linux/macOS/Windows + size <100 KB + drift-check fail-not-skip — verified by configuration |
| AC-13 | PASS (per Gate 3b adjudication) | `benchmark-spawn.test.js` asserts **client_work_ms** p50 0.07 / p95 0.11 ms ≤ targets 12/20 ms (~100× margin); full-spawn p50 25.7 ms overage is Node interpreter cold-start (~11.6 ms = C-03's "~12 ms spawn floor"), documented in artifact. Binding Gate-3b decision (item 6) — not reopened. |
| AC-14 | PASS | `contract-roundtrip.test.js` client frames vs `bindings/fixtures/*.json` incl. `transcript_delta_payload.json` |
| AC-15 (amended) | PASS | `queue.test.js` fail→enqueue→replay-order→drain, poison-pill, sync-trio non-enqueue, 32/256 KiB bounds; `delta.test.js` delta-failure → offset-non-advance + NO queue file (amended letter, Delivery Note 1) |
| AC-16 | PASS | merge-settings/init regression: 9-event local set fresh + pre-existing, back-compat byte-identical settings output, diff confined to list+matchers (SR-07) |

## Notes

- **AC-13 caveat** (carried, not a regression): only the isolated client-work measurement meets
  the literal 12/20 ms target. Full child-spawn wall time on this arm64 container is ~25 ms,
  dominated by Node interpreter startup (~11.6 ms) — accepted at Gate 3b as the C-03 spawn floor.
  On CI-class hardware the spawn floor is lower; the client's own work has ~100× headroom.
- **ass-071 freebie (declined)**: the Layer 2 harness drives SubagentStart / PreCompact / delta
  flows but never produces a real SubagentStop stdin payload, so there is no authentic payload to
  capture (synthesizing one answers nothing). Declined for the same reason agent-14 did.
- **GH Issues filed**: #695 (pre-existing stale `init.test.js` assertion, nan-004 lineage).
