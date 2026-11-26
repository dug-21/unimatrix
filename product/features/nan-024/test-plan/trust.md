# Test Plan: trust (`lib/trust.js`)

Risks: R-06, R-22, R-30. File: `test/trust.test.js` (POSIX cases skip win32; Windows cases run in CI `wiring-e2e` windows cell).

POSIX: group/other-writable `redirect.json`, `install.json`, shim, `~/.unimatrix` or `bin` not 0700, foreign-uid owner (simulated via injectable `getuid`) -> `TrustError{path,reason}`; hook exit 1 naming path; init under umask 002 then fire -> passes. Symlinked `redirect.json` / `install.json` / shim (`lstat` is a symlink) -> REFUSED: `TrustError{path, reason:"symlink not allowed"}`, hook exit 1 naming path; assert refusal, not resolve-and-check (matches pseudocode/trust.md).
Ordering: outside a project with every input broken -> exit 0 silent, `require` trace shows trust module not loaded.
Windows (CI): full ACL check at init via `icacls` called by absolute `%SystemRoot%\System32\icacls.exe` with PATH stripped; extra-writer ACE -> exit 1; identity `{size,mtimeMs,ino}` match -> zero process spawns (trace-asserted, paths 2 and 3); mismatch -> one full check then `trust.json` rewritten, next event spawns nothing (R-22).
`trust.json` (R-30): group-writable/foreign-ACL, malformed, missing -> full recheck, never trusted; same size + restored mtime + different `ino` -> full check; two concurrent rewrites -> valid JSON; file changed inside `packageRoot` -> documented residual assertion (test names the residual).

## Rework: sibling trust.js (D-090; accepted, ADR-002 amended)
Files: `test/trust.test.js`, `test/trust-selfcontained.test.js`.
- Verdict parity: `checkTrust` exercised through the installed sibling `~/.unimatrix/bin/trust.js` AND `lib/trust.js`: same matrix, identical verdicts; files byte-equal after init.
- Self-containment (static): parse installed `trust.js`; every `require(...)`/`import` argument is a Node builtin (`node:`-prefixed or in `module.builtinModules`) or absent; zero relative/absolute path requires, no `require.resolve` of package paths, no dynamic `require(variable)`. Negative control: fixture copy with `require('./other')` appended -> check fails (test fails if the check cannot fail).
- Self-containment (dynamic): copy only `trust.js` into an empty temp dir, package dir absent (not on disk, `NODE_PATH` unset), `require` it and run the POSIX matrix subset -> same verdicts, no `MODULE_NOT_FOUND`.
- Atomic refresh (init/upgrade): written via tmp + rename in `bin/` (trace `fs` calls: write to `trust.js.tmp*` then `rename`, no in-place write to `trust.js`); fault injection kills the writer after the tmp write and before rename -> old `trust.js` intact byte-for-byte, still loads and verifies; stale tmp ignored/cleaned on next init; no partial/zero-byte `trust.js` observable by a concurrent reader (two processes: 50 loops reader vs refresher). After refresh bytes == package `lib/trust.js` (new version); mode 0600/0700 per install-store modes (POSIX), refreshed file passes its own trust check.
- Sibling replaced by group-writable/foreign file -> refused before required. Sibling absent -> exit 1 naming path in a project, exit 0 silent outside.
