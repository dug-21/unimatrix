# Test Plan: shim-entry (`lib/shim-entry.js`)

Risks: R-04, R-06, R-28, R-34, R-07. Files: `test/shim-entry.test.js`, shared `test/project-parity.test.js`.

- Order assertions via `require` trace: not-a-project -> zero package modules loaded, exit 0, empty stdout/stderr, even with broken trust inputs, stale `install.json`, missing node (R-06).
- Inside a project: trust check -> `isInstalled` -> `resolveRuntime` -> `require(<packageRoot>/bin/unimatrix.js)` in-process; one Node process per event (spawn trace).
- argv offset: `shim-entry.js hook Stop` -> launcher receives `['hook','Stop']`; `--provider codex-cli` forwarded; `mcp-stdio` forwards (R-04).
- Unhandled rejection / sync throw in client -> exit 1 + stderr, never 2; open socket handle does not hang (test with 5 s timeout).
- `require.main === module` guard: `require()`ing `hook-client/index.js` does not auto-run.
- Parity (R-28): test loads the inline predicate from the **built `shim-entry.js`** (not a re-export; extract via `vm`/subprocess `--print-project`), runs the shared fixture table (see project-state.md) and asserts equality with `lib/project-state.js`.
- `install.json` empty / extra fields / `packageRoot` deleted -> exit 1 inside project naming path.

## Rework (D-077, D-079, D-090)
- D-079: predicate stays D-052-strict in the inlined copy: parity table gains rows {worktree cwd with `.git` file and no store -> null, headless root var set + cwd outside root -> null}. Shim pre-check starting Node on these is allowed; Node result is authoritative.
- D-077: `shim-entry.js mcp-stdio` (Windows D-047 fallback entry) writes zero stdout bytes before the launcher spawns the Rust child (see launcher.md); covered by spawning the written Windows-form entry (argv `[node, shim-entry.js, 'mcp-stdio']`) on every OS via stub binary.
### Trust bootstrap via sibling `trust.js` (D-090)
- Installed `~/.unimatrix/bin/trust.js` exists after init, byte-identical to `lib/trust.js`; loaded (require trace) only after the project predicate is non-null, and before anything from `packageRoot`; a tampered sibling (group-writable or foreign-owned) -> exit 1 naming the path (stat before require).
- Sibling absent -> exit 1 naming path inside a project; exit 0 silent outside (ordering unchanged).
- Self-containment, assembled: copy only `shim-entry.js` + `trust.js` into an EMPTY temp dir (package dir and `install.json` packageRoot absent/deleted) and run the trust step there with a project fixture: trust check completes (verdict reached, TrustError only for real tampering), no `MODULE_NOT_FOUND`. Proves the shim can still trust-check when the package dir is gone.
- Shim path unchanged across upgrade (D-090/D-028 extension): see merge-settings.md and codex-install.md; hook command byte-equal before/after trust.js refresh.
