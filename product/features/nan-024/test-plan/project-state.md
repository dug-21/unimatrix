# Test Plan: project-state (`lib/project-state.js`)

Risks: R-28, R-34, R-20, R-23, R-31, R-35. Files: `test/project-state.test.js`, `test/project-parity.test.js`.

Shared fixture table (one JSON/JS module, consumed by shim-entry inline copy, project-state, `hook-client/config.js` hash, and the Rust store-dir hash via a recorded golden or `unimatrix hash` call on Linux CI): {root cwd, subdir, symlinked cwd, trailing slash, nested repos, Windows `C:\` vs `c:/`, store dir, `remote.json`, headless env with/without `UNIMATRIX_PROJECT_ROOT`, none}. All agree on hash and verdict. D-052 rows: root `/fx/repoA`, cwd `/fx/repoB`, `/fx/repoA-other`, `/`, `$HOME` -> null; `/fx/repoA/sub` -> `headless`; empty, relative, symlinked root.
`isUnimatrixProject(cwd, env, home)` returns `{hash, kind}` or null; Codex-only project (user-scope MCP + store dir, no project config) -> true (D-044/R-20).
`isInstalled(project, platform)`: Linux valid `install.json` or credential; darwin/win32 `UNIMATRIX_BUNDLE` or credential; (macOS/Windows, store dir, no bundle, no credential) -> `installed:false` (R-23 table rows 1,2,4); transport rules per D-032/D-042; Linux installed + binary unresolvable -> Broken (exit-1 path tested in launcher).
Only 16-hex store dirs considered; fixture HOME with 5 store dirs and no credential (R-31) -> not installed, no output.
