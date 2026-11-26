# Test Plan: shim-entry (`lib/shim-entry.js`)

Risks: R-04, R-06, R-28, R-34, R-07. Files: `test/shim-entry.test.js`, shared `test/project-parity.test.js`.

- Order assertions via `require` trace: not-a-project -> zero package modules loaded, exit 0, empty stdout/stderr, even with broken trust inputs, stale `install.json`, missing node (R-06).
- Inside a project: trust check -> `isInstalled` -> `resolveRuntime` -> `require(<packageRoot>/bin/unimatrix.js)` in-process; one Node process per event (spawn trace).
- argv offset: `shim-entry.js hook Stop` -> launcher receives `['hook','Stop']`; `--provider codex-cli` forwarded; `mcp-stdio` forwards (R-04).
- Unhandled rejection / sync throw in client -> exit 1 + stderr, never 2; open socket handle does not hang (test with 5 s timeout).
- `require.main === module` guard: `require()`ing `hook-client/index.js` does not auto-run.
- Parity (R-28): test loads the inline predicate from the **built `shim-entry.js`** (not a re-export; extract via `vm`/subprocess `--print-project`), runs the shared fixture table (see project-state.md) and asserts equality with `lib/project-state.js`.
- `install.json` empty / extra fields / `packageRoot` deleted -> exit 1 inside project naming path.

## Rework (D-077, D-079; trust bootstrap PENDING H1)
- D-079: predicate stays D-052-strict in the inlined copy: parity table gains rows {worktree cwd with `.git` file and no store -> null, headless root var set + cwd outside root -> null}. Shim pre-check starting Node on these is allowed; Node result is authoritative.
- D-077: `shim-entry.js mcp-stdio` (Windows D-047 fallback entry) writes zero stdout bytes before the launcher spawns the Rust child (see launcher.md); covered by spawning the written Windows-form entry (argv `[node, shim-entry.js, 'mcp-stdio']`) on every OS via stub binary.
### SEPARABLE: trust bootstrap (sibling `trust.js`, pending H1)
If kept: assert `~/.unimatrix/bin/trust.js` exists after init, is the same bytes as `lib/trust.js`, is loaded (require trace) only after the project predicate is non-null, and a tampered sibling (group-writable or foreign-owned) -> exit 1 naming the path (sibling is itself a trust input). If H1 drops it: delete this subsection and the matching trust.md subsection; no other test depends on it.
