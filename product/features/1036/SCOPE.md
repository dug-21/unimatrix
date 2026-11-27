# Harden Per-Machine Harness Wiring Placement

Feature: 1036 (GH #1036). Scope and ACs are human-approved and final (D-005). Source of truth: #1036 "Scope (revised)", "Acceptance criteria", "Human decisions"; `DECISIONS.md` D-001..D-030 (D-011 supersedes D-004).

## Summary

nan-024 (#1023, PR #1033) chose the right destinations for Unimatrix's hook and MCP wiring, for the wrong reason (committed-config portability). ass-110 (#1040) confirmed the placement: no project-level MCP surface meets the constraints. This feature keeps the placement (D-001) and hardens it:
- restates the placement rationale as the D-046 rule in a new ADR;
- honours the harnesses' config-dir overrides;
- extends the removal set;
- routes every file Unimatrix writes through one hardened writer (D-010);
- reports a missing Claude local-scope entry in `status`;
- makes sure `.claude/settings.local.json` is git-ignored via the per-repo exclude file (D-011).

## Problem Statement

Three gaps found in ass-110 leave a harness silently dark, which violates R3. A fourth corrupts user files, and a fifth leaves local wiring committable.
- **G1, config dirs ignored.** `lib/` and `bin/` contain zero references to `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR` or `XDG_CONFIG_HOME`. When a user sets one, init writes `~/.claude.json` or `~/.config/opencode/opencode.json`, which the harness never reads, and no error is shown.
- **G2, stale user-scope Claude MCP.** A user-scope `~/.claude.json` top-level `mcpServers.unimatrix` is never removed. After `claude purge` deletes `projects[<root>]`, Claude silently falls back to the stale entry.
- **G3, legacy OpenCode plugin.** The `@dug-21/unimatrix-opencode-plugin` entry in project `opencode.json` `plugin[]` (pre-nan-024) is never removed.
- **Write safety (#1035).** This covers F2 and the predictable temp name.
  - `lib/atomic-write.js` replaces a symlinked `~/.claude.json` with a regular file (measured).
  - It uses a predictable `<file>.<pid>.tmp` temp name.
  - Its compare-and-swap checks only mtime and size.
  - It does not verify after writing.
  - It has no Windows rename retry.
- **Gitignore.** If Unimatrix creates `.claude/settings.local.json` before Claude ever writes it, the file is not covered by Claude's global excludes. Today init only warns about this (D-039).

## Goals

1. Record the placement rule (D-046) in an ADR. The ADR names the global-file exceptions and supersedes the placement rationale in nan-024 ADR-001 (#5896) and ADR-003 (#5897).
2. Write each harness's wiring to the file the harness actually reads, under `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`/`XDG_CONFIG_HOME` and `CODEX_HOME`.
3. Remove every Unimatrix-owned entry from abandoned or shadowing locations, and preserve foreign entries byte-for-byte.
4. Make every file Unimatrix writes symlink-safe, race-checked, verified and leak-free (D-010).
5. Make sure a lost Claude local-scope entry is never silent: `status` names the file.
6. Make sure `.claude/settings.local.json` is git-ignored, idempotently, without touching any tracked file (D-011).

## Non-Goals (Out)

- Shim, trust check, launcher hook path, latency budgets (nan-024 ADR-001 shim form, ADR-002, ADR-005 unchanged).
- Zero-tool `mcp-stdio` handshake outside a Unimatrix project (decision 1 / D-002). Outside a Unimatrix project, user-scope MCP shows as a failed server, and that is accepted. ADR-004 and D-043 are unchanged.
- OpenCode hooks (#998).
- Claude plugin packaging (local-scope plugin, marketplace).
- Uninstall.
- Moving any destination. Claude MCP stays in the global `.claude.json` local-scope entry (D-003).
- Claude hook exec form, OpenCode `project_overlay`, `{env:VAR}` forms. These are out-of-scope discoveries in ass-110.
- Trust-check TOCTOU (#1035 F4).
- Broader `status` diagnostics (#1034).

## Background Research

### Placement (ass-110, `product/research/ass-110/ass-110-findings.md`)

| Harness | Surface (kept) | Class |
|---|---|---|
| Claude hooks | `<main root>/.claude/settings.local.json` | project, per-machine |
| Claude MCP | `${CLAUDE_CONFIG_DIR:-~}/.claude.json` `projects[<main root>].mcpServers.unimatrix` | global file (named exception) |
| Codex hooks | `$CODEX_HOME/hooks.json` | user (named exception) |
| Codex MCP | `$CODEX_HOME/config.toml` `[mcp_servers.unimatrix]` | user (named exception) |
| OpenCode MCP | `${OPENCODE_CONFIG_DIR:-${XDG_CONFIG_HOME:-~/.config}/opencode}/opencode.json` `mcp.unimatrix` | user (named exception) |

Why no destination moves:
- `settings.local.json` ignores `mcpServers` (measured).
- `.mcp.json` shows `✘ Failed to connect` on an uninstalled clone (measured).
- Codex has no per-project overlay (`ProjectConfig` = `trust_level` only).
- OpenCode's `project_overlay` is off in releases.

Other measured facts:
- Claude's own writer preserves a symlinked `.claude.json`.
- `claude mcp add-json` cannot update in place, so ass-110 recommends against CLI delegation.

### Current code (`packages/unimatrix/`)

| Concern | File | State |
|---|---|---|
| Orchestrator | `lib/wire.js` (185 code lines) | `home = o.home \|\| os.homedir()`. `dispatchClaude` hardcodes `path.join(c.home, ".claude", "settings.json")` for the user-hook removal. The no-transport leg and `dispatchOpencode` error leg hardcode `~/.claude.json` / `~/.config/opencode` paths. `gitIgnored()` + `GITIGNORE_WARNING` (D-039 warning) is to be replaced (In-6) |
| Claude MCP writer | `lib/claude-json.js` (129) | `writeClaudeMcp` uses `path.join(o.home, ".claude.json")`. CAS is mtime + size, 3 attempts, no content hash, no post-write verify. `removeOwnedMcpJson` handles project `.mcp.json` only. No user-scope (top-level `mcpServers`) removal (G2). Re-exports `atomicWriteText` = `atomicWrite` |
| Generic writer | `lib/atomic-write.js` (25) | tmp `file + "." + pid + ".tmp"`, `writeFileSync` (no `O_EXCL`), `renameSync` onto the link path, which breaks symlinks. No fsync, no retry |
| Claude hooks | `lib/merge-settings.js` (301) | `mergeSettings` writes `settings.local.json`. `removeOwnedHooks` removes from project and user `settings.json`. Both go through `atomicWriteText` |
| Codex | `lib/codex-install.js` (337), `lib/codex-toml.js` (237) | `codexHome(env, home)` already honours `CODEX_HOME` with the D-085 relative-path warning. That is the pattern for the new resolvers. hooks.json and config.toml writes go through `atomicWriteText` |
| OpenCode | `lib/opencode-install.js` (182) | `writeOpencodeMcp` uses `path.join(o.home, ".config", "opencode", "opencode.json")`. `removeOwnedOpencodeProjectMcp` removes `mcp.unimatrix` only. No `plugin[]` removal (G3) |
| Ownership | `lib/hook-ownership.js` (106) | `isUnimatrixCommand`, `OWNED_FORMS`, `isUnimatrixMcpEntry`. Shape-based, no marker fields (D-040) |
| Status | `lib/status.js` (195) | Read-only, exits 0. Its own `codexHome` fallback; `mcpNode` reads `path.join(home, ".claude.json")` (win32 only). **No Claude MCP presence line at all** and no resolved-file lines (In-2, In-5) |
| Unimatrix-owned writers | `lib/install-store.js` (268), `lib/trust.js` (224) | Their own tmp + rename with `Math.random` suffix and Windows `renameWithRetry` (EPERM/EBUSY/EACCES, 25 ms × 10). This is reuse material for the hardened writer |
| Init | `lib/init.js` (391) | Calls `wire()`. Not the place for new logic |

Code-line counts exclude comments and blank lines. No production file is near the 1,000 code-line cap (PL-10); the largest is `lib/init.js` at 391. The new hardened writer and config-dir resolver fit as new focused modules. `lib/status.js` and `lib/wire.js` gain thin call sites. Test files over 1,000 lines (`test/merge-settings.test.js` 1695, `test/init-remote.test.js` 1130) are excluded from the cap.

### Prior lessons
- #5803: hook configs merge additively across scopes. Keep machine-local wiring out of tracked files, which is the motivation for In-6.
- #5837: tooling must own every harness surface, or clients silently regress. This is the motivation for the complete removal set and for config-dir resolution.

### Environment
`claude` is installed in the devcontainer (`~/.local/bin/claude`), so AC-1's real `claude mcp get` check can run. `opencode` is present, so AC-2 gains a real-`opencode` check (D-027). `codex` is not installed.

D-012: file-level checks are accepted for Codex and Claude in this feature (beyond AC-1's `claude mcp get`). Follow-up #1053 tracks real-binary Codex and Claude CI checks.

## Proposed Approach (Scope In 1-6)

1. **Placement ADR** (uni-architect, Unimatrix only). It supersedes the placement rationale in nan-024 ADR-001/ADR-003.
   - It states the D-046 rule: write to a project-scoped, per-machine surface. Use a harness global/user file only where no such surface exists. Name each such file, and keep writes owned, surgical and reversible.
   - It names the exceptions: Claude MCP (`.claude.json`), Codex (hooks and MCP), OpenCode MCP.
   - It records that the intent gate is the per-machine store or credential, never the wiring (ass-110 Q4).
2. **Config-dir resolution.** One resolver is shared by `wire` and `status`.
   - Claude: `CLAUDE_CONFIG_DIR` → `.claude.json` and user `settings.json`.
   - OpenCode: `OPENCODE_CONFIG_DIR`, then `$XDG_CONFIG_HOME/opencode`, then `~/.config/opencode`.
   - Codex: `CODEX_HOME` unchanged.
   - A relative value is ignored with a warning (the D-085 pattern).
   - `status` prints each resolved file.
3. **Removal set extension.**
   - G2: user-scope `mcpServers.unimatrix` in the resolved `.claude.json`.
   - Default-location twins = nan-024 legacy locations only (D-019): `~/.claude/settings.json` owned hooks and `~/.config/opencode/opencode.json` `mcp.unimatrix`. With `CLAUDE_CONFIG_DIR` set, the default `~/.claude.json` entry (incl. `projects[root]`) is kept, not removed, and listed by `status` (D-028). No extra twins are removed (XDG opencode when `OPENCODE_CONFIG_DIR` wins, `~/.codex` when `CODEX_HOME` is set, other alternate config dirs); `status` lists them.
   - G3: `@dug-21/unimatrix-opencode-plugin` in project `opencode.json` `plugin[]`.
   - Twin identity (D-007): compare resolved real paths. A twin that is the same real file as the target (for example `CLAUDE_CONFIG_DIR=~`, `XDG_CONFIG_HOME=~/.config`) is never removed.
   - Removal runs write-before-remove: a removal happens only after the destination write is verified.
4. **One hardened writer** for every file `init` / `init --bundle` / `wire` writes (D-010, D-020): user/global harness files, project files (`settings.local.json`, `.mcp.json`, project `opencode.json`, `.codex/*`), `~/.unimatrix/{install.json,trust.json,remote.json}`, `.claude/skills/*`, `bin/*` and the root marker. Excluded: hook-time runtime state. The package-copy directory swap is an atomic directory rename, not a file write. The writer core lives in `trust.js`; the rest of the installer imports it, with no second copy (D-021). A symlinked `trust.json` fails safe: exit 1 with a message naming the symlink and how to fix it (D-021).
   - realpath the destination; for a project file, resolve the project root realpath first and write through a symlink only if its real target is inside that root, else exit 1 naming the link (D-026); write a same-directory temp opened with `O_EXCL` and a random name (new config/credential files 0o600, new `bin/*` 0o700; `remote.json`, `install.json`, `trust.json`, root marker and `bin/*` are re-tightened to their mode on every write; all other existing files keep their mode; D-023, D-025), fsync it, and rename it onto the resolved target;
   - compare-and-swap on a content hash, re-checked right before rename;
   - verify after write: retry, then exit 1 naming the file;
   - retry the rename on Windows `EPERM`/`EBUSY`;
   - remove the temp on any failure.

   - read→modify→rename is synchronous, with no awaits between read and rename (D-013).

   It replaces `atomicWrite`/`atomicWriteText` and the private tmp + rename in `install-store.js` and `trust.js`. The weak path (`atomic-write.js` as-is) is removed. No delegation to the `claude` CLI.

   The lost-update window against a running Claude session is accepted (D-013): Claude does not lock `.claude.json`, and CAS cannot catch a later overwrite. Init output tells the user to re-run init or check `status` if Claude was running during install; In-5 is the backstop.
5. **`status` Claude local-scope check.** Report a missing `projects[<main root>].mcpServers.unimatrix` in the resolved `.claude.json` (for example after `claude purge`), name the file, and give the init hint.
6. **Git-ignore ensure via the per-repo exclude file (D-011, supersedes D-004).**
   - Resolve the exclude file with `git rev-parse --git-path info/exclude`. Never hardcode `.git/info/exclude`; this covers linked worktrees and submodules.
   - Run `git check-ignore .claude/settings.local.json`. If not ignored, append one entry to the resolved exclude file.
   - If already ignored from any source, the exclude file is untouched.
   - Outside a git repo, skip and report it.
   - Idempotent. `--dry-run` reports it. This replaces `GITIGNORE_WARNING`. No tracked file is modified.
   - Edge cases (no git / not a repo, linked worktree, entry text) are defined by the specification (D-008).

## Acceptance Criteria (#1036; AC-1 amended by D-028, AC-2 by D-027, AC-3 by D-010/D-026, AC-10 by D-010, AC-8 by D-011)

- AC-1 (amended, D-028): With `CLAUDE_CONFIG_DIR=<d>`, init writes MCP only to `<d>/.claude.json`; the default `~/.claude.json` entry (incl. `projects[root]`) is kept and listed by `status`, not removed. `claude mcp get unimatrix` from the project shows the entry.
- AC-2 (amended, D-027): With `OPENCODE_CONFIG_DIR` or `XDG_CONFIG_HOME` set, the OpenCode MCP entry lands in the resolved dir; an owned twin at `~/.config/opencode` is removed. A real-`opencode` check from the project (modeled on AC-1's `claude mcp get`) shows the entry.
- AC-3: A symlinked `.claude.json`, `$CODEX_HOME/config.toml`, `hooks.json` or `opencode.json` is still a symlink after init; its target holds the change.
  - Extension (D-010): applies to every file Unimatrix writes, including `settings.local.json`, `.mcp.json`, project `opencode.json`, `.codex/*` and `~/.unimatrix/{install.json,trust.json}`.
  - Amendment (D-026): a project-file symlink is written through only if its real target is inside the project root's realpath; otherwise init exits 1 naming the link. User/global symlinks behave as above.
- AC-4: Seeded owned entries (user-scope `mcpServers.unimatrix`, legacy `opencode.json` `plugin[]`, every nan-024 legacy location) are removed; foreign entries preserved byte-for-byte.
- AC-5: A second `init` reports every leg `unchanged` and changes no file's mtime.
- AC-6: Across all resolved scopes, at most one owned entry per hook event and per harness MCP key.
- AC-7: If the destination changes between read and rename (simulated), init retries; if verify still finds no entry, init exits 1 naming the file and never leaves the harness without wiring.
- AC-8 (reworded, D-011; intent unchanged): If `git check-ignore .claude/settings.local.json` fails, init appends one entry to the exclude file resolved by `git rev-parse --git-path info/exclude`; if already ignored (any source), that file is untouched. Outside a git repo, init skips and reports it. Re-run adds nothing. `--dry-run` reports it. Edge cases are specified in SPECIFICATION.md (D-008).
- AC-9: `--dry-run` lists exactly the writes and removals of a real run, including twins.
- AC-10: Temp files use a random `O_EXCL` name; none remain after a failed write.
  - Extension (D-010): applies to every file Unimatrix writes (same set as AC-3).

## Constraints

- **R1:** no user PATH, profile or repo-layout changes. Env vars are read, never set. No tracked file is edited; the git-ignore entry goes to the per-repo exclude file (D-011).
- **ADR-004 / D-043 unchanged** (D-002). `mcp-stdio` outside a project keeps its current outcome, and the launcher is not touched.
- **D-046 placement rule:** project-scoped, per-machine surface first. A global/user file is used only where no project surface exists, is named in the ADR, and is written owned-only, surgically and reversibly. No destination moves (D-001, D-003).
- **D-040:** entry text stays identical per OS family, with no marker fields. Ownership stays shape-based via `lib/hook-ownership.js`.
- **D-065:** a foreign entry at a per-machine `unimatrix` key is never overwritten. The writer warns and skips. The same applies at resolved and twin locations.
- **D-010 / ADR-003:** at most one owned entry per event and per MCP key across scopes (AC-6).
- **FR-12d:** dry-run parity. Every writer returns the same legs in dry-run as in a real run (AC-9).
- **Parse failure:** an unparseable `.claude.json` aborts that leg untouched with exit 1 (nan-024 FR-07j). The hardened writer must not weaken this. An unparseable `opencode.json` is skipped with a warning (exit 0), and `status` names the file as leaving OpenCode unwired (D-024). An unparseable project-level `opencode.json` during removal stays exit 1 (D-029).
- **AC-4 "byte-for-byte" (D-022):** JSON foreign values deep-equal with order kept; TOML byte-identical outside the owned table; untouched files never rewritten; existing JSON indentation detected and reused.
- **No Rust:** the work is Node-only, in `packages/unimatrix/lib/` (memory: Rust binary is not a client). The zero-dependency check (`test/check-zero-deps.js`) applies: use only `fs`/`crypto` built-ins.
- **Modularity (PL-10):** new logic goes in new focused modules, with thin call sites in `wire.js`, `status.js`, `claude-json.js`, `merge-settings.js`, `codex-install.js` and `opencode-install.js`.
- **Test infrastructure is cumulative:** extend `test/helpers`, `test/fixtures`, `merge-settings.test.js`, `wire.test.js` and `init-integration.test.js`. No isolated scaffolding.

## Prior Decision Verdicts (D-006)

| Decision | Verdict | Change |
|---|---|---|
| D-026 | Revise (rationale + config dir); placement kept | Rationale becomes: Claude has a per-machine project surface for hooks; Codex has none, so it is a named exception. User-scope hook removal uses the resolved `CLAUDE_CONFIG_DIR` |
| D-040 | Keep (clarify) | Local and cloud write identical entries on a given machine. MCP text is identical across users per OS family, except the Windows fallback (D-072). Hook text embeds the user's absolute home path |
| D-046 | Keep; becomes the placement rule | See Constraints |
| D-060 | Revise (mechanism + removal set); placement kept | Honour `CLAUDE_CONFIG_DIR` and `OPENCODE_CONFIG_DIR`/XDG. Add the G2, G3 and twin removals. Write through the hardened writer. The zero-tool handshake part of the ass-110 verdict is dropped (D-002) |
| D-039 | Revise (D-011, supersedes D-004) | init ensures the file is git-ignored via the per-repo exclude file (`git rev-parse --git-path info/exclude`), not `.gitignore`, instead of warning |

## Design Decisions (D-014..D-030)

- D-014: pre-nan-024 `.opencode/plugins/` shim and package.json dep are out of scope; only the legacy `plugin[]` entry is removed.
- D-015: an owned OpenCode entry at the old default location counts as prior opt-in (FR-13 kept).
- D-016 (amended by D-024): unparseable/commented `opencode.json` is skipped with a warning, exit 0.
- D-017: AC-5 "every leg" = harness legs + git-exclude; store legs also report `unchanged`.
- D-018 (amended by D-023): new-file mode set per caller.
- D-019: In-3 twins = nan-024 legacy locations only; extra alternate-dir twins are not removed; `status` lists them.
- D-020: writer coverage = every file init / init --bundle / wire writes, incl. `remote.json`, `.claude/skills/*`, `bin/*`, root marker; excludes hook-time runtime state; package-copy dir swap is an atomic dir rename.
- D-021: single writer core in `trust.js`, imported by the rest of the installer; symlinked `trust.json` fails safe with exit 1 naming the symlink and fix.
- D-022: AC-4 byte-for-byte = JSON deep-equal with order kept, TOML byte-identical outside owned table, untouched files not rewritten, JSON indentation reused.
- D-023: new config/credential files 0o600, new `bin/*` 0o700, existing files keep mode.
- D-024: unparseable `opencode.json` warns at install (exit 0) and `status` names the file as leaving OpenCode unwired.
- D-025 (amends D-023): modes enforced (re-tightened every write) on `remote.json`, `install.json`, `trust.json`, root marker, `bin/*`; other existing files keep mode.
- D-026 (amends AC-3): project-file symlinks written through only if the real target is inside the project root realpath; else exit 1 naming the link.
- D-027 (amends AC-2): real-`opencode` check added, modeled on AC-1's `claude mcp get`.
- D-028 (amends AC-1; D-019 wins): with `CLAUDE_CONFIG_DIR` set, the default `~/.claude.json` entry is kept and listed by `status`.
- D-029: unparseable project-level `opencode.json` during removal stays exit 1.
- D-030: with `CLAUDE_CONFIG_DIR` set, owned hooks in default `~/.claude/settings.json` are still removed (R4 kept, In-3 as written); hooks come from project `settings.local.json`, and removal prevents duplicate firing (AC-6).

## Dependencies

- nan-024 merged (#1023 closed, PR #1033), which provides the base code.
- Closes #1035 F2 (lost-update window, symlink replacement) and the temp-naming item. F4 stays open.
- Related, not blocking: #998 (OpenCode hooks), #1014 (OAuth attach), #1034 (status reporting), #1053 (real-binary Codex/Claude CI, D-012).

## Open Questions

None. Former questions resolved: writer coverage (D-010), twin equal-path rule (D-007), AC-8 edge cases (D-008, D-011), lost-update window (D-013), Codex/Claude live checks (D-012).

## Tracking

GH Issue: #1036. Research input: ass-110 (#1040). Decision register: `product/features/1036/DECISIONS.md`.
