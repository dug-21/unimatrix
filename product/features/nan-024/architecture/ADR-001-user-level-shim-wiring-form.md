## ADR-001: Wiring form is a user-level shim invoked by absolute resolved path

### Context
D-003 requires one wiring form scored against R1 (no PATH/profile/repo-layout change), R2 (no npx on hot path, one Node process per event), R3 (survive cache clean/reinstall/upgrade or fail loud), R4 (touch only owned entries). R5 (portability) is dropped: every install, local or cloud, is per-machine (D-046). Today local writes `LD_LIBRARY_PATH=<abs> <abs>/unimatrix hook E` and cloud writes `node <abs-npx-cache>/hook-client/index.js E`; both violate R3. nan-023 C-10 byte-identity is superseded (D-008).

| Candidate | R1 | R2 | R3 | R4 | Hook latency (p95, devcontainer) | Notes |
|---|---|---|---|---|---|---|
| A `npx -y @dug-21/unimatrix hook E` | Y | **N** | Y | Y | 200 ms+ (npx resolution; network on miss) | Hot-path npx |
| B bare `unimatrix hook E` | **N** | Y | partial | Y | ~node baseline (33.6 ms) + launcher | Needs PATH |
| C `node <abs npx cache>/hook-client/index.js E` | Y | Y | **N** | Y | ~node baseline | Current cloud form |
| E in-process launcher, `node <abs ~/.unimatrix/pkg/<version>>/bin/unimatrix.js hook E` | Y | Y | **N** (string changes every upgrade, which re-untrusts Codex, D-028; breaks when the version is pruned) | Y | ~node baseline | Same in-process routing as D, but the version path is baked into the string |
| D user-level shim, absolute path | Y | Y | Y | Y | ~node baseline + one `sh`/`cmd` exec (est. 1-5 ms; measured per ADR-005) | Chosen |

MCP placement (D-060):

