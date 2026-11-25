# Risk Coverage Report: vnc-050

Stage 3c, 2026-11-25, branch `feature/vnc-050`.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | CA branch leaks before TLS verify | JS byte-capturing TLS server tests (bridge, observe POST, pingForInit, `NODE_TLS_REJECT_UNAUTHORIZED=0`) in `packages/unimatrix/test` | PASS | Full |
| R-02 | Capability ceiling bypass | `mcp/cap_ceiling_http_tests.rs` (step 0 `principal_visible_in_tool_request_context_over_http`, write-tool × agent_id table); infra `test_read_token_search_succeeds_store_denied` (human / omitted / system) | PASS | Full |
| R-03 | Slug guard bypass | `http/router/slug_guard_tests.rs` (route × token × slug, path variants); infra `test_scoped_token_other_slug_returns_403`, `test_cross_slug_403_body_identical_for_registered_and_unknown` | PASS | Full |
| R-04 | Read-token learning side effects | Rust table-snapshot tests; infra `test_read_token_leaves_learning_tables_unchanged` (search/get/lookup/briefing at real daemon; rw drain barrier + rw query_log companion) | PASS | Full |
| R-05 | Revocation not live / tombstone | `client_bundle_headless_e2e.rs` (revoke/list/remint); infra `test_revoked_token_returns_401_without_restart` (open session 401, re-init 401, `--list` revoked, remint fails) | PASS | Full |
| R-06 | Timing leak in scan | scoped-validator unit (index 0 / n-1, no HashMap/early break) | PASS | Full |
| R-07 | Bridge vs observe resolve divergence | JS remote-resolve precedence unit matrix; spawned bridge suites (MCP side); spawned hook `test/hook-client/observe-bundle.test.js` (Stage 3c rework): UNIMATRIX_BUNDLE only, clean HOME, no remote.json/env, so the event lands at the bundle observe_url with the bundle token (pinned fp and fp:null + NODE_EXTRA_CA_CERTS; wrong-fp and untrusted-CA negatives send nothing). Hook precedence store < env pair < bundle, bundle + URL/TOKEN conflict (nothing sent, exit 0, var named), and empty vars give no conflict. Same winners as resolveRemote/bridge | PASS | Full (scenarios 2 and 3 now at the hook entry point; Gate 3c iteration 1 found the earlier "Full" was overstated) |
| R-08 | Extra headers per request type / reserved override | JS capturing proxy + reject matrix | PASS | Full |
| R-09 | Secret/header leakage | JS sentinel grep; Rust CLI stdout/stderr tests; infra `--list` and 403 body token-absence asserts | PASS | Full |
| R-10 | Bundle decoder `fp:null` / parity | `bundle_codec_fp_null.rs`, `fingerprint_parity.rs`, JS parity corpus | PASS | Full |
| R-11 | Pinned regression | cert-pin tests unchanged vs main (gate 3b diff empty); JS fp-source tests | PASS | Full |
| R-12 | Token-file concurrency | `twenty_parallel_cli_mints`, 0600/tmp tests | PASS | Full |
| R-13 | `token:` namespace reservation | `principal.rs::reserved_token_prefix_rejected`; infra stdio `test_agent_id_token_prefix_reserved` (new), HTTP `test_token_prefix_agent_id_rejected_over_http` (no `token:%` in agent_registry) | PASS | Full |
| R-14 | Flag matrix | `invalid_combos_exit_2_no_io`, `revoke_other_slug_label_errors` | PASS | Full |
| R-15 | Legacy disable | `legacy_token_boot.rs` + config×env matrix unit tests; infra `test_legacy_token_still_works_all_slugs` | PASS | Full |
| R-16 | `http_access` audit | slug_guard_tests audit rows; infra 403 body identity | PASS | Full |
| R-17 | Read-token observe | `observe_gate_tests.rs` (DISPATCH_SPY empty); infra `test_read_token_observe_returns_403_zero_rows` (real daemon, rw positive control); JS spawned-hook marker latch tests | PASS | Full |
| R-18 | Bundle/env conflict list | JS conflict-var tests (bridge + hook), empty-string, partial pair | PASS | Full |
| R-19 | Modularity | gate-3b line delta (WARN, non-blocking, documented) | WARN | Full (gate check) |
| R-20 | Residual agent_id gap (accepted) | cap_ceiling regression pin (rw + human write OK, Admin denied) | PASS | Full |
| R-21 | Backward compat | `compat-matrix.test.js` (0.11.3 tarball) incl. old-client write and `fr39_legacy_response_diff` vs main-built baseline (7/7) | PASS | Full |
| R-22 | Omitted agent_id → anonymous (accepted) | cap_ceiling/principal tests | PASS | Full |

## Test Results

