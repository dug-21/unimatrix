# nan-024: Every Harness on the Node Client — Portable Wiring (OpenCode Deferred)

## Problem Statement
Personal-cloud clients have no local server binary, so hooks and MCP there must be Node. Production local installs still wire Claude hooks to the Rust `hook` client (`packages/unimatrix/lib/wire.js:154-157` passes `t.binaryPath` as the legacy string form; `merge-settings.js:120-128` maps it to `LD_LIBRARY_PATH=<dir> <bin> hook <EVENT>`). Both modes write absolute machine paths (`node_modules/...`, or ephemeral `~/.npm/_npx/<hash>/...`) into host-owned config. Those paths break silently on cache clean or reinstall.

This affects every local install (Rust hook path, non-portable config) and every cloud install (npx-cache paths).

## Goals
1. Local `init` and cloud attach produce identical harness config for Claude and Codex. The client resolves binary, transport and credentials at runtime (D-002).
2. No shipped wiring invokes the Rust binary as a hook client. `bin/unimatrix.js` routes `hook` to the Node client (D-001, P1).
3. Harness entries use one wiring form, chosen in design against R1–R4 (D-003; R5 dropped, D-046).
4. Re-running `init` over any legacy wiring form replaces Unimatrix entries and never duplicates them. Across every scope `init`/`wire` writes, there is at most one Unimatrix entry per hook event (D-010).
5. Hook p95 ≤ max(`node -e 0` baseline p95 + 15 ms, 50 ms), including the not-a-project path (D-025, ADR-005).
6. Dogfood seam (D-013, D-024): the released launcher honours a user-level override. `UNIMATRIX_BINARY` governs the binary only; the redirect file in `~/.unimatrix/` governs both binary and Node client; otherwise both resolve from the installed package.
7. `init` writes no OpenCode plugin provisioning. Opt-in `wire --harness opencode` keeps its retrieval MCP entry (D-004).

