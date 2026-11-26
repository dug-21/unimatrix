# Test Plan: hook-client (`lib/hook-client/index.js`, `last-hook.json`)

Risks: R-04, R-32, R-01. Files: update `test/hook-client/index.test.js`; new `test/hook-client/last-hook.test.js`, `test/hook-client/main-export.test.js` (default hook-client set; runs on all OS x Node 18/20/22/24).
- `main(argv)` exported; `require()` does not auto-run; `require.main === module` still runs when invoked as script; fd-0 stdin path works on Windows (existing test retained).
- Same event via `node hook-client/index.js E`, `bin/unimatrix.js hook E` and the literal shim command -> identical stored record (stub server, async spawn #4774).
- async throw -> exit 1 (never 2); no hang with open sockets.
- `last-hook.json` (R-32): `{provider: ISO}` written under `~/.unimatrix/<hash>/`; 20 concurrent events from two providers -> valid JSON, both present (atomic tmp+rename or per-provider merge); read-only or missing `<hash>` dir -> event still stored, exit code unchanged; corrupt file overwritten safely; mode 0600 (POSIX skip win32); write cost <= 2 ms p95 in `benchmark-spawn`.
- Parity-layer1 replay and golden outputs unchanged; size gate (<100 KB) and zero-dep audit still pass.
- parity-layer2* (UDS, PreCompact, concurrency) run in the required CI job (AC-08).
