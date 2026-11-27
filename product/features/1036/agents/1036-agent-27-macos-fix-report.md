# 1036-agent-27-macos-fix report (rework under D-042)

## Cause
CI job 140492981678 (macOS, Package + opencode-plugin) failed 23 `not ok`, all `local init requires Linux`. The 20 tests behind them spawned local `init`. Nothing else failed on macOS.

## Fix (tests only; lib/ untouched)
- Each test is now classified by what it exercises. Shared-wiring tests are rewritten to spawn `init --bundle` against the wiring-e2e HTTPS stub and cert: the same `startStub`/`bundleFor`/`loadCert`, plus `stagedCli()`.
- A probe found that `init --bundle` writes `remote.json`, `root`, `install.json` and `bin/*`, and refuses a symlinked `trust.json` (install-store step 8). The refusal runs before the platform branch, so all 20 tests exercise shared behaviour. None is local-only.
- `test/helpers/e2e-harness.js` (cumulative) gains four helpers:
  - `initBundle(cli, s, cwd, home, {args, env, umask})`: async spawn, because the stub runs in-process (#4774). It isolates the env with tmp HOME/USERPROFILE, `gitEnv`, and `UNIMATRIX_*` stripped.
  - `useStub()`: a stub scoped to one describe.
  - `certSkip()`: skips when no cert is available, the same condition as wiring-e2e.
  - `offlineBundle()`: an `fp:null` bundle for `--dry-run`, which never connects.
- `test/helpers/linux-local.js`:
  - The `linuxLocal` skip reason now reads `personal-cloud goal (#5957): local mode is Linux-only; macOS/Windows are cloud-only`, not D-032.
  - Adds `winSymlink` and `winModes` (win32-only skips with reasons) and `anySkip`.
- `test/wire.test.js`: the three describes are rewritten as async, with `await init(...)`.
  - Assertions are unchanged, with one addition. `remote.json` now exists, so the drift test asserts its repair line, `Wrote credential store <p> (mode 0600)`. Under local init that file never existed and was silently skipped.
  - The stale "remote.json drift cannot be exercised" comment is replaced.
  - The ac10 `NODE_OPTIONS --require` path uses forward slashes, so Windows parses it.
- `test/status.test.js`: S-1 parity now runs `init --bundle <offline> --dry-run`, with no fake binary. The unused `fakeBinary` was removed.

## Mapping
| Test | Behaviour | Classification | Linux | macOS | Windows |
|---|---|---|---|---|---|
| test_ac3_project_link_inside_root_{relative,absolute,2-hop}_written_through_{real,dry-run} (6) | symlink-safe contained project writes | shared → --bundle | run | run | skip: symlink creation needs SeCreateSymbolicLinkPrivilege (EPERM) |
| test_ac3_project_link_outside_root_refused_real_and_dry_run | containment refusal | shared → --bundle | run | run | skip (symlink) |
| test_ac3_parent_dir_link_outside_root_refused | containment refusal | shared → --bundle | run | run | skip (symlink) |
| test_ac3_prefix_sibling_of_root_refused | containment refusal | shared → --bundle | run | run | skip (symlink) |
| test_ac3_dangling_project_link_refused_dangling_symlink | dangling-link refusal | shared → --bundle | run | run | skip (symlink) |
| test_ac3_root_reached_via_symlink_inside_target_followed | root realpath | shared → --bundle | run | run | skip (symlink) |
| test_ac3_trust_json_symlink_live_and_dangling_exact_message | trust.json symlink refusal (runs before the platform branch) | shared → --bundle | run | run | skip (symlink) |
| test_s3_drifted_store_modes_repaired_reported_updated_and_trust_check_passes | D-025 store modes (now includes remote.json) | shared → --bundle | run | run | skip: no POSIX mode bits (ACLs) |
| test_s3_new_install_modes_under_umask_{000,002,022} (3) | D-025 modes under umask | shared → --bundle | run | run | skip (modes) |
| test_ac4_rt07_codex_project_and_user_owned_removed_foreign_kept | removal set | shared → --bundle | run | run | run |
| test_s3_keep_mode_controls_unchanged_under_store_drift | keep-mode | shared → --bundle | run | run | skip (modes) |
| test_ac10_spawned_rename_failure_settings_local_no_temp_exit1_names_file | atomic write failure | shared → --bundle | run | run | run |
| S-1 parity: spawned init 'resolved:' paths equal collect().configDirs | config-dir resolution, status | shared → --bundle --dry-run | run | run | skip: existing `posix` (whole-file status convention, pre-existing) |

There are no local-only skips, and none cites an existing bundle test. The whole set is also skipped when no cert exists (openssl missing and E2E_CERT_DIR unset), which is the wiring-e2e convention.

## Results (Linux; clean env and adversarial env with CLAUDE_CONFIG_DIR/OPENCODE_CONFIG_DIR/XDG_CONFIG_HOME/CODEX_HOME set; identical)
| Set | Result |
|---|---|
| default | 978 pass / 0 fail |
| parity-layer2 | 24/0 |
| package | 974/2 (resolve-binary, waived #1055) |
| wiring-e2e | 42/0, plus the environmental `not ok 8` before-hook (pre-existing, see agent-26) |
| opencode-plugin | 37/0 |
| audit | exit 0 |
| size gate | OK, hook-client unchanged (16 B stripped headroom) |

The 20 rewritten tests pass 20/20 on Linux.

## Unverified (cannot run macOS/Windows)
- macOS: I expect these to pass. Wiring-e2e's `--bundle` path already passes there, and tmp paths are realpath'd (/private/var). Modes, umask via `sh`, and symlinks behave the same as on Linux.
- Windows: rt07 and ac10 run there.
  - ac10 assumes safe-write's rename target string equals `path.join(dir, ".claude/settings.local.json")`, and that EIO is not retried or masked on win32.
  - rt07 assumes the Codex TOML uses `\n`.
  - If either fails on the Windows cells, the fallback is a win32 skip with a reason. Those cells are the remaining risk.
- Linux no longer runs these 20 tests through the local transport (D-042 trade-off). Local init coverage stays in init-integration and init.

## Knowledge Stewardship
- Queried: context_briefing -- #5784 (macOS portability traps), #5788 (package-suite matrix), #5911 (tmp home), #5832 (CI-only failures).
- Stored: nothing novel to store yet -- the result is pending macOS/Windows CI. If it is green, store a pattern: "a refusal that runs before the platform branch (trust.json symlink) is shared behaviour; cover it via --bundle."

Unimatrix entries consulted: #5784, #5788, #5911, #5832
