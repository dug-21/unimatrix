# shim (`lib/shim/unimatrix.sh`, `lib/shim/unimatrix.cmd`) — templates (sh + batch, packages/ only)

## Purpose
Templates rendered by install-store to `~/.unimatrix/bin/unimatrix` (0700) and `~/.unimatrix/bin/unimatrix.cmd`. Logic: [optional pre-Node check] -> recorded node -> PATH node -> loud hint -> run `shim-entry.js` with all args, stdin and exit code propagated. No per-version content: the hook string depends only on the shim path (D-028); upgrades rewrite shim content only.

## Scope of the pre-check (D-079; ADR-005)
The pre-check is a FILTER ONLY. It decides "start Node or exit 0 silently"; it never routes and never decides `kind`/transport. Clause (d) (ancestor `.git` regular file => start Node, for git worktrees) and the loosened headless clause (a) (any listed var non-empty AND `UNIMATRIX_PROJECT_ROOT` non-empty => start Node, no containment test) exist in the PRE-CHECK ONLY. Node's `isUnimatrixProject` keeps D-052 exactly (headless requires `UNIMATRIX_PROJECT_ROOT` to contain cwd; no `.git` clause). Consequence: the pre-check may start Node where Node then exits 0 silently (false positive, accepted); it must never exit 0 where Node would have found a project (false negative, forbidden).

## Render placeholders (install-store substitutes; values are pre-validated: sh path has no `'`; cmd path has no `%` or `"`)
`@@NODE@@` recorded absolute node; `@@UHOME@@` absolute `~/.unimatrix`; `@@REMOTE_VARS@@` space-separated names from `remote-resolve.VARS` (excluding HEADERS) plus `UNIMATRIX_BUNDLE`; `@@PRENODE_CALL@@` replaced by the early call line when `PRENODE_ENABLED`, else empty.

## unimatrix.sh
```
#!/bin/sh
NODE_REC='@@NODE@@'
UH='@@UHOME@@'

unimatrix_may_be_project() {
  # superset of isUnimatrixProject; shell builtins only (no subprocess). true => start Node.
  # (a) headless: any listed var non-empty AND UNIMATRIX_PROJECT_ROOT non-empty -> true (looser than Node's containment: superset, Node decides)
  [ -n "$UNIMATRIX_PROJECT_ROOT" ] && for v in @@REMOTE_VARS@@; do eval "x=\${$v}"; [ -n "$x" ] && return 0; done   // v from a fixed literal list; no user data evaluated
  cwd=$(pwd -P)         // builtin; physical path so a symlinked cwd matches a realpath'd marker (R-29)
  # (d) worktree: any ancestor containing a `.git` FILE -> true (Node resolves worktree to the main root; marker for main root may not contain cwd)
  d=$cwd; while [ -n "$d" ]; do [ -f "$d/.git" ] && return 0; d=${d%/*}; done
  # store dirs: only direct children of "$UH" named exactly 16 lowercase hex chars
  found_any=0
  for s in "$UH"/[0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f][0-9a-f]; do
    [ -d "$s" ] || continue            // unmatched glob stays literal: -d guards it (R-29)
    if [ ! -f "$s/root" ]; then return 0; fi                 // (c) legacy store without marker -> true
    IFS= read -r root < "$s/root" || true; root=${root%"$(printf '\r')"}   // strip CR; read -r, never eval/source
    case "$cwd/" in "$root"/*) return 0 ;; esac             // (b) path-component match: "$root"/ prefix of "$cwd"/
  done
  return 1
}
@@PRENODE_CALL@@        # when enabled: unimatrix_may_be_project || exit 0

if [ -x "$NODE_REC" ]; then NODE=$NODE_REC
else
  NODE=$(command -v node 2>/dev/null) || NODE=
  if [ -z "$NODE" ]; then
    unimatrix_may_be_project || exit 0         # ALWAYS runs on the node-not-found path (ADR-001/005): silent outside a project
    echo "unimatrix: node not found (recorded $NODE_REC); run 'npx unimatrix init'" >&2
    exit 1
  fi
fi
exec "$NODE" "${0%/*}/shim-entry.js" "$@"
```
Note: `$(command -v node)` and `$(pwd -P)` use command substitution (a subshell fork in some shells, no exec); keep on PATH-fallback and pre-check paths only; the common path (recorded node exists, pre-check disabled) has zero substitutions. If pre-check enabled, implementer measures; replace `$(pwd -P)` with `cwd=$PWD` + `pwd -P` via `cd -P . && cwd=$PWD` if substitution cost matters. `${0%/*}` avoids `dirname`. Single-quote embedding is safe because init refuses `'` in paths.

