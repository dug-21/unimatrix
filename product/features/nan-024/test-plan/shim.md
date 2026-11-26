# Test Plan: shim (`lib/shim/unimatrix.sh`, `unimatrix.cmd`)

Risks: R-09, R-17, R-18, R-29, R-21, R-02, R-34(5). Files: `test/shim-sh.test.js` (POSIX, skip on win32), `test/shim-cmd.test.js` (skip unless win32), both in `wiring-e2e`.

## Unit/behaviour (real shim copied to temp `~/.unimatrix/bin`, stub `shim-entry.js` that echoes argv/stdin/exit)
- Recorded node used when present; deleted -> PATH node; neither -> exit 1 + stderr hint inside a project, exit 0 silent outside (R-09).
- Recorded node dir containing a space (both shims): runs, argv intact.
- Exit code of entry (0, 1, 7) propagates; stdin JSON payload reaches entry byte-exact (R-18); args with spaces preserved; `%*`/`&`/`^`/`(` in temp path either work or init refuses (R-17).
- `.cmd`: no "Terminate batch job (Y/N)" prompt (stdin closed/EOF), exit seen by `cmd /C` caller and by Git Bash; codepage with non-ASCII home.
- Not-a-project run: no Node spawned when pre-Node check enabled (spawn trace count 0), Node-spawn count 1 otherwise.

## Pre-Node root-marker check (R-29, only if ADR-005 trigger enabled)
Table {cwd=root, subdir, sibling `/fx/repoA-other` vs root `/fx/repoA`, different case (win), `\` vs `/`, symlinked cwd, marker with CRLF, root with spaces, project moved, empty glob (literal `*`), legacy dir without `root`, non-16-hex dir} x {sh, cmd}. Assert: shell verdict "start Node" whenever `isUnimatrixProject` is true (lossless); property test over random fixtures: shell-miss => Node-false. Marker content never evaluated (marker containing `$(touch pwn)` creates no file).
Headless env: cwd outside `UNIMATRIX_PROJECT_ROOT` -> no Node (or Node sends nothing).

## Shell matrix (R-02, R-17) in `wiring-e2e`
Literal written command via `sh -c`, `bash -lc`, `zsh -lc`, `fish -lc` (skip if not installed, log), `cmd.exe /C "<line>"` with Codex outer-quote wrapping, Git Bash; HOME/USERPROFILE with a space and non-ASCII. Expected: exit 0/1 per scenario, observation stored.

## Rework (D-079): pre-Node check clauses (d) and loosened headless; MANDATORY superset test
Clauses implemented in the shim (builtins only): (a) headless env set and `UNIMATRIX_PROJECT_ROOT` non-empty (no containment test in shell), (b) cwd equal/inside a `root` marker, (c) store dir without `root`, (d) any ancestor of cwd has `.git` as a regular FILE (worktree/submodule). Else exit 0 without Node.
- **Superset test (MANDATORY, `test/shim-superset.test.js`, in `wiring-e2e`, POSIX; `.cmd` twin on win32)**: real `unimatrix.sh`/`.cmd` copied to temp `~/.unimatrix/bin`, stub node = `process.execPath` script that appends one line to a spawn log and exits 0, recorded in `install.json`. Fixture matrix: {local store with `root`, `remote.json`, legacy store without `root`, headless valid (`UNIMATRIX_PROJECT_ROOT` contains cwd), headless without root var, headless cwd outside root, headless root var set + cwd outside (shell may start Node: clause a; Node says null), worktree cwd (`.git` file in ancestor), nested subdir of project, prefix sibling `/fx/repoA-other`, symlinked cwd, root with spaces, none}. For each fixture compute `isUnimatrixProject(cwd, env, home)` from `lib/project-state.js`; assert `verdict != null  =>  spawn log has 1 line`. The reverse is NOT asserted (false positives allowed: shell only filters). Negative: `none` fixture and prefix sibling with no `.git` file and no markers -> 0 spawns.
- Property variant: 200 seeded random fixtures (seed printed on failure); any case with verdict non-null and 0 spawns fails with fixture dump.
- Clause (d) cases: `.git` regular file in cwd, in grandparent; `.git` directory (normal repo) alone does NOT trigger (0 spawns when nothing else matches); `.git` file with CRLF/garbage content: not evaluated, still triggers; symlink named `.git`: documented outcome asserted.
- Node predicate unchanged (D-052 strict): same fixtures run end to end through shim -> real `shim-entry.js`: headless root var set + cwd outside root -> Node started, exits 0 silent, recording stub endpoint receives 0 requests; worktree cwd with no store -> Node started, exit 0 silent (clause d is a filter only, does not make it a project).
- Markers/`.git` content never evaluated (`$(touch pwn)` content creates no file).
