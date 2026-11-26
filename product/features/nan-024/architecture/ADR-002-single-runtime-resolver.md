## ADR-002: Single runtime resolver, fixed override precedence, trust check

### Context
D-002 moves resolution to runtime. D-024: `UNIMATRIX_BINARY` governs the binary only; client resolves from redirect, then installed package. D-029/D-036: trust check on shim, `install.json`, redirect; Windows does a full ACL check at init and a per-event identity compare. D-032: macOS/Windows are cloud-only. R-06: `require(packageRoot)` executes code named by the pointers.

### Decision
- `lib/resolve-runtime.js` exports `resolveRuntime({env, home, platform, transport}) -> {binary, clientRoot, binarySource, clientSource}`. Sole resolver for shim entry, launcher, hook, `mcp-stdio`, pass-through, `status`.
- **Client:** redirect `client` -> `install.json` `packageRoot`. No env var, no inference beside the binary.
- **Binary**: `UNIMATRIX_BINARY` -> redirect `binary` -> installed package. Resolved only (a) for hook/`mcp-stdio` when transport is local, or (b) on Linux for pass-through verbs (`serve`, daemon and other Rust verbs), whatever the project's transport. Pass-through targets the local server, not a project transport, so a cloud-attached Linux project still runs `serve` from the resolved binary. If it can't be resolved, exit 1 naming the sources tried. Otherwise `null`, no lookup. macOS/Windows never resolve a binary (D-032); pass-through prints the Linux-only message (ADR-004).
- **Both set:** `UNIMATRIX_BINARY` wins for binary; redirect `client` governs client; tested for hook, `mcp-stdio`, pass-through (AC-14).
- Canonical client entry: `<clientRoot>/lib/hook-client/index.js` (repo: `packages/unimatrix/lib/hook-client/index.js`).
- **Validation:** present-but-invalid (missing target, not executable, client root lacking the entry, malformed JSON) is a resolution failure -> exit 1 inside a project (ADR-004), stderr names source and path. Only an absent redirect falls through (SR-20).
- **Order:** the project predicate (ADR-004; read-only, inlined in `shim-entry.js`, no package code) runs first. Outside a project the shim exits 0 silently before any trust check or `require` (R-06).
- **Trust check** by `shim-entry.js` on the shim, `install.json`, `redirect.json` (if present), and `packageRoot`; refuse loudly, exit 1, never skip:
  - POSIX, every event: owner == euid; no group/other write bit; `~/.unimatrix` and `~/.unimatrix/bin` mode 0700. init sets modes explicitly regardless of umask (container umask 002 passes).
  - `trust.json` itself: yes, it is in the trust set. POSIX checks it every event (owner, mode) like the other files. On Windows it gets the full ACL check at init and on every full re-check. Per event it can't be compared against itself, so it is covered by the recorded identity of its parent `~/.unimatrix` dir (an ACL-restricted dir). A missing, malformed or unparseable `trust.json` forces a full re-check.
  - Windows (D-036): init sets ACLs (`icacls /inheritance:r /grant:r`) and runs the full check: owner SID == current user; DACL allows write/modify/full only to current user, SYSTEM, Administrators; read via `icacls /save` SDDL (locale-independent). init then records each checked file's identity `{size, mtimeMs, ino}` (`fs.statSync`, `ino` = NTFS file-id) in `~/.unimatrix/trust.json`. Each event: `statSync` and compare; on any mismatch, or `trust.json` missing, rerun the full check, and on pass rewrite `trust.json`. No process spawn on the matching path.
- Cost: POSIX ~4 `statSync` + 1-2 JSON reads; Windows matching path ~5 `statSync` + 1 JSON read; measured in ADR-005.

### Consequences
- One switch for all harnesses; promote/rollback is a redirect edit (D-005). Redirect management stays in #1025.
- `resolve-binary.js` becomes a thin wrapper over `resolveRuntime().binary`.
- Windows: an ACL-only change that leaves size/mtime/file-id intact is not detected until the next identity change or init (accepted by D-036).
- The shim cannot verify itself before it runs (same-user attacker residual).
