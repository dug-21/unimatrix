# 1036-agent-25-ci-env-fix report

Unimatrix entries consulted: #5911, #5788

## Cause
`test/helpers/launcher-fixture.js` `envFor()` copied `process.env` verbatim, so ambient `XDG_CONFIG_HOME` (set by GitHub runners) reached spawned `status` (R-09 / SR-10). Same leak in `e2e-harness.js` `baseEnv()`, four wiring-e2e test files (only `CODEX_HOME` was stripped), and in-process `initRemote`/`wire()` in init-remote / remote-client tests (read `process.env`).

## Fix (tests only, no assertion changes)
- `test/helpers/launcher-fixture.js`: `envFor` builds via `scrubbedEnv(...)`; overrides only via explicit (tmp-asserted) extras.
- `test/helpers/e2e-harness.js`: `baseEnv` starts from `scrubbedEnv()`.
- `test/codex-detect.test.js`, `test/init-preflight.test.js`, `test/mcp-stdio-spawn.test.js`, `test/pkg-binary-copy.test.js`: `Object.assign({}, process.env, …)` -> `scrubbedEnv(…)`.
- `test/init-remote.test.js`, `test/remote-client.test.js`: delete `OVERRIDE_VARS` from `process.env` at load (in-process callers).
- macOS: fixture bases are `realpathSync`'d, so `/private/tmp` assertions already match; no change needed.

## Results (node test/run-hook-client.js sets)
| set | adversarial env (before) | adversarial (after) | clean (after) |
|---|---|---|---|
| default | 978/0 | 978/0 | 978/0 |
| parity-layer2 | 24/0 | — | 24/0 |
| package | 969 pass, 2 fail (resolve-binary, waived #1055); init-remote/remote-client 33 fail in full run | 969/2 (resolve-binary only) | 969/2 (resolve-binary only) |
| wiring-e2e | 20 pass / 16 fail | 42/0 | 42/0 |
| opencode-plugin | 37/0 | — | 37/0 |
| audit | exit 0 | — | exit 0 |

wiring-e2e prints `not ok 8` (wiring-e2e-local before-hook: "refusing to overwrite existing node_modules/@dug-21/unimatrix-linux-x64") in both envs: local real platform package present; not env leakage, not counted as fail.

## Knowledge Stewardship
- Queried: context_briefing -- #5911 (inject tmp home+env, scrubbedEnv) directly applicable.
- Stored: nothing novel to store -- #5911 already covers it; the gap was helpers not applying it.
