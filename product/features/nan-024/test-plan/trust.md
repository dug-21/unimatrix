# Test Plan: trust (`lib/trust.js`)

Risks: R-06, R-22, R-30. File: `test/trust.test.js` (POSIX cases skip win32; Windows cases run in CI `wiring-e2e` windows cell).

POSIX: group/other-writable `redirect.json`, `install.json`, shim, `~/.unimatrix` or `bin` not 0700, foreign-uid owner (simulated via injectable `getuid`) -> `TrustError{path,reason}`; hook exit 1 naming path; init under umask 002 then fire -> passes. Symlinked trust input resolved before check.
Ordering: outside a project with every input broken -> exit 0 silent, `require` trace shows trust module not loaded.
Windows (CI): full ACL check at init via `icacls` called by absolute `%SystemRoot%\System32\icacls.exe` with PATH stripped; extra-writer ACE -> exit 1; identity `{size,mtimeMs,ino}` match -> zero process spawns (trace-asserted, paths 2 and 3); mismatch -> one full check then `trust.json` rewritten, next event spawns nothing (R-22).
`trust.json` (R-30): group-writable/foreign-ACL, malformed, missing -> full recheck, never trusted; same size + restored mtime + different `ino` -> full check; two concurrent rewrites -> valid JSON; file changed inside `packageRoot` -> documented residual assertion (test names the residual).

## Rework: separable sections (pending H1)
### SEPARABLE: trust bootstrap via sibling `trust.js`
If kept: `checkTrust` is exercised through the installed sibling copy `~/.unimatrix/bin/trust.js` (not only `lib/trust.js`): same matrix gives identical verdicts from both copies (byte-equal files asserted); sibling absent -> exit 1 naming path inside a project, exit 0 silent outside (ordering unchanged); sibling replaced by a group-writable/foreign file -> refused before it is required (stat before require). If dropped by H1: remove this subsection; core trust matrix above stands.
