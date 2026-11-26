## ADR-004: Launcher routing, shared project predicate, exit outcomes, status

### Context
`bin/unimatrix.js` execs Rust for every verb except init/wire/mcp-bridge (AC-02/03). D-023/D-042: three outcomes; exit 1 only for resolution failures. D-032: macOS/Windows cloud-only. D-037 as amended by D-050: package present but no credential -> exit 0 silent; the hint appears only in `unimatrix status`. D-038: status check for lost Codex approval. D-043: `mcp-stdio` no-install exits 1 only inside a project. D-044: no silent-drop gap for Codex-only projects. D-045: one definition of "Unimatrix project here".

### Decision
- **Project predicate** (D-045), `isUnimatrixProject(cwd, env, home) -> {hash, kind: "local"|"cloud"|"headless"} | null`:
  - `root = walkToProjectRoot(cwd)`, `hash = computeProjectHash(root)` (existing `hook-client/config.js` semantics; the hash is a local directory key only, cloud identity is the bundle slug).
  - Project iff any of: local store dir `~/.unimatrix/<hash>/` exists; cloud credential `~/.unimatrix/<hash>/remote.json` exists; headless env (`UNIMATRIX_BUNDLE` or any `UNIMATRIX_REMOTE_*`) set **and** `UNIMATRIX_PROJECT_ROOT` set to an absolute path with realpath(cwd) inside realpath(it) (D-052). Headless env without `UNIMATRIX_PROJECT_ROOT`, or cwd outside it, is not a project: no event is routed (stops cross-repo leak). Same rule in the MCP bridge (`mcp-bridge.js` / `remote-resolve.js` env tier), which refuses the env tier outside the root (D-053).
  - Config-file location (MCP or hook entries) plays no part, so a Codex-only project with relocated MCP is still a project (closes Q8, D-044).
  - Inlined in `shim-entry.js` (runs before trust check, ADR-002) and exported from `lib/project-state.js`; a parity test asserts both agree with `config.js`. Used by hook route, `mcp-stdio`, `status`, and the not-installed check. No other project test exists.
- **Installed** (`isInstalled(project, platform)`), D-032 read with D-045/D-052:
  - macOS/Windows: installed iff a bundle (`UNIMATRIX_BUNDLE`, honoured only under D-052) or a credential store entry (`~/.unimatrix/<hash>/remote.json`) exists. This is D-032 verbatim.
  - Linux (D-032 is silent; spec Domain Model): installed iff `install.json` is valid, or a credential (`remote.json`, or headless env under D-052) exists.
- **Transport:** bundle, `UNIMATRIX_REMOTE_*` (under D-052) or `remote.json` -> cloud; else Linux and installed -> local (binary resolved per ADR-002; unresolvable binary = Broken); else (macOS/Windows, no bundle/credential) -> not installed.
- **Exit outcomes** (hook route and shim):
  | State | Exit | Output |
  |---|---|---|
  | Not a Unimatrix project (incl. cloned repo never init-ed, D-051; headless env without matching `UNIMATRIX_PROJECT_ROOT`, D-052) | 0 | none; `status` reports a cloned repo |
  | Project on Linux, not installed (store present, no valid `install.json`, no credential) | 0 | one stderr line: `unimatrix: not attached on this machine; run 'npx unimatrix init'` (D-023) |
  | macOS/Windows: package copy present (`install.json` -> `pkg/<version>`) but no bundle and no credential for this project (L-4) | 0 | none; hint only in `status` (D-037 as amended by D-050) |
  | Project on macOS/Windows, not installed (no bundle, no credential) | 0 | none; hint shown only by `status` (D-050). This is D-023's "project with no install" case, with its hint relocated by D-050 |
  | Project, resolution failure: Linux local with unresolvable binary, invalid client or install pointer, invalid redirect, trust failure, node not found (D-042) | 1 | stderr naming cause and path |
  Connection/transport errors after resolution (daemon down, HTTPS failure) keep current hook-client exit codes (D-042).