## unimatrix.cmd
```
@echo off
setlocal EnableExtensions DisableDelayedExpansion
set "NODE_REC=@@NODE@@"
set "UH=@@UHOME@@"
@@PRENODE_CALL@@           :: when enabled: call :may_be_project & if errorlevel 1 exit /b 0
if exist "%NODE_REC%" ( set "NODE=%NODE_REC%" & goto :run )
for %%N in (node.exe) do set "NODE=%%~$PATH:N"
if defined NODE goto :run
call :may_be_project
if errorlevel 1 exit /b 0
>&2 echo unimatrix: node not found (recorded %NODE_REC%); run 'npx unimatrix init'
exit /b 1
:run
"%NODE%" "%~dp0shim-entry.js" %*
exit /b %ERRORLEVEL%
```
`:may_be_project` (subroutine, `set /p` and `for` only, no subprocess): (a) headless vars + `UNIMATRIX_PROJECT_ROOT` defined -> errorlevel 0; (d) walk `%CD%` ancestors for a `.git` file (`if exist "%D%\.git" if not exist "%D%\.git\*"`); (b)/(c) for each `%UH%\` dir name of exactly 16 hex chars (for `/d` loop, length test via substring `%%~nxD:~15,1%` defined and `:~16,1%` empty, hex check via `for /f "delims=0123456789abcdef"` leaving nothing): missing `root` -> start Node; `set /p ROOT=<"%%D\root"`, trim CR, case-insensitive compare of `%CD%\` against `%ROOT%\` using `if /i "!CD_PREFIX!"=="%ROOT%\"` on a substring of equal length (delayed expansion enabled only inside this subroutine); `\` vs `/` normalised by replacing `/` with `\` in both. Root with spaces handled by quoting; `set /p` reads exactly one line. Residual: junction/symlinked cwd cannot be resolved without a subprocess (R-29 residual; pre-check stays disabled on Windows, D-088; ESCALATION TRIGGER: if the latency gate forces it on, escalate the junction/symlink gap to the primary rather than accepting it).

Exit-code propagation: `exit /b %ERRORLEVEL%` after the node call. Stdin: inherited by the child. `%*` preserves args including `--provider codex-cli`. Avoid parenthesised blocks around the node call (errorlevel/percent-expansion at parse time) and `pause`; no interactive prompt except the unavoidable Ctrl-C "Terminate batch job".

## Superset parity test (D-079; file `test/shim-precheck-superset.test.js`)
Runs the REAL rendered shim (`sh` template on POSIX CI; `.cmd` on the Windows job) with `PRENODE_ENABLED` forced on and `@@NODE@@` pointing at a stub node that writes a marker file and exits 0. For each fixture, also evaluate the Node predicate by importing the built `shim-entry.js` copy and `lib/project-state.js` (both must agree, R-28). Assertion per fixture: `isUnimatrixProject(cwd, env, home) != null  =>  marker file written (Node started)`. The converse is NOT asserted (false positives are allowed); a second table lists the expected strict-superset cases (worktree `.git` file with a non-containing marker; headless env + `UNIMATRIX_PROJECT_ROOT` not containing cwd) where the pre-check starts Node and Node's predicate returns null, and asserts the end-to-end result is still exit 0 silent with no output. Fixture matrix: {local store with root marker; remote.json store; headless valid (root contains cwd); headless without `UNIMATRIX_PROJECT_ROOT`; headless with cwd outside root; worktree cwd (`.git` file); nested subdir; prefix sibling (`/fx/repoA-other` vs `/fx/repoA`); legacy store without `root`; non-16-hex sibling dirs; none}. Failure of any implication fails the build.

## Error handling
Node missing + inside project -> stderr message, exit 1. Outside project (pre-check false) -> exit 0 silent. shim-entry failure codes pass through via `exec`/`exit /b`.

## Key test scenarios
Both shims via the literal written string under `sh -c`, `bash -lc`, `zsh -lc`, `fish -lc`, `cmd.exe /C "<line>"`, Git Bash; HOME with space and non-ASCII; recorded node deleted -> PATH node used; neither -> exit 1 inside project / 0 silent outside; stdin JSON reaches client; exit 1 visible to `cmd /C` and Git Bash; arg with spaces preserved. Pre-check table (R-29) {cwd=root, subdir, sibling shared prefix, case (Win), `\`/`/`, symlinked cwd, marker CRLF, root with spaces, project moved, no store dirs, worktree `.git` file, non-16-hex dirs ignored, legacy dir w/o `root`} x {sh,cmd}: verdict is "start Node" whenever Node predicate true.
