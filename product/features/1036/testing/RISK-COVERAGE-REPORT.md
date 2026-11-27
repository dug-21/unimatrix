# Risk Coverage Report: 1036

Branch `feature/1036-harness-wiring` @ 1c431cba. Node-only feature (`packages/unimatrix`); no crates changed vs main.

## Coverage Summary

| Risk | Description | Test(s) (examples, all PASS) | Coverage |
|---|---|---|---|
| R-01 | modes | `test_s3_*` mode-drift/keep-mode tests, umask fixtures; `D-036 — credential store mode repair precedes the Ping` | Full except spawned `--bundle` remote.json leg (D-036 accepted gap, #1053) |
| R-02 | fixpoint | wire-plan/merge-settings/claude-json/codex/opencode mutate-twice tests; AC-5 double init | Full |
| R-03 | containment | `containWithin*`, spawned outside/inside-root init, `test_symlinked_root_resolves_to_realpath_same_hash` | Full |
| R-04 | path divergence | `SR-09 parity: configDirs equals resolver paths…`, `FR-01 grep: no private codexHome…`, FR-34 spawned status | Full |
| R-05 | same-file | `test_ac1b_same_file_claude_config_dir_{exact_one_entry,trailing,symlink_one_entry}`, `sameRealFile / realPathForm` | Full |
| R-06 | migration | trust-writer-core migrated tests, trust byte-identical copy | Full |
| R-07 | dry-run | AC-9 parity harness, `dry-run lists user-level writes; set of paths equals the real run` | Full |
| R-08 | idempotency | AC-5 double init (overrides, git, bundle) | Full |
| R-09 | env leak | `scrubbedEnv strips all override vars…`, `scrubbedEnv re-adds an override only when under tmp…`, `parent GIT_DIR … scrubbed` | Full |
| R-10 | §6a names | `legacy exports and mtime CAS gone` (contract-conformance), S-5 grep clean, `lib/atomic-write.js` absent | Full |
| R-11 | gating | `test_r11_w2_skipped_foreign_leaves_user_scope_untouched`, `test_r11_w5_skipped_malformed_keeps_twin`, W1 verify-failed | Full |
| R-12 | swallow | `test_r12_rename_failure_{codex_hooks,codex_config,opencode}_throws_naming_file_never_skipped` | Full |
| R-13 | CAS | AC-7 fault-seam tests (once/always) | Full |
| R-14 | git | `test_ac8_fr42_fixtures_expected_action_and_clean_diff`, GIT_DIR scrub | Full |
| R-15 | fidelity | `__proto__`/big-int/key-order fixtures | Full |
| R-16 | parse posture | `S-1 spawned: unparseable resolved opencode.json named, exit 0`, `test_fr53_unparseable_resolved_claude_json_exits_1_naming_file` | Full |
| R-17 | edge values | `test_fr53_invalid_override_warns_exit_0`, S-2 matrix | Full |
| R-18 | status local-scope | `S-1 spawned --json: all five claudeMcp.status values reachable`, missing-entry, twin-only stale | Full |
| R-19 | foreign-at-key | AC-4 RT table rows with foreign-at-key | Full |
| R-20 | plugin predicate | hook-ownership FR-15 tests | Full |
| R-21 | pkg swap | install-store staging fault | Full |
| R-22 | temp | `temp: wx flag, name pattern, in real target dir; collisions regenerate, 5 -> io`, `test_ac10_spawned_rename_failure_settings_local_no_temp_exit1_names_file` | Full |
| R-23 | trust hot path | `trust-selfcontained.test.js`, `check-zero-deps.js` OK, byte-identical copy; latency measured below | Full (latency: no regression vs main) |
| R-24 | win32 | `renameWithRetry: win32 retries EPERM/EBUSY/EACCES up to 10…`, `containWithin case-insensitive on injected win32/darwin` | Full (injected fs only, by design) |
| R-25 | real-binary | `test_ac1c_real_claude_mcp_get_reads_resolved_entry`, `test_ac2b_real_opencode_debug_config_{opencode_config_dir,xdg_only,both_set_precedence}` all ran (not skipped) | Partial: Codex file-level only (D-012, #1053, SKIP) |

## Test Results

### Node unit + assembled + spawned (`npm test`, per set)
| Set | Tests | Pass | Fail | Skip |
|---|---|---|---|---|
| default (run-hook-client) | 979 | 978 | 0 | 1 |
| parity-layer2 | 24 | 24 | 0 | 0 |
| package | 973 | 969 | 2 | 2 |
| wiring-e2e | 45 | 42 | 0 (3 cancelled) | 0 |
| opencode-plugin | 37 | 37 | 0 | 0 |
| audit | pass (rc 0) | | | |

Gates: `check-test-count.js` PASS (64 suites, 1421 tests, no decrease); `check-zero-deps.js` OK.

Failures, triaged as pre-existing/environmental (D-037, #1055): `resolve-binary` (2) and `wiring-e2e linux local` (3 cancelled: "refusing to overwrite an existing node_modules/@dug-21/unimatrix-linux-x64"). Both come from the locally installed `@dug-21/unimatrix-linux-x64`, and neither test file is changed on this branch. I moved the package aside temporarily: `resolve-binary` 9/9 and `wiring-e2e` 45/45 pass. The package was then restored. `npm test` stops at the failing `package` set, so the later sets were run individually.

Skips: `test_root_walk_windows_separators` (win32), Codex real-binary (D-012, #1053), `fr39_legacy_response_diff` (needs a main-built baseline binary), `unimatrix.cmd behaviour` (win32).

### Integration (infra-001 smoke)
37 passed, 689 deselected, 0 failed (275 s). Ran with `target/release/unimatrix`, with pytest installed in a scratch venv because it is not on the system python. No xfails added.

### Hook-path latency vs nan-024 ADR-005 (D-094) budget (`benchmark-wiring.js`, linux, node 18.20.8, n=50)
| case | budget p95 | branch p95 (runs 1/2) | main p95 |
|---|---|---|---|
| cloud FNF PostToolUse | 74 | 121.1 / 131.2 | 122.9 |
| cloud sync UserPromptSubmit | 74 | 118.1 / 121.9 | 115.5 |
| NOT-A-PROJECT hook | 50 | 68.5 / 71.1 | 72.0 |
| mcp-stdio NOT-A-PROJECT | 50 | 73.2 / 71.9 | 70.1 |
| `node -e 0` baseline | - | 57.6 / 58.6 | 58.9 |

Every row breaches its absolute budget on this host for both the branch and main. The baseline (`node -e 0`) p95 alone exceeds the 50 ms budget. The branch is within noise of main, so this feature adds no regression. The breach is pre-existing and comes from the host. CI's latency gate reports only (D-096).

## Gaps
- R-01/S-3: no spawned `init --bundle` check of remote.json mode repair (D-036, #1053). The credstore unit test and the write-before-ping order test cover it.
- R-25: Codex real-binary check not covered (D-012, #1053).
- Latency budget: breaches on this devcontainer for both main and the branch. Not caused by this feature.

## Acceptance Criteria Verification
| AC | Status | Evidence |
|---|---|---|
| AC-1 | PASS | `test_ac1a_claude_config_dir_writes_only_resolved_and_keeps_default_profile`, `test_ac1b_*` (3), `test_ac1c_real_claude_mcp_get_reads_resolved_entry` (real claude) |
| AC-2 | PASS | `test_ac2a_*` (3), `test_ac2b_real_opencode_debug_config_*` (3, real opencode, `--standalone`) |
| AC-3 | PASS | 15 `ac3_*` tests (symlink preserved, D-026 outside-root exit 1) |
| AC-4 | PASS | RT table tests (4) |
| AC-5 | PASS | double-init tests (5) |
| AC-6 | PASS | owned-entry scanner test |
| AC-7 | PASS | fault-seam tests (4) |
| AC-8 | PASS | `test_ac8_fr42_fixtures_expected_action_and_clean_diff` + 2 |
| AC-9 | PASS | dry-run parity tests (3) |
| AC-10 | PASS | temp `wx`/regex/collision test; `test_ac10_spawned_rename_failure_settings_local_no_temp_exit1_names_file` |
| S-1 | PASS | `S-1 spawned:` tests (4) |
| S-2 | PASS | invalid-override matrix (20) |
| S-3 | PASS (D-036 gap) | `test_s3_*` (5) |
| S-4 | PASS | check-zero-deps OK; trust-selfcontained pass |
| S-5 | PASS | grep of lib/test clean (conformance test only); `lib/atomic-write.js` absent |
| S-6 | PASS | S-6 tests (2) |

## Real-config hygiene (D-035)
- `~/.unimatrix/install.json` mtime: 02:00:30.748 before and after. Unchanged.
- `~/.claude.json` mtime: 02:26:31 before, 02:36:55 after. The content has no tmp/scratch project keys, no top-level `mcpServers`, and no `unimatrix` entry under `/workspaces/unimatrix`. The live Claude Code session rewrites this file routinely, and the tests did not write it.