## Non-Goals
- **Dogfood work for this repo, tracked in #1025 (D-009, D-012).** The override seam itself stays in scope (D-013). Out of scope: writing or managing the redirect, the dogfood procedure (packed client, HEAD-tied binary, DB backup), resetting this repo's tracked harness configs, and removing the stray `.opencode/plugins/unimatrix.js` and `.mcp copy.json`.
- OpenCode hooks or a v2 plugin (#998). Changes to the published `opencode-plugin/lib/emit.js` (P3) wait for #998.
- Deleting Rust `hook.rs`. It stays as the parity oracle (#4751).
- Native remote HTTP MCP without the bridge.
- Uninstall/unwire, Windows/macOS local mode, a Gemini writer (D-007).
- Recording F6 soak continuity. D-001 removes the soak gate.
- Changing the Rust MCP stdio server or daemon behaviour.
- Controlling how a harness fires hooks. Unimatrix guarantees only its own configuration (D-010).

## Background Research
- **Wiring code**: `lib/wire.js` `dispatchClaude` uses Node only for `stdio-bridge` (cloud). Local passes the binary path. `lib/init.js:554` (cloud Claude) and `lib/codex-install.js:536` (Codex) already emit `node <abs>/hook-client/index.js`. `lib/opencode-install.js` provisions `.opencode/package.json` and plugins and appends to `opencode.json` `plugin[]` (P4). These must be removed from `init`.
- **Ownership regexes**: `merge-settings.js:20-27` matches `unimatrix hook`, `<path>/unimatrix hook` and `node <path>/hook-client/index.js`. It does not match `npx ...` forms or whatever new form design picks. This is the highest regression risk.
- **C-10 byte-identity**: nan-023 AC-16 is encoded in `normalizeCommandSource`'s string arm and in tests (`merge-settings.test.js:52-55,151`, `dogfood-effect.test.js:178,648,703`, `init-remote.test.js`). D-008 supersedes it, so these tests get updated, not deleted.
- **Binary override (P6)**: `lib/resolve-binary.js:24` already honours `UNIMATRIX_BINARY`. ass-109 listed P6 as missing, but the override exists. The remaining work is runtime resolution in the client.
- **Launcher routing**: `bin/unimatrix.js` routes only `init`, `wire` and `mcp-bridge` to JS and execs Rust for everything else. `hook` needs a JS route.
- **Harness scopes** (ass-109 Q6, not re-verified upstream):
  - Claude: `settings.local.json` plus local-scope MCP. Hooks from every scope are combined.
  - Codex: `~/.codex/*`. Project config is honoured only for trusted projects.
  - OpenCode: `~/.config/opencode/*`.
- **Relevant ADRs**: nan-016 #5825 and #4926 (switchover via `mergeSettings`), vnc-031 #4941. #682 is the F6 gate. D-001 overrides its soak requirement.

## Proposed Approach
1. **Launcher**: add a `hook` route in `bin/unimatrix.js` to the Node client, so legacy `unimatrix hook` configs also land on Node. Add the D-013 override check: the launcher reads a user-level redirect file in `~/.unimatrix/` or `UNIMATRIX_BINARY`, and resolves both the binary and the Node client from it.
2. **Wiring form (ADR-001)**: a user-level shim under `~/.unimatrix/bin/`, referenced by absolute resolved path (D-027), scored against R1–R4 (R5 dropped, D-046).
3. **One command source** for Claude and Codex in both modes:
   - Delete the legacy string arm.
   - Extend the ownership patterns to every legacy form and to the new form.
   - Dedupe across every scope `init`/`wire` writes.
4. **OpenCode**: remove provisioning from `init`. Keep `wire --harness opencode` MCP.
5. **CI**:
   - End-to-end hook test for Claude and Codex over UDS and HTTPS.
   - Latency measurement on the assembled install path.
   - Move `parity-layer2*` into CI.
6. **Docs**: update README:252/254/280 and `.gemini/settings.json` to remove Rust `hook`. Release notes state OpenCode = retrieval only. The repo's `.codex/hooks.json` reference config is reset in #1025 (D-048).

## Acceptance Criteria
- AC-01: In a clean repo, `npm install` + `npx unimatrix init` (local) and cloud attach write byte-identical Claude and Codex hook and MCP entries. Identity holds per machine and per OS family (D-040, D-046). MCP entries go only to per-machine scope (Claude `~/.claude.json` project key, Codex `~/.codex/config.toml`); never a committed `.mcp.json`; the D-047 Windows fallback is excepted from per-OS-family identity (D-060).
- AC-02: No file written by `init` or `wire` contains a `unimatrix hook` invocation of the Rust binary. A local install spawns zero Rust hook processes per event, which a test asserts.
- AC-03: `unimatrix hook <EVENT>` via `bin/unimatrix.js` executes the Node client, not the Rust binary.
- AC-04: Written entries meet the following (R5 dropped; every install is per-machine, D-046):
  - R1: no PATH, profile or repo-layout changes are required.
  - R3: entries survive npx-cache clean, reinstall and upgrade, or exit non-zero with a visible error.
  - R4: only Unimatrix-owned entries are touched.
  - Hooks: Claude entries go to `settings.local.json`; Codex entries go to `~/.codex/hooks.json` only. On every platform (Linux, macOS, Windows) the hook command is the absolute resolved shim path, with no `$HOME` and no shell expansion, quoted for paths with spaces; Windows uses the `.cmd` shim (D-027). A test asserts it byte-for-byte and unchanged across upgrade (D-028). `init` prints the Codex approval step and writes no trust hashes.
  - Exit outcomes (D-023): not a Unimatrix project → exit 0, silent; Unimatrix project without install → Linux: exit 0 with a one-line hint; macOS/Windows: exit 0 silent, hint only in `unimatrix status` (D-050); resolution broken → exit 1 with stderr; connection errors keep their current codes (D-042). "Unimatrix project here" = local store `~/.unimatrix/<hash>/`, or `~/.unimatrix/<hash>/remote.json`, or headless env (`UNIMATRIX_BUNDLE` / `UNIMATRIX_REMOTE_*`) with `UNIMATRIX_PROJECT_ROOT` set and cwd inside it, shared by hooks and the MCP bridge (D-045, D-052). `mcp-stdio` with no install: exit 1 with a one-line hint inside a project (every OS), silent outside (D-043). `init` refuses a shim path containing `'` (Unix) or `%`/`"` (Windows). Re-recording Node rewrites the Windows fallback MCP entry; a stale Node path is reported by `status` and fails init/upgrade preflight with exit 1 (D-057).
  - Trust check (D-029): shim, `install.json`, redirect file and resolved `packageRoot` are current-user owned, not group/world-writable, parent dir 0700 (Windows: full ACL check at init recording file identity in `trust.json`; each event compares identity and reruns the full check on mismatch, D-036); failure → exit 1, loud. Project detection runs before the trust check. macOS/Windows project with no bundle (`UNIMATRIX_BUNDLE`) and no credential (`remote.json`) → exit 0 silent, reported only by `unimatrix status` (D-050). Cloned repo without init → hooks and `mcp-stdio` exit 0 silent, reported by `status` (D-051). `init` does not edit `.gitignore`; it warns when `.claude/settings.local.json` is not ignored (D-039). `unimatrix status` reports a wired-never-fired Codex hook from `last-hook.json` (not approval state) and prints the approval step (D-038).
- AC-05: Re-running `init` over each legacy form (Rust string, absolute `node`, npx-cache, and any prior form) leaves exactly one Unimatrix entry per event. Foreign entries are preserved. There is one test per form.
- AC-06: Hook p95 per-event wall time ≤ max(`node -e 0` baseline p95 + 15 ms, 50 ms), including the not-a-Unimatrix-project path, measured in CI on the assembled install path (D-025). Cloud budget = local + measured RTT.
- AC-07: CI fires a real hook event for Claude and Codex, from the harness command through the Node client over UDS and over HTTPS. Each run ends in a stored observation with the correct provider. Commands are read from the hook destination (Codex trust bypassed in CI). Linux covers UDS and HTTPS; macOS and Windows cover cloud attach plus Claude and Codex hooks firing through the shim over HTTPS (D-033). Each platform includes a non-Unimatrix-cwd run that exits 0 with no stderr and no observation.
- AC-08: `parity-layer2*` suites (UDS, PreCompact, concurrency) run in CI and pass.
- AC-09: With `.opencode/` and `opencode.json` present, `init` creates no `.opencode/plugins/` and no `.opencode/package.json`, and leaves `opencode.json` `plugin[]` unchanged. `wire --harness opencode` writes the `mcp.unimatrix` entry to `~/.config/opencode/opencode.json` (D-063).
- AC-10: `UNIMATRIX_BINARY` governs the binary at runtime in both hook and MCP paths; it does not affect client resolution (D-024).
- AC-11: README and `.gemini/settings.json` contain no Rust `unimatrix hook` references. Release notes state OpenCode = retrieval only. Docs state PowerShell is unsupported as a hook shell (D-041).
- AC-12: The regression-guard suites stay green, and tests that pin old strings are updated, not deleted. The guarded behaviours are:
  - skills install, DB pre-create, postinstall model download, bundle decode
  - credential 0600, ping, pinned/CA TLS
  - headless attach (`npx -y @dug-21/unimatrix mcp-bridge`)
  - Codex TOML upsert, MCP removal preserving foreign servers, SubagentStop opt-in
  - Rust pass-through `LD_LIBRARY_PATH`
  - the `[hc]`, `[pkg]`, `[oc]` and orphan-audit CI suites
- AC-13: Across every config scope `init`/`wire` writes for a harness (for example Claude project `settings.json`, `settings.local.json` and user settings; Codex user `~/.codex/hooks.json`, project `.codex/hooks.json`, and `[hooks]` in `~/.codex/config.toml`), there is at most one Unimatrix entry per hook event after any sequence of `init`/`wire` runs. A test asserts this per harness. Codex: Codex merges layers additively, so after init/wire exactly one Unimatrix entry per event exists, in user scope (`~/.codex/hooks.json`); owned entries in project `.codex/hooks.json` and `~/.codex/config.toml` `[hooks]` are removed. Claude: owned hook entries exist only in `settings.local.json` (D-026). A Codex-only project with MCP in user scope still stores events (D-044). User-level edits are listed in a dry-run. The test seeds user + project and asserts this. Limit: the harness, not Unimatrix, fires hooks. Unimatrix guarantees only its own configuration, not entries written by users or other tools (D-010).
- AC-14: For `hook`, MCP and pass-through commands, the released launcher resolves the binary from `UNIMATRIX_BINARY`, then the redirect file in `~/.unimatrix/`, then the installed package; it resolves the Node hook client from the redirect file, then the installed package (D-013, D-024). The binary is resolved only when local transport is selected (D-032). A test covers the redirect file, `UNIMATRIX_BINARY`, both, and no override, and asserts which binary and which client are executed in each case.
- AC-15: Headless env (`UNIMATRIX_BUNDLE` / `UNIMATRIX_REMOTE_*`) counts as a project only with `UNIMATRIX_PROJECT_ROOT` set and cwd inside it, for hooks and the MCP bridge. A test proves no cross-repo leak: with the env exported and cwd outside the root, it is not a project and no events are sent. `docs/client-setup.md` and `headless-smoke.test.js` require `UNIMATRIX_PROJECT_ROOT` (D-052, D-053).
- AC-16: `init` removes owned Unimatrix MCP entries from project `.mcp.json` and project `.codex/config.toml` (D-062) and project `opencode.json` (D-064), preserving foreign servers, never deleting the file, and not rewriting it when nothing owned is present. A foreign command under the name `unimatrix` is left in place with a warning. A foreign entry already at a per-machine Unimatrix key (`~/.claude.json`, `~/.codex/config.toml`, OpenCode user config) is left in place, no Unimatrix MCP entry is written for that harness, and init prints `unimatrix: <file> key <key> holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written` without changing its exit code (D-065). `unimatrix status` reports each project file with a leftover Unimatrix entry (D-068). `~/.claude.json` is changed at one key via tmp+rename; on parse failure init aborts without overwriting (D-060).

## Constraints
- Local server is Linux-only and UDS is Unix-only. macOS and Windows are cloud-only (D-032).
- Node is the only hook client. Hooks never invoke Rust.
- One Node process per hook event. No npx resolution on the hot path.
- Claude combines hooks across scopes, so the wiring must not place Unimatrix entries in more than one scope per event (AC-13).
- Codex honours project `.codex/*` only for trusted projects.
- The published OpenCode plugin still emits Rust `hook`. It is not touched here (#998).

## Open Questions
None. Closed at design-review: wiring form (ADR-001, D-026, D-027), Codex scope (D-026), latency budget (D-025), D-034..D-046.

## Tracking
GH #1023. Dogfood follow-up: #1025.
