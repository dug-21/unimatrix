# Agent Report — vnc-027-agent-3-config-transport-selection

Component: config-transport-selection (`lib/hook-client/config.js`). Merge step 3.
Commit: `80af2d61` on `feature/vnc-027`.

## Files modified
- `/workspaces/unimatrix/packages/unimatrix/lib/hook-client/config.js`
- `/workspaces/unimatrix/packages/unimatrix/test/hook-client/config.test.js`

## What changed
- `resolve(cwd)` now returns `mode: "http" | "uds"`.
  - Env pair present -> `okHttp` (`mode:"http"`). Wins unconditionally; no probe (OQ1, FR-13).
  - `settings.local.json` valid `unimatrix.remote` -> `okHttp`.
  - No remote config (ENOENT / no remote key / incomplete key) -> `okUds` (`mode:"uds"`)
    with derived `socketPath`.
  - `partial_env` and `malformed` stay terminal. No HOME -> terminal `malformed`.
  - Terminal `{ok:false, reason:"missing"}` path retired.
- New `socketPathFor(projectHash)` -> `~/.unimatrix/{projectHash}/unimatrix.sock`,
  using the SAME `os.homedir()` + `projectHash` as `stateDirFor` (ADR-007 single
  derivation). Exported.
- `ok(...)` split into `okHttp(...)` (adds `mode:"http"`) and `okUds(...)`
  (`mode:"uds"`, `source:"local"`, `urlHost:""`).
- Doc comments updated (ADR-002/ADR-007). No `net`/network surface added — config.js
  stays a pure-string decision (the no-network unit-test guard still passes).

## Tests
- `node --test test/hook-client/config.test.js`: **48 pass, 0 fail, 1 skip**
  (the skip is the Windows-only `test_root_walk_windows_separators`, pre-existing).
- New tests added: mode matrix (env->http, settings->http, http-wins-with-live-socket,
  no-remote->uds-with-socketpath), missing-path-removed, partial_env/malformed terminal,
  no-HOME terminal malformed, no-daemon-not-terminal, single-derivation invariant
  (`dirname(socketPath)===dirname(stateDir)`, same-projectHash), `socketPathFor` null on
  no HOME. Three pre-existing `reason:"missing"` tests flipped to assert `mode:"uds"`.
- Hash-parity fixtures (TS-vs-Rust corpus) already covered by existing `GOLDENS` tests;
  the broader corpus + worktree/symlink/non-git hash family is owned by
  parity-corpus-uds.md, not duplicated here.

## Self-check / confirmations
- socketPath and stateDir share ONE derivation (`computeProjectHash(walkToProjectRoot(cwd))`);
  invariant `path.dirname(socketPath) === path.dirname(stateDir)` asserted. CONFIRMED.
- Mode selection: remote config -> http unconditionally, else uds. CONFIRMED.
- Size gate (Wave 1 in place) PASSES: totals stripped=68399/100000, raw=110832/160000.
  - config.js: stripped 4997 -> 5854 (+857), raw 7874 -> 10148 (+2274, mostly comments
    which the gate strips). The larger total raw jump vs. the 99,997 baseline is the
    parallel agent's new `transport-uds.js` (4269/8008) in the shared checkout, not my file.
- zero-deps gate PASSES (no runtime deps; built-ins only).
- Fail-open preserved: never throws; all os/fs calls wrapped; no secrets/output surface.

## Issues / blockers
- None functional. Shared-checkout git index.lock contention with parallel agents;
  resolved by waiting for the lock to clear before committing.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search -- surfaced ADR-007 (#4808),
  ADR-002 (#4803), hash-parity pattern (#4766: hash the normalized root, generate goldens
  from the Rust oracle), and the macOS symlink-alias trap (#4784). Applied: reused existing
  `walkToProjectRoot`/`computeProjectHash` verbatim; no second hash impl.
- Stored: entry #4823 "config.js transport selection: socketPath is a pure-string
  derivation; never probe/connect in config (no-network gate)" via context_store (pattern).
  Captures the no-network test guard trap, the corrected single-derivation invariant
  (test-plan form, not the off-by-one ADR prose), and the missing->uds flip.
