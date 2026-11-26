# nan-024 Q9 Windows CI proof report (D-034 / D-047)

Unimatrix entries consulted: #5865 (resolved to active #5889, ADR-001), #5790, #5128, #4779

## Report
CI run: https://github.com/dug-21/unimatrix/actions/runs/45731809245 (windows-latest, Node 22, USERPROFILE=`D:\a\_temp\q9 home dir`, space in path). Workflow commit pushed on feature/nan-024 (workflow-only commit).

Files: `.github/workflows/q9-windows-mcp-proof.yml`, `.github/q9-proof/driver.js`, `.github/q9-proof/spawn.rs`.

| Form | Result | Evidence |
|---|---|---|
| Primary MCP `cmd /d /s /c ""%USERPROFILE%\...\unimatrix.cmd" mcp-stdio"` | **FAIL** | Both Node spawn (Claude Code semantics) and Rust `Command.args` (Codex semantics): exit 1, no marker, stderr "The network path was not found." Spawner backslash-escapes the embedded quotes (`\"`), which cmd.exe does not understand, so the path is misparsed (UNC-like). |
| Fallback MCP (D-047) node abs path + forward-slash `shim-entry.js mcp-stdio` | **PASS** | Node spawn and Rust Command: marker received, stdin piped, exit code 7 propagated, args exact, space in home handled. |
| Hook `"<abs forward-slash>.cmd" hook SessionStart` via cmd.exe /C | **PASS only when the string is passed verbatim** | Rust `raw_arg` (Codex-like): PASS. Node `windowsVerbatimArguments`: PASS. Node default quoting (escapes quotes): FAIL ("'\"D:/...cmd\"' is not recognized"). Depends on whether Codex passes the command string raw; Codex source (`command_runner.rs:470-526`) not verified in CI. |
| Hook `.cmd` via Git Bash (`bash -c`, `bash -lc`) | **PASS** | Marker, stdin, exit 7, args, space in path all OK. |

Evidence the real shim path works end-to-end: stub `unimatrix.cmd` wraps `node shim-entry.js %*` with `exit /b %errorlevel%`; stdin and exit code propagate through it (R-18 partial).

## Limitations
- Real Claude Code / Codex binaries not run (no auth/MCP session in CI). Claude Code approximated by Node `child_process.spawn` shell:false; Codex by compiled Rust `std::process::Command` (`.args()` for MCP, `.raw_arg()` for the hook string).
- Stub shim, not the real one.
- Hook-via-cmd.exe result for Codex rests on the assumption that Codex uses raw_arg for the `/C` string; if Codex uses `.args()`, that form would FAIL like the Node default-quoting case. Recommend verifying against Codex source.
- No decision made on the swap (per instructions): evidence is D-034 primary fails, D-047 fallback passes.

## Knowledge Stewardship
- Queried: context_briefing (#5790 CI model, #5128 Linux-only local gates) -- confirmed Windows behavior is only provable in GH Actions.
- Stored: nothing yet -- lesson (Windows spawners backslash-escape embedded quotes, breaking `cmd /s /c ""..." args"`) should be stored by the lead after the swap decision.
