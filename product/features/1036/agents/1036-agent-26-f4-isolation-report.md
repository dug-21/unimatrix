# 1036-agent-26-f4-isolation report

Unimatrix entries consulted: #5911, #5955, #5832

## Task 1: Central env isolation (R-09)
- `test/helpers/home-guard.js`: loading the guard deletes ambient `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`, `XDG_CONFIG_HOME` and `CODEX_HOME` from `process.env` (`clearAmbientOverrides`, `AMBIENT_CLEARED`). A test that needs one of these vars sets it itself after load, or passes it through `scrubbedEnv(extra)`, which checks the path is under tmp.
- `test/run-hook-client.js`: requires home-guard once, so every spawned `node --test` set inherits the scrubbed env.
- Agent-25's per-file deletes are gone from `init-remote.test.js` and `remote-client.test.js`; both now just require the guard.
- Guard require added to every test file that read `process.env` without it: bridge-headless-root, config-dirs, bin-mcp-bridge, launcher-hook, resolve-runtime, compat-matrix, fire-time-parity, resolve-binary, wiring-e2e-local, headless-smoke, project-state, opencode-install, shim, shim-entry. The rest of the `process.env` readers (cli-routing, check-test-count, postinstall, git-exclude, trust-selfcontained, shim-superset) don't touch HOME or the four vars. They are still covered by the runner.
- `test/home-guard.test.js`: new tests. A child process starts with the adversarial env and requires the guard: all four vars are cleared and `AMBIENT_CLEARED` lists them. After load, an explicit tmp `XDG_CONFIG_HOME` and a `scrubbedEnv({CODEX_HOME: tmp})` both stick.

## Task 2: F4 (D-040)
- `lib/install-store.js`: on win32, an empty `recordTrustIdentity` result now throws `unimatrix: failed to write <trust.json>; trust state was not recorded. Check permissions on <dir> and re-run 'npx unimatrix init'`. That is a hard failure, exit 1, naming the file, per FR-53. The old `|| "updated"` fallback is removed. `trust.js` is unchanged, so the installed byte-identical copy rule still holds.
- `test/install-store.test.js` new tests:
  - simulated win32 failed write (stubbed trust plus icacls spawn) throws naming trust.json, and trust.json is absent afterwards;
  - on win32 the leg reports the writer's created/updated/unchanged result;
  - the real `recordTrustIdentity` returns undefined on a failed write (read-only dir, posix, skipped as root).
- No existing test was changed.

## Results (`npm test` sets)
| set | adversarial env | clean env |
|---|---|---|
| default | 978/0 | 978/0 |
| parity-layer2 | 24/0 | 24/0 |
| package | 974/2 (resolve-binary, waived #1055) | 974/2 (same) |
| wiring-e2e | 42/0, plus `not ok 8` before-hook (local platform package, environmental) | same |
| opencode-plugin | 37/0 | 37/0 |
| audit | exit 0 | exit 0 |

- Size gate passes, but headroom is only 16 B stripped and 217 B raw, measured against the current working tree. This change touches no `lib/hook-client/` code.
- `package.json`, the lockfile and `lib/trust.js` are unchanged.
- The clean-worktree check of wiring-e2e `not ok 8` was not run: it needs a built server binary and a fresh install, so it was not cheap.

## Knowledge Stewardship
- Queried: context_briefing -- #5911 (tmp home/env injection), #5955 ADR-005 (trust writer core in trust.js).
- Stored: nothing novel to store -- #5911 covers guard-based isolation. The F4 fix applies the existing FR-53 hard-failure posture.
