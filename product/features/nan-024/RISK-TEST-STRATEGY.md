# Risk-Based Test Strategy: nan-024

Revision 6. Covers decisions D-015..D-068; where entries conflict, the later one wins (D-023>D-015, D-026>D-018/D-019, D-029>D-020, D-046 supersedes the R5 waivers, D-050 amends D-037, D-052 amends D-045, D-060 amends D-026 MCP placement). Architecture final: Limitations "None open".

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---|---|---|---|---|
| R-01 | Exit outcomes (D-023/D-042). Residual: any path exits 2 (uncaught throw, `process.exitCode` leak); hint written to **stdout** (Claude injects exit-0 stdout into context); connection errors re-mapped to 1 contrary to D-042 | High | Low | Medium |
| R-02 | Codex shell (evidence + D-027). Residual: non-POSIX `$SHELL` (fish, nu); PowerShell unsupported, documented (D-041) | Med | Low | Low |
| R-03 | Ownership predicate misses a variant (absolute shim single/double quoted, `/` or `\`, `.cmd`, `--provider`, space in path, future event) → duplicates; or matches foreign entry | High | High | Critical |
| R-04 | In-process client via shim-entry → launcher → `main(argv)`: argv offset, unhandled rejection, open handles, `require.main` guard → exit 0 unhandled, or hang | High | Med | High |
| R-05 | Override source (D-024). Residual: client read from env or beside binary; redirect with only `binary` moves client | Med | Low | Low |
| R-06 | Trust (D-029, ADR-002 ordering: predicate before trust). Residual: inline predicate loads package code; tamper of `shim-entry.js` undetected (same-user, accepted) | High | Low | Medium |
| R-07 | `mcp-stdio`: stdout byte before exec corrupts JSON-RPC; predicate/transport computed from a different cwd than the project | High | Med | High |
| R-08 | User-level rewrites (`~/.claude/settings.json`, `~/.claude.json`, `~/.codex/hooks.json`, `~/.codex/config.toml` `[hooks]` and `[mcp_servers]`, `~/.config/opencode/opencode.json`): reformat, foreign loss, TOML comment loss, non-atomic write, dry-run ≠ actual | High | Med | High |
| R-09 | Node path (D-030). Residual: recorded path with spaces mis-quoted in `.cmd`/sh; node-not-found outside a project prints instead of silent 0 | Med | Med | Medium |
| R-10 | Launcher `hook` route breaks pass-through (`LD_LIBRARY_PATH`) or pass-through ignores override | Med | Low | Medium |
| R-11 | Latency benchmark measures warm in-process path rather than the literal command through the harness shell cold; `mcp-stdio` start (now in every Codex/OpenCode session) not measured | Med | Med | Medium |
| R-12 | Local vs cloud identity per OS family (AC-01, D-040) broken by an incidental field | Med | Low | Medium |
| R-13 | Old-string tests deleted rather than updated; parity-layer2 advisory | Med | Med | Medium |
| R-14 | OpenCode removal leaves partial provisioning or mutates `plugin[]` | Low | Med | Low |
| R-15 | `resolve-binary.js` wrapper changes semantics (throws vs null; null normal on macOS/Windows) | Med | Low | Low |
| R-16 | Legacy direct-path Rust `hook` outside init-written files. Gated by Stage 1 audit (D-035); residual: audit misses published artifacts (npm tarball, OpenCode plugin dist) | Med | Low | Low |
| R-17 | Absolute shim path quoting under `sh -c`, `$SHELL -lc` (bash/zsh/fish), `cmd.exe /C` with Codex outer quotes, Git Bash; refused chars (`'`, `%`, `"`) not refused; `&^(` and non-ASCII on Windows | High | Med | High |
| R-18 | `.cmd` shim: exit code not propagated, stdin payload lost, `%*` mangling, "Terminate batch job" prompt, codepage | High | Med | High |
| R-19 | Codex approval lost. **Mitigated by D-038** (`status` + `last-hook.json` + printed step). Residual: a hook that fired once and is later un-trusted shows only a stale timestamp, not a warning; server-side signal is a follow-up | High | Low | Medium |
| R-20 | Project detection false negative. **Closed by D-044/D-045** (predicate independent of config location). Parity risk moved to R-28 | Med | Low | Low |
| R-21 | Pre-Node check cost: not-a-project path over budget; walk/glob over many `~/.unimatrix/*` dirs; legacy store dirs without `root` force Node on every event | Med | Med | Medium |
| R-22 | Windows trust cost. **Mitigated by D-036** (full ACL at init, per-event identity compare). Residual: first event after any identity change pays the `icacls` spawn; `icacls` absent from Codex-stripped PATH; integrity of `trust.json` → R-30 | Med | Med | Medium |
| R-23 | macOS/Windows project with no `UNIMATRIX_BUNDLE` and no credential. **Resolved by D-050**: hook exit 0 silent; reported only in `status`. Linux with no valid `install.json` and no credential keeps the one-line hint (D-023). Residual: the two platform outcomes swapped; hook exits 1; `status` misses it | Med | Low | Low |
| R-24 | Stable copy: Windows `rename` over open `install.json` (EPERM/EBUSY); prune removes a version an in-flight hook loads; concurrent init; interrupted copy | Med | Med | Medium |
| R-25 | Windows MCP argv re-escaped by the harness spawner. **Gated by D-034, fallback D-047; placement resolved by D-060** (per-machine entry, rewritten on node re-record, D-057). Residual: a stale node path after node removal without re-init gives a harness ENOENT at spawn (accepted by D-066, amends D-057); `status`/preflight report it | Med | Low | Low |
| R-26 | D-033 CI matrix: macOS/Windows jobs prove an emulated runner rather than the literal command through the harness shell; HTTPS test CA not trusted by Node; flaky jobs made advisory; space-in-home case dropped | High | Med | High |
| R-27 | **Closed (D-046, D-039).** R5 dropped; init does not edit `.gitignore` and warns instead. Residual: warning not emitted in dry-run, or init edits `.gitignore` anyway | Low | Low | Low |
| R-28 | **Predicate parity.** `isUnimatrixProject` is implemented three times: inlined in `shim-entry.js`, in `lib/project-state.js`, and in the `hook-client/config.js` hash semantics (plus the Rust server's store-dir hash). Any drift in `walkToProjectRoot`, path normalisation (symlink, trailing slash, Windows case/drive letter) or hash → hooks silent in a real project, or events stored under a different hash than MCP reads | High | Med | High |
| R-29 | **Pre-Node root-marker check lossy.** Shell prefix-match of cwd against `~/.unimatrix/*/root`: case-insensitive Windows paths, `/` vs `\`, symlinked cwd, trailing newline/CR in marker, `root` with spaces under `set /p`, project moved after init, glob with no matches literal `*` in sh. Any miss silences a D-045 project — contradicts the "lossless superset" rule | High | Med | High |
| R-30 | **`trust.json` staleness/integrity (Windows).** `trust.json` itself not in the checked set, so a same-user-writable cache could whitelist a tampered file; `mtimeMs` resolution/`ino` reuse after replace yields false match; upgrade replaces files without refreshing the cache, so every event re-runs `icacls` until the rewrite succeeds; concurrent rewrites corrupt JSON; directory identity (`packageRoot`) does not change when files inside change | Med | Med | Medium |
| R-31 | Noise in every project. **Resolved by D-050** (no hook hint) and D-052 (headless env counts only inside `UNIMATRIX_PROJECT_ROOT`). Residual: some other hook path still writes stderr in non-attached projects | Low | Low | Low |
| R-32 | **`last-hook.json` write per event.** Concurrent events from two providers clobber the JSON; partial write corrupts it so `status` misreports; write failure changes the exit code or adds latency; file written outside the trusted dir mode | Med | Med | Medium |
| R-33 | Cloned project, init never run. **Closed by D-060**: no committed Unimatrix MCP entry exists, so a cloner's harness spawns nothing; `status` reports not attached (D-051) | Low | Low | Low |
| R-34 | **Cross-repo leak (D-052).** With `UNIMATRIX_BUNDLE`/`UNIMATRIX_REMOTE_*` exported, an event or MCP call from a cwd outside `UNIMATRIX_PROJECT_ROOT` reaches the cloud endpoint: predicate copies (R-28) drift on the new rule; prefix match treats `/a/foo` as inside `/a/foobar` root or vice versa; relative/unnormalised/symlinked `UNIMATRIX_PROJECT_ROOT`; `mcp-bridge` (headless entry point) or the pre-Node check honours the env without the root test; `UNIMATRIX_PROJECT_ROOT` set but empty treated as `/` | High | Med | High |
| R-35 | **Headless contract regression (D-053).** vnc-050 headless setups now need `UNIMATRIX_PROJECT_ROOT`: `docs/client-setup.md` and `headless-smoke.test.js` not updated, or updated so the smoke test still passes without the variable; a missing variable makes every directory "not a project" (D-052), so the headless setup goes silent with no diagnostic (accepted by register) | Med | Med | Medium |
| R-36 | **Removal preserving foreign servers** (D-060 migration). init removes owned Unimatrix entries from project `.mcp.json`, `.codex/config.toml` and `opencode.json` `mcp.unimatrix` (D-064): deletes a foreign server, reorders/reformats the file, drops TOML comments, rewrites a file with nothing owned, or leaves an empty `mcpServers`/file in a state the harness rejects | High | Med | High |
| R-37 | **Concurrent writes to `~/.claude.json`.** Claude Code rewrites this file constantly (history, project state); init's read-modify-write loses Claude's update or Claude's write discards ours; partial write truncates the file, losing all Claude state | High | Med | High |
| R-38 | **`projects[root]` key mismatch.** Entry written under a key Claude does not look up (symlink vs realpath, trailing slash, Windows drive-letter case or `\`/`/`, nested git root vs cwd), so the Unimatrix MCP silently never loads in that project | High | Med | High |
| R-39 | **User-scope MCP in every project.** Codex `~/.codex/config.toml` and OpenCode user config start `mcp-stdio` in every session in every directory: must exit 0 silent and fast outside a D-045/D-052 project; startup cost or a harness "MCP server failed" notice in every non-Unimatrix session | Med | High | High |
| R-40 | **Migration edge cases.** A foreign entry at the `unimatrix` key (project or per-machine) overwritten, or left without the D-065 warning; legacy project-scope Unimatrix entries left behind (init not re-run) not detected by `status`; legacy project entries live alongside the per-machine entry → two Unimatrix MCP servers until init re-runs | Med | Med | Medium |

## Risk-to-Scenario Mapping

### R-01: Exit outcomes
**Scenarios**: (1) Each Claude event class × {not project, not attached, each resolution failure, client throws sync/async}: exit ∈ {0,1} per FR-12, never 2. (2) Not attached: stdout empty, exactly one stderr line. (3) Not a project: stdout and stderr empty. (4) Daemon down / HTTPS failure after resolution: existing hook-client codes unchanged (D-042).
**Coverage**: full FR-12 matrix on all three OSes.

### R-02: Codex shell
**Scenarios**: Literal Codex command via `bash -lc`, `zsh -lc`, `fish -lc`, `/bin/sh -c`; Windows `cmd.exe /C "<line>"` with Codex wrapping. PowerShell documented (D-041).

### R-03: Ownership predicate
**Scenarios**: (1) Positive table: L1–L6, `$HOME`/`~` shim, absolute shim single/double quoted, `/` and `\`, `.cmd`, space, `--provider codex-cli`, every KnownEvent. (2) Negative: `my-unimatrix-hook.sh`, `echo unimatrix hook`, `unimatrix-backup hook X`. (3) Property: `isUnimatrixCommand(buildShimCommand(e,p,platform))` for all e, p, platform, home incl. spaces.

### R-04: In-process client
**Scenarios**: Same event via `node hook-client/index.js E`, `bin/unimatrix.js hook E` and the literal shim command gives an identical stored record; async throw → exit 1; no hang on open sockets; `require` does not auto-run.

### R-05: Override source
**Scenarios**: `UNIMATRIX_BINARY` set → clientSource unchanged; redirect with only `binary` → client from install.

### R-06: Trust
**Scenarios**: Inside a project: group-writable redirect/install.json/shim → exit 1 naming path; foreign-owned `~/.unimatrix` → exit 1; init under umask 002 then fire → works. Outside any project with every trust input broken → exit 0, no output, `require` trace shows no package module loaded.

### R-07: MCP stdio
**Scenarios**: Spawn written MCP entry (Linux local, all-OS cloud), send `initialize`, first stdout byte `{`; launch from subdir cwd; one process after exec; inside project not installed → exit 1 + stderr, stdout empty; outside project → exit 0 silent (D-043).
 D-043 per OS: inside a project with no install on Linux, macOS and Windows, the written MCP entry exits 1 with one stderr hint and empty stdout; outside a project it exits 0 silent.
### R-08: User-level writes
**Scenarios**: Foreign hooks, extra keys, custom indent, TOML comments preserved; owned-free files byte/mtime unchanged; temp+rename; malformed JSON/TOML aborts untouched; `--dry-run` list equals files changed by the real run.

### R-09: Node path
**Scenarios**: Recorded node in a dir with a space (both shims); recorded path deleted → PATH fallback; neither → exit 1 inside a project, exit 0 silent outside; upgrade re-records.

### R-10 – R-15
- R-10: stub binary records argv and `LD_LIBRARY_PATH` for `serve`/daemon under each override source.
- R-11: benchmark drives the literal command through the harness shell cold, 50 iterations/event class; reports absolute p95 and baseline; fails on breach.
- R-12: byte-compare whole written files, local vs cloud, per OS family.
- R-13: CI test-count diff on touched suites; parity-layer2 is a required check.
- R-14: `.opencode/` before/after: byte-equal `plugin[]`, no new files.
- R-15: existing `resolve-binary` tests pass; macOS/Windows return null without throw.

### R-16: Legacy direct-path
**Scenarios**: Stage 1 audit output (D-035) covers repo, docs, installers **and** the packed npm tarball and OpenCode plugin build; after init, fixture scan finds no L2; process trace on a fired event shows zero Rust exec.
 The repo `.codex/hooks.json` reference config is exempt from Stage 1 escalation (D-058, D-048 → #1025); the audit report lists it as exempt, not as a finding.
### R-17: Shim path quoting
**Scenarios**: Temp HOME/USERPROFILE with a space and non-ASCII; literal command via `sh -c`, `bash -lc`, `zsh -lc`, `fish -lc`, `cmd.exe /C` (Codex wrapping), Git Bash; observation stored. init with `'` (Unix) or `%`/`"` (Windows) in path → exit 1. `&`/`^`/`(` on Windows → works or refused.

### R-18: `.cmd` shim
**Scenarios**: Resolution failure → exit 1 seen by `cmd /C` caller and Git Bash; hook JSON on stdin reaches client; args with spaces preserved; no interactive prompt.

### R-19: Codex approval
**Scenarios**: (1) init, simulated upgrade, re-init from another cwd: `~/.codex/hooks.json` byte-identical per platform (D-028); re-init over a current entry writes nothing. (2) init output contains the approval step. (3) `status` with Codex wired and no `codex-cli` entry in `last-hook.json` prints the approval step; with an entry prints its time; exits 0. (4) Driving the literal Codex command records `codex-cli` in `last-hook.json`.

### R-20: Detection independent of config
**Scenarios**: Codex-only project with MCP in user scope and a store dir → predicate true, event stored (D-044).

### R-21: Pre-Node check cost
**Scenarios**: Not-a-project p95 on all three OSes with 0, 1 and 50 store dirs; legacy dir without `root` → Node starts (measured, counted); zero Node processes on a miss when enabled.

### R-22: Windows trust cost
**Scenarios**: Windows paths 2 and 3 p95 with matching identity (no process spawn, trace-asserted); first event after upgrade (mismatch → full check once, then `trust.json` rewritten, next event spawns nothing); `icacls` invoked by absolute `%SystemRoot%\System32` path with PATH stripped; extra-writer ACE → exit 1.

### R-23: Bundle, no credential
**Scenarios**: One table, both platforms. (1) macOS/Windows, store dir present, no `UNIMATRIX_BUNDLE`, no credential → hook exit 0, stdout and stderr empty (D-050); `status` reports it. (2) Linux, store dir present, no valid `install.json`, no credential → hook exit 0 with exactly one stderr hint line (D-023). (3) Linux, valid `install.json`, local store present, binary missing → exit 1 Broken, stderr names binary path (ADR-004). (4) Credential present (any OS) → cloud, stored. (5) Pass-through on macOS/Windows → Linux-only message, exit 1; binary stub never touched.

### R-24: Stable copy
**Scenarios**: Upgrade while a hook holds `install.json` open (Windows) → swap succeeds or retries; kill copy mid-way → pointer on previous version; prune keeps current+previous; two concurrent inits → one consistent pointer.

### R-25: Windows MCP argv
**Scenarios**: D-034 gate: Windows CI spawns the written per-machine MCP entry through Claude Code's and Codex's actual spawners with USERPROFILE containing a space; `initialize` succeeds. The same job runs the D-047 fallback form (recorded node path in `Program Files` + `shim-entry.js mcp-stdio`). D-057: change the recorded node path and re-init → fallback entry in `~/.claude.json`/`~/.codex/config.toml` rewritten with the new path, otherwise byte-identical. Delete the recorded node without re-init → `status` prints the stale path and re-init hint, exit 0; init/upgrade preflight exits 1 with re-init hint; harness spawn error recorded, not asserted beyond "no Unimatrix process ran".

### R-26: CI matrix
**Scenarios**: CI config check: macOS and Windows jobs exist, are required, read the literal command from the hook destination and run it through the harness shell, include a space-in-home case, and pass a test CA via `NODE_EXTRA_CA_CERTS`.

### R-27: gitignore
**Scenarios**: Fixture where `.claude/settings.local.json` is not ignored: init and `--dry-run` print the warning; `.gitignore` bytes unchanged.

### R-28: Predicate parity
**Scenarios**: One shared fixture table {root cwd, subdir, symlinked cwd, trailing slash, nested repos, Windows drive-letter case, `\` vs `/`, store dir / `remote.json` / headless env / none}: `shim-entry.js` inline predicate, `lib/project-state.js`, `config.js` hash and the Rust server's store path all agree (same hash, same verdict). The test imports the inline copy from the built `shim-entry.js`, not a re-export. Fire a hook from a subdir, then MCP search: the event is visible (same store).

### R-29: Root-marker pre-check
**Scenarios**: Table {cwd = root, subdir, sibling with shared prefix, different case (Windows), `\`/`/`, symlinked cwd, marker with CRLF, root with spaces, project moved after init, no store dirs (empty glob)} × {sh, cmd}: shell verdict is "start Node" whenever the Node predicate is true (lossless); a miss on a D-045 project fails the test. Property: for random fixtures, shell-miss implies Node-false.

### R-30: trust.json
**Scenarios**: (1) `trust.json` group-writable/foreign-ACL → treated as missing (full check), or refused, never trusted. (2) Replace a checked file with same size and restored mtime → `ino` differs → full check. (3) Upgrade → first event full check, cache rewritten; later events no spawn. (4) Two concurrent events rewriting the cache → valid JSON after. (5) File changed inside `packageRoot` → behaviour documented (directory identity residual).

### R-31: Noise in every project
**Scenarios**: Fixture HOME with 5 store dirs, credentials removed and no `UNIMATRIX_BUNDLE` (macOS/Windows): Codex literal command in each → exit 0, stdout and stderr empty, in budget.

### R-32: last-hook.json
**Scenarios**: 20 concurrent events from two providers → valid JSON, both providers present; read-only/missing `<hash>` dir → event still stored, exit unchanged; corrupt file → `status` reports "unknown", does not crash; file mode 0600 (POSIX) / inside ACL'd dir (Windows); write adds ≤ 2 ms p95.

### R-33: Cloned project, init never run
**Scenarios**: Clone a fixture repo that was initialised elsewhere: tracked files contain no Unimatrix MCP entry (grep `.mcp.json`, `.codex/config.toml`); on the clone machine no `projects[root]` Unimatrix entry exists; `unimatrix status` reports not attached and names `npx unimatrix init`.

### R-36: Removal preserving foreign servers
**Scenarios**: Fixture `.mcp.json`, `.codex/config.toml` and `opencode.json` each with {Unimatrix only, Unimatrix + 2 foreign servers, foreign only, comments/custom indent, legacy L-forms}: after init, every foreign server is semantically identical, file order kept, TOML comments kept; foreign-only files byte/mtime unchanged; Unimatrix-only file left valid (empty `mcpServers` / table) and accepted by the harness's parser; `--dry-run` lists each removal; malformed file → abort untouched.

### R-37: Concurrent `~/.claude.json` writes
**Scenarios**: Concurrent-writer simulation: a writer process rewrites `~/.claude.json` (tmp+rename, as Claude does) every few ms while init runs 50 times → final file parses, contains the writer's latest keys and exactly one `projects[root].mcpServers.unimatrix`; init uses tmp+rename and touches only that key (other keys byte-equal in re-serialised form); parse failure → init aborts, file untouched, exit 1 with message; kill init mid-write → original file intact.

### R-38: `projects[root]` key
**Scenarios**: Fixture table {plain root, symlinked root, trailing slash, subdir cwd, nested repo, Windows `C:\` vs `c:/`}: key written equals the key Claude Code uses (compare with a key Claude writes itself in CI, or its documented normalisation); spawn Claude with cwd in each → Unimatrix MCP listed and `initialize` succeeds.

### R-39: User-scope MCP everywhere
**Scenarios**: Codex and OpenCode with user-scope entry, cwd outside any project (incl. headless env with cwd outside `UNIMATRIX_PROJECT_ROOT`) → `mcp-stdio` exits 0, empty stdout/stderr, zero network requests; ADR-005 benchmark extended: `mcp-stdio` start-to-exit p95 on the not-a-project path ≤ hook budget on all three OSes, and start-to-`initialize`-response inside a project measured and reported. Record what each harness displays for a 0-exit MCP server (notice text in CI log).

### R-40: Migration
**Scenarios**: (1) Foreign server named `unimatrix` (command not matching the predicate) in `.mcp.json` or user scope → left byte-identical, never overwritten; init prints a warning naming file and key; no Unimatrix entry written in its place (D-065). (2) Leftover owned Unimatrix entry in each of project `.mcp.json`, `.codex/config.toml`, `opencode.json` (init not re-run) → `status` prints one `legacy:` line per file naming it, and its JSON output carries `legacyProjectMcp: string[]` listing exactly those files (D-068). (3) After re-init → no `legacy:` lines, `legacyProjectMcp` empty, only the per-machine entry remains.

### R-34: Cross-repo leak
**Scenarios** (all from the real entry point, against a recording test HTTPS endpoint): (1) Export `UNIMATRIX_BUNDLE` + `UNIMATRIX_REMOTE_*` + `UNIMATRIX_PROJECT_ROOT=/fx/repoA`; run the literal Codex command from `~/.codex/hooks.json` with cwd `/fx/repoB`, `/fx/repoA-other` (shared prefix), `/`, `$HOME` → endpoint receives zero requests, exit 0 silent. (2) Same with cwd `/fx/repoA/sub` → one event received. (3) Spawn the written MCP entry (`mcp-stdio`) and `npx -y @dug-21/unimatrix mcp-bridge` from cwd `/fx/repoB` → not a project (D-045 as amended by D-052): exit 0, no stdout/stderr, endpoint receives zero requests (D-043, AC-15). (4) Headless env without `UNIMATRIX_PROJECT_ROOT`, with an empty value, with a relative value, and with a symlinked root → no request from outside. (5) Pre-Node check with headless env: cwd outside root → no Node started (or Node starts and sends nothing). (6) Parity table (R-28) extended with the D-052 rows.

### R-35: Headless contract
**Scenarios**: `headless-smoke.test.js` fails when `UNIMATRIX_PROJECT_ROOT` is removed from its env (negative case added) and passes with it; `docs/client-setup.md` lists the variable (grep test); with headless env but no `UNIMATRIX_PROJECT_ROOT`, hook and `mcp-bridge` exit 0 silent with zero requests.

## Behavioral-Outcome Coverage

| Outcome | Entry point | Required scenario |
|---|---|---|
| Hooks fire, observation stored with correct provider, no Rust hook process | `npx unimatrix init` local (Linux) | Clean fixture + temp HOME: init, read command from `settings.local.json` and `~/.codex/hooks.json`, run through harness shell per event; store shows observation + provider; zero Rust exec |
| Entries identical per OS family; events stored over HTTPS | Cloud attach (Linux, macOS, Windows) | Attach against test HTTPS endpoint; byte-compare with local (Linux); fire literal command via `sh -c` / `cmd.exe /C` / Git Bash; server stores it (D-033) |
| One entry per event across scopes; foreign intact | Re-run init/wire over legacy | Seed L1–L6 in every Claude/Codex scope + foreign; run sequences; one in destination, zero elsewhere; foreign diff empty |
| Stored, or exit 1 with visible stderr; never silent in a project | Harness-fired hook (Claude, Codex) | Literal command through harness shell; break each resolution input → exit 1 + stderr, never 2; fire from subdir and symlinked cwd of a D-045 project → stored (R-28, R-29) |
| Silent, in budget, in non-Unimatrix repos | Codex user-level hook / `mcp-stdio` in unrelated directory | cwd outside any D-045/D-052 project incl. stale install, missing node, broken trust: exit 0, no output, p95 in budget, all three OSes |
| Lost Codex approval is discoverable | `unimatrix status` | Wire Codex without firing → `status` prints approval step; fire literal Codex command → `status` shows last-fired time |
| Missing credential reported, not broken | Hook / `status` on macOS/Windows | No `UNIMATRIX_BUNDLE`, credential removed: hook exits 0 silent; `status` reports it (D-050) |
| No cross-repo data with headless env exported | User-level Codex hook, `mcp-bridge`, written MCP entry | R-34 scenarios 1–3: exported env, cwd outside `UNIMATRIX_PROJECT_ROOT` → recording endpoint gets zero requests; inside root → stored |
| Cloned repo without init is reported | `unimatrix status` | R-33 scenario (D-051) |
| Handled by Node and stored | Legacy `unimatrix hook E` in config | L1 command with launcher on PATH: fire, stored, Rust stub not invoked |
| Works or fails loud | Cache clean / reinstall / upgrade | npx install, init, wipe `~/.npm/_npx`, fire → works; delete `pkg/<version>` → exit 1; upgrade → works, Codex string byte-identical |
| Both hook and MCP use that binary; client unchanged | `UNIMATRIX_BINARY` | Literal hook command and spawned MCP entry with recording stub; clientSource = install |
| Binary and client from redirect; removal reverts; invalid fails loud | Redirect | Matrix {env, redirect, both, none, invalid, malformed} × {hook, mcp-stdio, pass-through} via written commands; remove → install; fixture `git status` clean |
| Retrieval MCP works, no plugin provisioning | `wire --harness opencode` | `mcp.unimatrix` written to `~/.config/opencode/opencode.json` (not project `opencode.json`); spawn that argv from the project, `initialize` + search; no plugin files |
| No `.opencode/` provisioning | `init` with `.opencode/` | Fixture before/after diff |
| Inside `UNIMATRIX_PROJECT_ROOT`: MCP reachable; outside or variable unset: exit 0 silent, zero requests (D-052, D-053) | Headless `npx -y @dug-21/unimatrix mcp-bridge` | Spawn exactly that command with headless env against a recording endpoint: cwd inside root → `initialize` + search succeed; cwd outside root, and root unset → exit 0, no output, zero requests |

## Integration Risks
- Shim (sh/.cmd) → `shim-entry.js` → launcher in-process: argv, stdin, exit codes (R-04, R-18).
- One resolver for hook, MCP, pass-through, `status`: unit matrix plus one integration per caller.
- Builder ↔ predicate ↔ Codex trust hash: string stability is a cross-component contract (R-03, R-19).
- Project predicate in three places plus the Rust store hash (R-28), now including the D-052 root rule (R-34); shell pre-check must stay a superset (R-29).
- Headless contract shared with vnc-050 (`mcp-bridge`, smoke test, docs) (R-35).
- Trust check ↔ `trust.json` cache ↔ upgrade (R-22, R-30).
- Client `last-hook.json` ↔ `status` reader (R-32).

## Edge Cases
- HOME/USERPROFILE with space, `'`, `%`, `&`, non-ASCII; HOME unset; home moved after init (per-machine, re-init).
- `install.json` empty, extra fields, `packageRoot` or `node` deleted.
- Redirect symlink, relative `binary`, `client` lacking `lib/hook-client/index.js`.
- Concurrent hooks, concurrent init, upgrade during a hook.
- cwd = `/`, drive root, UNC, symlink, nested projects, project moved.
- Many store dirs; legacy store dirs without `root`; headless env exported globally.
- Event names added after release; settings with no `hooks` key or `hooks` as array.

## Security Risks
- Untrusted inputs: `redirect.json`, `install.json`, `trust.json`, `root` markers, `last-hook.json`, `UNIMATRIX_BINARY`, `UNIMATRIX_BUNDLE`/`UNIMATRIX_REMOTE_*`, shim file, `pkg/<version>` tree, existing settings JSON/TOML, hook stdin payload.
- Damage: arbitrary code per hook/MCP via a writable pointer (R-06); a writable `trust.json` whitelisting a tampered file (R-30); `root` marker content only ever compared as a string, never executed or sourced (R-29); foreign settings entry deleted (R-03); corrupted user settings (R-08).
- Blast radius: user-level and cross-project; for Codex, every directory.
- Exec of resolved binary and node uses argv arrays; shim scripts quote every expansion; `set /p` / `read` content never evaluated.
- Residuals accepted by register: same-user tamper of `shim-entry.js`; Windows ACL-only change with unchanged identity (D-036).

## Failure Modes
| Condition | Expected behaviour |
|---|---|
| cwd not a D-045 project (any install state), hook or `mcp-stdio` | Exit 0, no output, in budget |
| macOS/Windows project, no `UNIMATRIX_BUNDLE`, no credential | Hook: exit 0 silent (D-050); `status` reports |
| Linux project, no valid `install.json`, no credential | Hook: exit 0 + one-line hint (D-023, ADR-004) |
| Any OS, project, not installed, `mcp-stdio` | Exit 1 + one-line hint, stdout empty (D-043) |
| Windows fallback MCP entry, recorded node stale | `status`: reports the stale path + re-init hint, exit 0. Init/upgrade preflight: exit 1 + re-init hint (D-057). MCP spawn: harness ENOENT, no Unimatrix output; accepted by D-066 |
| Headless env, cwd outside `UNIMATRIX_PROJECT_ROOT` | Not a project: hook and `mcp-stdio`/`mcp-bridge` exit 0 silent, send nothing (D-052, D-043) |
| Binary/client/install pointer invalid, node not found, trust failure, Linux local project with binary missing | Exit 1, stderr names cause and path; no fallback |
| Connection/transport error after resolution | Existing hook-client codes (D-042) |
| Codex hook unapproved | Not fired; `status` prints approval step |
| `trust.json` missing/mismatched | Full check, then cache rewrite; failure → exit 1 |
| `last-hook.json` write fails | Event still stored, exit unchanged |
| Pass-through on macOS/Windows | Linux-only message, exit 1 |
| Malformed existing settings JSON/TOML (incl. `~/.claude.json`) | init aborts, file untouched |
| Codex/OpenCode user-scope `mcp-stdio` outside a project | Exit 0 silent, no network, in budget |
| Foreign `unimatrix`-named MCP entry | Left untouched; warning naming file and key |
| Legacy project-scope Unimatrix MCP left behind | `status` reports it |
| Interrupted upgrade | Pointer remains on previous version |
| D-034 Windows MCP proof fails | Swap to D-047 fallback (recorded node + `shim-entry.js mcp-stdio`) and continue; escalate only if the fallback also fails |
| Windows hook command form (`.cmd` shim, `"<abs>.cmd" hook E`) fails in the D-034 job | No fallback named in the register: stop that work and escalate to the human |

## Scope Risk Traceability
| Scope Risk | Architecture Risk | Resolution |
|---|---|---|
| SR-01 | R-17, R-18, R-25 | ADR-001 absolute shim (D-027); R5 dropped (D-046); Windows MCP proved first (D-034) |
| SR-02 | R-11, R-21, R-22, R-29, R-39 | ADR-005 relative budget incl. not-a-project; Windows identity compare (D-036) |
| SR-03 | R-01, R-23 | D-023/D-042 outcomes; missing credential silent hook, `status` only (D-050) |
| SR-04 | R-13 | parity-layer2 gating (AC-08) |
| SR-05 | R-08, R-19, R-39 | Codex user-level only (D-026); string fixed (D-028); `status` (D-038) |
| SR-06 | R-12, R-38 | No mode field; identical per OS family (D-040) |
| SR-07 | R-26 | Fixtures/temp HOME on all OS jobs |
| SR-08 | R-03 | `hook-ownership.js` table + round-trip |
| SR-09 | R-03 | Negative table; event adjacency |
| SR-10 | R-13 | Test-count gate |
| SR-11 | R-16 | Launcher route; Stage 1 audit (D-035) |
| SR-12 | R-10 | Pass-through regression |
| SR-13 | R-17, R-26 | E2E runs literal string through each harness shell on all OSes |
| SR-14 | R-07, R-26, R-34, R-35 | HTTPS E2E from written config, three OSes |
| SR-15 | R-01, R-20, R-28, R-29 | Exit 1 for resolution failure; silent only outside a D-045 project; predicate parity and lossless pre-check |
| SR-16 | R-05 | AC-10 per-path tests |
| SR-17 | R-08, R-36, R-40 | Cross-scope removal incl. project MCP files (D-060) + seeded test |
| SR-18 | R-06, R-22, R-30, R-34 | D-024 precedence; D-029/D-036 trust after predicate |
| SR-19 | R-05 | D-024 |
| SR-20 | R-10 | Present-invalid fails; absent falls back |

## Coverage Summary
| Priority | Risk Count | Required Scenarios |
|---|---|---|
| Critical | 1 (R-03) | 3 + 16 behavioral |
| High | 13 (R-04, R-07, R-08, R-17, R-18, R-26, R-28, R-29, R-34, R-36, R-37, R-38, R-39) | 52 |
| Medium | 15 (R-01, R-06, R-09, R-10, R-11, R-12, R-13, R-19, R-21, R-22, R-24, R-30, R-32, R-35, R-40) | 48 |
| Low | 11 (R-02, R-05, R-14, R-15, R-16, R-20, R-23, R-25, R-27, R-31, R-33) | 12 |

## Open Escalations
None. L-1..L-6, Q8, Q9, OQ-6, T-1 (D-051), T-2 (D-052/D-053) and V-1/A-5 (D-060) are decided.
