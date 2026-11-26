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