### Unit / in-process (Rust)
`cargo test --workspace --no-fail-fast` (setsid/timeout): rc=0 — 7240 passed, 0 failed, 31 ignored. The pre-existing eval sweep flake did not recur.
Full-workspace link smoke (`check-workspace-link-smoke.sh`): rc=0 (#878 holds).

### JS (`npm test`, packages/unimatrix)
Rework re-run: 1406 tests, 1404 passed, 0 failed, 2 skipped, audit PASS (64 suites). Initial run: 1395 tests: 1393 passed, 0 failed, 2 skipped (`test_root_walk_windows_separators`, platform; `fr39_legacy_response_diff`, needs `UNIMATRIX_COMPAT_BASELINE_BIN`). Selection-set audit PASS. The parity-layer2 AC-04 flake did not recur.

### Integration (infra-001, release binary)
| Suite | Passed | Failed | xfail/skip |
|---|---|---|---|
| smoke (`-m smoke`) | 37 | 0 | — |
| tools | 230 | 0 | 1 xfail (GH#405) |
| protocol | 14 | 0 | — |
| security | 29 | 0 | — (includes new stdio test) |
| lifecycle | 108 | 0 | 6 xfail (GH#291, GH#406, embedding), 1 xpass |
| confidence | 13 | 0 | 1 xfail (GH#405) |
| project_isolation | 4 | 0 | — |
| transport_health | 19 | 0 | — |
| https_uds_parity | 11 | 0 | 2 skip (pre-existing) |
| https_uds_parity_matrix | 29 | 0 | — |
| edge_cases | 24 | 0 | 1 xfail (GH#111) |
| headless_access (new) | 9 | 0 | — (3 consecutive green reruns) |
| **Total** | **527** | **0** | 10 xfail, 1 xpass, 2 skip |

All xfails are pre-existing markers with existing GH issues; none added.

## New Integration Tests
- `packages/unimatrix/test/hook-client/observe-bundle.test.js` (10 spawned-hook tests, R-07.2/.3). `test/helpers/stub-server.js` gained a `tls` option (HTTPS stub).
- `compat-matrix.test.js` old-client write cell (AC-20).
- `suites/test_headless_access.py` (9 tests) on `multi_slug_http_server`. The fixture substrate `harness/multi_slug_client.py` gained `mint_token`/`revoke_token`/`list_tokens` (real `client-bundle --headless` CLI against the running daemon), `mcp_open`/`mcp_call` (Streamable HTTP MCP), `observe_record_as`, `query` (WAL-robust read), and passes `UNIMATRIX_MODEL_CACHE` so write tools can embed.
- `suites/test_security.py::test_agent_id_token_prefix_reserved` (stdio leg of R-13).
- Triage: one failure in the new test, caused by the test itself. Random hex markers sometimes matched the PII phone scanner. Fixed with digit-free markers (`_tag()`).

## Gaps
- **Closed (follow-up)**: AC-20 old-client write. New cell `compat_matrix_legacy_enabled: old bridge writes via legacy token (AC-20 write)` uses the 0.11.3 bridge to `context_store` through the legacy token, then looks the entry up. It uses the local model cache, which the server receives through `UNIMATRIX_MODEL_CACHE`.
- **Closed (follow-up)**: FR-39. Baseline server built from `main` (c4140ff0) in a temp worktree, which was then removed. `fr39_legacy_response_diff` ran with `UNIMATRIX_COMPAT_BASELINE_BIN` and passed. compat-matrix: 7 pass, 0 fail, 0 skip. By default it still skips without the env var.
- **AC-17 audit attribution test** reads audit_log through python3 sqlite3 and would skip if python3 were missing. python3 is present here, so it ran.
- **Closed**: `server_db_zero_observation_rows`. AC-18 server-side zero rows is now proven at the real daemon by `test_read_token_observe_returns_403_zero_rows`.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | infra `test_legacy_token_still_works_all_slugs` (2 slugs, write); legacy Rust tests |
| AC-02 | PASS | JS spawned bridge, clean HOME, `NODE_EXTRA_CA_CERTS` |
| AC-03 | PASS | JS partial-pair tests |
| AC-04 | PASS | JS byte-capturing TLS tests |
| AC-05 | PASS | pin tests unchanged vs main; fp-source tests |
| AC-06 | PASS | JS capturing proxy + reject matrix |
| AC-07 | PASS | token_table strict-parse unit tests |
| AC-08 | PASS | cap_ceiling_http_tests; infra read-token store denied (3 agent_ids), DB unchanged |
| AC-09 | PASS | slug_guard_tests; infra cross-slug 403 (MCP + observe) |
| AC-10 | PASS | scoped-validator 401 / constant-time tests; hashes-at-rest test |
| AC-11 | PASS | slug_guard audit tests |
| AC-12 | PASS | headless-smoke (9/9 in npm) |
| AC-13 (delivery) | PASS | scripted local CA smoke |
| AC-13 (live) | POST-DELIVERY | operator check (HD 9), not a gap |
| AC-14 | POST-DELIVERY | operator check (HD 9), not a gap |
| AC-15 | PASS | Rust snapshot tests; infra `test_read_token_leaves_learning_tables_unchanged` |
| AC-16 | PASS | `client_bundle_headless_e2e.rs`; infra revoke → 401 without restart (open session) |
| AC-17 | PASS | CLI tombstone tests; infra list/remint |
| AC-18 | PASS | observe_gate_tests; infra real-daemon zero rows; JS marker latch |
| AC-19 | PASS | `legacy_token_boot.rs` + config×env matrix |
| AC-20 | PASS | compat-matrix old-client initialize/read + old-client `context_store` write via legacy token |
| AC-21 | PASS | compat-matrix remote.json byte compare |
| AC-22 | PASS | compat-matrix env observe (old client ignores fp; CA cell) |
| AC-23 | PASS | compat-matrix legacy-disabled cells |
| AC-24 | PASS | JS pre-vnc-050 v:2 fixture bundle, pinned branch |
| AC-25 | PASS | cap_ceiling_http_tests; infra store denied with `human` |
| AC-26 | PASS | Rust drain tests; infra real-daemon query_log/access unchanged |
| AC-27 | PASS | principal/cap_ceiling anonymous attribution test |
| AC-28 | PASS | JS empty conflict var tests |
| AC-29 | PASS | JS set+empty / empty+empty pair tests |

## Feature Bugs Found
None.

## GH Issues Filed
None (no new xfails; no pre-existing failures surfaced).