- `hook` verb: `require(clientRoot + "/lib/hook-client/index.js").main(argv)` in-process; `index.js` guards auto-run with `require.main === module`. On each successful event, the client writes `~/.unimatrix/<hash>/last-hook.json` `{"<provider>": "<ISO time>"}` (one small write, best-effort).
- `mcp-stdio` verb (D-043/D-051): outside a project (incl. cloned repo never init-ed) -> exit 0 silently; inside a project and not installed (Linux: store present, no valid `install.json`, no credential; macOS/Windows: no bundle/credential) or resolution failure -> one-line stderr hint `unimatrix: not attached on this machine; run 'npx unimatrix init'` (or the failure cause), exit 1 (D-043); local -> exec Rust stdio server with `LD_LIBRARY_PATH`; cloud -> mcp-bridge.
- Pass-through verbs: Linux execs `resolveRuntime().binary` whatever the project transport (ADR-002); an unresolvable binary exits 1; macOS/Windows print `unimatrix: <verb> requires the local server (Linux only)`, exit 1. Regression test: non-hook verbs still exec Rust on Linux (SR-12).
- **`status` verb** (new, Node, all platforms; D-038/D-050/D-051):
  - Signature: `unimatrix status [--json]`, evaluated for `process.cwd()`. No other arguments. No network, no writes. Exit 0 always (informational), including when every check fails.
  - Text output: one `key: value` line per field, in this order:
    - `project:` `local <hash>` | `cloud <hash>` | `headless <root>` | `none`. When `none`: `none (this machine has no Unimatrix store or credential for <root>; run 'npx unimatrix init' to attach)` (D-051). For each project file holding a leftover owned Unimatrix MCP entry (ADR-003 predicate), one line follows: `legacy: <file> has a Unimatrix MCP entry; re-run 'npx unimatrix init' to migrate`, where `<file>` is the project-relative path `.mcp.json`, `.codex/config.toml` or `opencode.json`, checked in that order (D-060, D-064, D-068). No lines when none are found.
    - `installed:` `yes` | `no`, with a reason when `no`: `missing credential` (D-050), `no install.json`.
    - `transport:` `local` | `cloud` | `none`.
    - `trust:` `ok` | `FAIL <path>: <reason>`.
    - `mcp node:` `ok` | `n/a` (no D-047 fallback entry written) | `MISSING <path>; run 'npx unimatrix init'`. Checks that the recorded node path in the D-047 fallback MCP entry exists (D-057, D-066).
    - `hook <harness>:` one line each for `claude-code` and `codex-cli`, value `wired, last fired <ISO>` | `wired, never fired` | `not wired`. For `codex-cli` `wired, never fired`, a following line prints the Codex approval step (`/hooks` approval) (D-038).
  - `--json`: a single object with the same fields: `{project: {kind, hash, root} | null, legacyProjectMcp: string[] /* project-relative paths with a leftover owned entry, order .mcp.json, .codex/config.toml, opencode.json; [] when none */, installed: boolean, reason: string | null, transport: "local" | "cloud" | null, trust: {ok: boolean, path?, reason?}, mcpNode: {status: "ok" | "n/a" | "missing", path: string | null}, hooks: {"claude-code": {wired: boolean, lastFired: string | null}, "codex-cli": {...}}}`.
  - Does not read Codex trust hashes (format unverified).
- OpenCode plugin invoking `unimatrix hook` via PATH lands on Node (#998).

### Consequences
- Legacy configs work until rewritten; direct-path Rust entries are rewritten by init (ADR-003). Delivery Stage 1 audits repo, docs and installers for direct Rust `hook` invocations outside init-written files and escalates any found (D-035). The repo's own `.codex/hooks.json` is exempt from that escalation; it is handled by #1025 (D-048, D-058).
- The repo's own tracked `.codex/hooks.json` reference config (bare Rust form) is reset in #1025, not under AC-11 (D-048).
- Delivery task (D-053): update `docs/client-setup.md` and `headless-smoke.test.js` for `UNIMATRIX_PROJECT_ROOT` as a required headless variable.
- Server-side "MCP calls but zero hook events" signal (D-038 optional): not cheap, so deferred to #1026 (D-054). Reason: MCP stdio connections carry no harness session id, so per-session correlation needs new session plumbing in the Rust server; out of nan-024's Node-only change set.