| Placement | Per-machine (D-046) | Cloned repo silent (D-051) | Windows fallback absolute path safe (D-047/D-057) | Notes |
|---|---|---|---|---|
| Committed project `.mcp.json` / `.codex/config.toml` | **N** | **N** (cloners spawn an MCP entry for a machine never attached) | **N** (one user's absolute node path committed) | Rejected; removed by init |
| Per-machine: Claude local scope in `~/.claude.json` (keyed by project), Codex user scope `~/.codex/config.toml` | Y | Y | Y | Chosen |

`$HOME`-anchored D is rejected by D-027: Windows `cmd.exe /C` (Codex default on Windows, `command_runner.rs:470-526`) does not expand `$HOME`.

### Decision
- **Shim files** (written by init, re-written by init and on upgrade, D-030):
  - Unix/macOS: `~/.unimatrix/bin/unimatrix`, POSIX `sh` script, mode 0700.
  - Windows: `%USERPROFILE%\.unimatrix\bin\unimatrix.cmd`, batch script.
  - Both embed the recorded absolute Node path and run `<node> <home>/.unimatrix/bin/shim-entry.js <args>`. `shim-entry.js` is version-independent: project predicate (ADR-004), trust check (ADR-002), read `install.json`, `require(<packageRoot>/bin/unimatrix.js)` in-process. One Node process per event.
- **Node resolution (D-030, R-09):** recorded path -> `node` on PATH -> loud stderr `unimatrix: node not found (recorded <path>); run 'npx unimatrix init'`, exit 1. Before failing, the shim runs the pre-Node project check (ADR-005) so outside a Unimatrix project it exits 0 silently.
- **Hook command string** — absolute resolved shim path on every platform, no `$HOME`, no shell expansion (D-027):
  - Unix/macOS: `'<abs shim path>' hook <EVENT>`; POSIX single quotes (valid in sh, bash, zsh, fish). init refuses (exit 1) if the path contains `'`.
  - Windows: `"<abs shim path with forward slashes>.cmd" hook <EVENT>`; double quotes handle spaces in cmd.exe `/C` and Git Bash. init refuses if the path contains `%` or `"`.
  - Codex appends ` --provider codex-cli`.
  - PowerShell as hook shell is not supported (a quoted path is a string literal there); documented (D-041). Codex uses `%COMSPEC%`, Claude Code on Windows uses Git Bash.
- **Byte-fixed string (D-028):** the string depends only on the shim path, unchanged across upgrades; upgrades rewrite shim content and `install.json` only. Test asserts byte-equality across a simulated upgrade, per platform. init prints the one-time Codex hook approval step.
- **Stable package copy (D-031):** `~/.unimatrix/pkg/<version>/`; `install.json` replaced via tmp + rename (atomic pointer swap); prune to current + previous. Local and cloud.
- **MCP placement (D-060):** per-machine scope only, never a committed file.
  - Claude: local-scope MCP in `~/.claude.json`, at `projects["<abs project root>"].mcpServers.unimatrix`.
  - Codex: user scope, `[mcp_servers.unimatrix]` in `~/.codex/config.toml`. It is one entry for all projects; `mcp-stdio` resolves the project from cwd and is silent outside a project (D-043).
  - OpenCode: ADR-006.
  - init removes owned Unimatrix entries from project `.mcp.json`, project `.codex/config.toml` and project `opencode.json` `mcp.unimatrix` (D-064), keeping foreign servers (ADR-003).
- **MCP entries** (argv, no shell): D-027's no-expansion rule covers hook destinations only. The MCP entry uses home-relative expansion, so its text is identical for every user on one OS family (D-040):
  - Unix/macOS `{"command":"sh","args":["-c","exec \"$HOME/.unimatrix/bin/unimatrix\" mcp-stdio"],"env":{}}`.
  - Windows `{"command":"cmd","args":["/d","/s","/c","\"\"%USERPROFILE%\\.unimatrix\\bin\\unimatrix.cmd\" mcp-stdio\""],"env":{}}`; `/s` strips the outer quotes, inner quotes survive a space in `%USERPROFILE%`. Whether harnesses pass this argv verbatim is proven in the first delivery task (Windows CI, D-034).
  - Windows fallback (D-047), swapped in by delivery without stalling if the D-034 proof fails: `{"command":"<recorded abs node path>","args":["<abs home>/.unimatrix/bin/shim-entry.js","mcp-stdio"],"env":{}}`. Node is a real executable, so no `cmd` quoting is involved; the recorded node path is re-written on init/upgrade (D-030), This is safe because the entry sits only in per-machine scope (D-060, D-046). Every time node is re-recorded (init, upgrade; D-030) init/upgrade also rewrites this entry (D-057). If the recorded node path is stale, the harness spawn fails (ENOENT) before Unimatrix code runs. This spawn-time gap is accepted (D-066, amends D-057). So `status`, and init/upgrade preflight, check the fallback entry's node path exists, and report a missing one loudly: `unimatrix: recorded node <path> missing; run 'npx unimatrix init'`, exit 1 from init/upgrade. The fallback entry never resolves node silently via PATH.
  - `mcp-stdio` behaviour: ADR-004.
- **Config identity (AC-01, D-040):** local and cloud write the same hook and MCP entry text per OS family (the D-047 fallback excepted), into per-machine files; no mode-specific field (SR-06).
- **Git ignore (D-039):** init does not edit `.gitignore`. If `git check-ignore` reports `.claude/settings.local.json` not ignored, init (and `--dry-run`) prints a warning.
- **Exits:** ADR-004.

### Consequences
- User-level footprint: `~/.unimatrix/bin/`, `install.json`, `trust.json` (Windows), `pkg/`, the `unimatrix` entry under the project key in `~/.claude.json`, and `[mcp_servers.unimatrix]` in `~/.codex/config.toml`.
- A cloned repo carries no Unimatrix MCP or hook entry, so it is silent until init runs on that machine (D-051). Every user-level write/removal is listed in `--dry-run` (D-026). Uninstall out of scope (D-007).
- Moving the home directory invalidates the hook string; re-run init. Acceptable: installs are per-machine (D-046).
- Tests execute the literal written string through each harness shell: `sh -c`, `$SHELL -lc`, `cmd.exe /C`, Git Bash, including a home path with a space.
- Supersedes nan-023 C-10.
