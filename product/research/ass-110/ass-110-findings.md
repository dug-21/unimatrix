# FINDINGS: Harness wiring placement under per-machine install

**Spike**: ass-110 (#1040), input to #1036
**Date**: 2026-11-26
**Approach**: investigation plus targeted measurement (scratch `HOME` experiments with Claude Code 2.1.332; source reading of Codex `ad7176f7` and OpenCode 2.4.1 `586fa903`, matching the installed `opencode v2.4.1`)
**Confidence**: empirical for Claude (behaviour measured); validated from source for Codex and OpenCode (no live Codex binary here); directional for Q6.

Scratch evidence: `/var/tmp/claude-1000/-workspaces-unimatrix/e5868048-b7e2-4786-a11e-00d1fb9d1f6f/scratchpad/{exp1,exp2,src}` (temporary, not kept).

---

## Headline

The issue's framing assumes Claude offers a project-level surface for MCP that nan-024 bypassed. It does not. The surfaces split three ways:
- Claude hooks: a project-level, per-machine, untracked file exists (`.claude/settings.local.json`).
- Claude MCP: only two choices exist, a shared project file (`.mcp.json`) or a per-project entry inside the global `~/.claude.json`.
- Codex (MCP and hooks) and OpenCode MCP: there is no per-project, per-machine surface. The choice is the shared project dir or user scope.

Every shared project file fails the constraints. A committed or committable entry is visible to cloners. Without an install it fails loudly: Claude prints `✘ Failed to connect` (measured), and Codex prints `MCP client for … failed to start` (source). It also can't be text-identical on Windows, because the MCP fallback embeds a recorded node path. **So nan-024's placement is right, but for the wrong reasons.** The work for #1036 is hardening, not moving entries:
- honour each harness's config-dir override;
- write symlinks and races safely;
- extend the removal set;
- make user-scope MCP silent outside a Unimatrix project.

---

## Findings

### Q: 1. Inventory: every file nan-024 writes, per harness and OS family

Source: `packages/unimatrix/lib/{wire,claude-json,merge-settings,codex-install,opencode-install,install-store,init}.js`.

| Harness | Surface | Class | Op | Linux (local + cloud) | macOS / Windows (cloud only, D-032) |
|---|---|---|---|---|---|
| Unimatrix | `~/.unimatrix/bin/{unimatrix, shim-entry.js, trust.js}`, `install.json`, `pkg/<ver>/` (+ Linux platform binary), `<hash>/` (DB, `remote.json`) | user, Unimatrix-owned | write | `sh` shim 0700 | `unimatrix.cmd` on Windows, plus `trust.json` |
| Claude | `.claude/settings.local.json` `hooks` (8 events) | project local overlay | write | `'<abs>/.unimatrix/bin/unimatrix' hook E` | Windows `"<abs fwd>/unimatrix.cmd" hook E` |
| Claude | `.claude/settings.local.json` `unimatrix.remote` | project local overlay | remove (stale credential, cloud) | yes | yes |
| Claude | `.claude/settings.json` hooks | project shared | remove owned | yes | yes |
| Claude | `~/.claude/settings.json` hooks | user | remove owned | yes | yes |
| Claude | `~/.claude.json` `projects[<main root>].mcpServers.unimatrix` | **global** (Claude-owned file) | write | `sh -c 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'` | Windows: `<recorded node> <home>/.unimatrix/bin/shim-entry.js mcp-stdio` (D-072) |
| Claude | `.mcp.json` `mcpServers.<owned>` | project shared | remove owned | yes | yes |
| Claude | `.claude/skills/*` | project shared | write (install-if-absent) | yes | yes |
| Codex | `$CODEX_HOME/hooks.json` (default `~/.codex`) | user (global for Codex) | write if Codex detected (D-081) | abs shim + `--provider codex-cli` | same, `.cmd` |
| Codex | `$CODEX_HOME/config.toml` `[mcp_servers.unimatrix]` | user | write (D-084) | as Claude MCP | as Claude MCP |
| Codex | `$CODEX_HOME/config.toml` `[hooks]` | user | remove owned | yes | yes |
| Codex | `.codex/hooks.json`, `.codex/config.toml` `[mcp_servers]` | project shared | remove owned | yes | yes |
| OpenCode | `~/.config/opencode/opencode.json` `mcp.unimatrix` | user | write (only if detected or `--harness opencode`) | argv form | argv form |
| OpenCode | `opencode.json` `mcp.unimatrix` | project shared | remove owned | yes | yes |
| Git | `.gitignore` | — | untouched; warning only (D-039) | — | — |

Gaps found in the inventory:
- **G1 (config-dir overrides):** `CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR` and `XDG_CONFIG_HOME` are ignored; there are zero references in `lib/` and `bin/`. Claude puts `.claude.json` and `settings.json` under `CLAUDE_CONFIG_DIR`. Measured: `CLAUDE_CONFIG_DIR=$S/cfg claude mcp add-json` wrote `$S/cfg/.claude.json`. OpenCode resolves its global dir from `OPENCODE_CONFIG_DIR ?? Path.config` (`util/src/global.ts:79`). When either variable is set, Unimatrix writes a file the harness never reads. The harness then runs with no Unimatrix wiring and no error (an R3 violation).
- **G2 (user-scope Claude MCP):** the user-scope entry `~/.claude.json` top-level `mcpServers.unimatrix` is never removed. The removal set covers only project files. Local scope outranks user scope, but if the local entry is lost (for example to `claude purge`, which deletes `projects[<root>]`), Claude silently falls back to a stale user-scope entry.
- **G3 (legacy OpenCode plugin):** a legacy project `opencode.json` `plugin: ["@dug-21/unimatrix-opencode-plugin"]` entry, written by pre-nan-024 init (ass-109 P4), is not removed.

**Recommendation**: Use the table above as the #1036 baseline, and fix G1–G3 in #1036 (see the feature scope below).

### Q: 2. Harness capabilities, verified against docs or behaviour

**Claude: does `.mcp.json` or `settings.json` expand env vars?**
- `.mcp.json` expands `${VAR}` and `${VAR:-default}` in `command`, `args`, `env`, `url` and `headers` (MCP docs). Measured: `"command":"${HOME}/bin/x"` spawned `$HOME/bin/x`. An unset variable stays literal and `claude mcp list` warns about it.
- `settings.json` hooks: Claude substitutes only `${CLAUDE_PROJECT_DIR}`, `${CLAUDE_PLUGIN_ROOT}` and `${CLAUDE_PLUGIN_DATA}`. Shell form (`sh -c`; Git Bash on Windows; PowerShell if Git Bash is absent) also expands `$HOME`. Exec form (`command` + `args`) uses no shell (hooks docs).
- `mcpServers` in `.claude/settings.local.json` is **not honoured**. Measured: `claude mcp get fromlocal` returned "No MCP server named".

**Claude: approval and trust semantics, project `.mcp.json` vs local scope**
- Project `.mcp.json` servers prompt for approval in interactive sessions and load without asking in `-p`, SDK and cloud sessions. An `enabledMcpjsonServers` approval in an untracked `settings.local.json` takes effect only after the folder is trusted. Measured: `⏸ Pending approval` before trust, then the server spawned after `hasTrustDialogAccepted`.
- Measured: a server that exits 0 shows `✘ Failed to connect — CONNECTION_CLOSED`. A committed entry that is inert on an uninstalled clone is therefore never silent.
- Local scope needs no approval. It is stored in `~/.claude.json` under the project path, and for linked worktrees it is keyed by the **main checkout root**. Measured: `claude mcp add-json` from a worktree wrote the entry under the main root, which matches `resolveProjectKey`.
- Precedence is local > project > user > plugin, and the whole entry comes from one source (no field merge).
- Hooks merge across scopes, and an identical handler defined in several settings files runs once.
- Claude's own writer **preserves a symlinked `~/.claude.json`**. Measured: the link was intact after `claude mcp add-json`. Unimatrix's `atomicWrite` **replaced the symlink with a regular file** (measured; this reproduces #1035 F2).
- `claude mcp add-json` refuses an existing name ("already exists"), so an update via the CLI is a non-atomic remove + add (≈0.7 s total).

**Codex: does a project-scoped surface exist, and how does approval behave?**
- Yes. `.codex/config.toml` and `.codex/hooks.json` are found in cwd, tree and repo layers (`config/src/loader/mod.rs:136-150`). They are "loaded but disabled when untrusted", and the gating covers "project-local config, hooks, and exec policies" (`:1161`).
- `mcp_servers` is not on `PROJECT_LOCAL_CONFIG_DENYLIST` (`:104-117`), so a trusted project can define MCP servers with no per-server approval.
- Hooks also need per-hook trust, keyed `file:<path>:<event>:<group>:<handler>` plus a content hash (nan-024 `codex-hooks-evidence.md`).
- There is **no per-project, per-machine overlay**: `[projects."<path>"]` admits only `trust_level` (`ProjectConfig`, `deny_unknown_fields`, `config_toml.rs:631-634`), and no `*.local.toml` exists.
- Codex MCP config does no env expansion; stdio is argv.
- A server that fails the handshake is reported as `MCP client for \`unimatrix\` failed to start` (`codex-mcp/src/connection_manager/startup.rs:142`, surfaced by `tui/src/chatwidget/mcp_startup.rs`).

**OpenCode: what project and local surfaces exist?**
- Project: `opencode.json[c]` and `.opencode/` (plugins, agents) found by walking up from cwd.
- Global: `$OPENCODE_CONFIG_DIR` or the XDG config dir. Explicit: `OPENCODE_CONFIG` (an env var, so R1 rules it out).
- Text substitution `{env:VAR}` and `{file:path}` applies to all config (`core/src/config/variable.ts:27-39`).
- No approval or trust gate was found for project MCP.
- A per-project personal overlay `~/.config/opencode/project/<project-id>/` exists (`config/discovery.ts` `overlay`), but it sits behind the `project_overlay` flag. That flag is "off in releases" (`schema/src/config/flag.ts:5-24`), and turning it on would mean editing the user's global `experimental` config. **Not usable today.**

**Recommendation**: Treat Claude local scope, Codex user scope and OpenCode user scope as the only per-machine MCP surfaces. Claude `settings.local.json` is the only per-machine hook surface across the three harnesses.

### Q: 3. Options per harness, scored against each constraint

Key:
- PS = project-scoped
- PM = per-machine
- EI = explicit intent (no clone auto-connect)
- SD = never silently dark
- SO = silent outside a Unimatrix project, and silent on an uninstalled clone
- LC = low churn (no tracked diffs for others)
- R1 = no PATH, profile or repo-layout change

**Claude hooks**

| Option | PS | PM | EI | SD | SO | LC | R1 | Verdict |
|---|---|---|---|---|---|---|---|---|
| `.claude/settings.local.json` (nan-024) | Y | Y (untracked by Claude's design) | Y | Y | Y (absent on clones) | Y¹ | Y | **Chosen** |
| `.claude/settings.json`, shell form `"$HOME/.unimatrix/bin/unimatrix" hook E` | Y | **N** | Y | Y | **N** (cloner without shim: `sh: not found` on every event) | **N** on Windows (abs `.cmd` path) | Y | Reject (D-018 reason holds) |
| `~/.claude/settings.json` | **N** (global) | Y | Y | Y | Y (shim predicate) | Y | Y | Reject: a project surface exists |
| Plugin enabled at local scope | Y | Y | Y | Y | Y | Y | Y | Viable later. It still writes global Claude state (marketplace declared in user settings, cache in `~/.claude/plugins`, install docs) and needs plugin packaging. Out of scope |

¹ If Unimatrix creates the file before Claude ever writes it, it is not yet in Claude's global excludes. Claude adds `**/.claude/settings.local.json` to the global git excludes only when Claude itself writes the file (settings docs). D-039's warning covers that case.

**Claude MCP**

| Option | PS | PM | EI | SD | SO | LC | R1 | Verdict |
|---|---|---|---|---|---|---|---|---|
| Local scope `~/.claude.json` `projects[root]` (nan-024) | Y (logically) | Y | Y | Y² | Y | Y | Y | **Chosen; named exception: the file is global** |
| Project `.mcp.json` with `${HOME}` form, approved via `settings.local.json` | Y | **N** | Y (prompt) | Y | **N** (measured `✘ Failed to connect` on uninstalled clone) | **N** (host-tracked file; Windows entry embeds node path) | Y | Reject |
| User scope `~/.claude.json` top-level | **N** | Y | Y | Y | N (fires in all projects) | Y | Y | Reject; add to removal set (G2) |

² Provided G1 (`CLAUDE_CONFIG_DIR`) is fixed, and `status` reports a missing entry after `claude purge`.

**Codex hooks and MCP**

| Option | PS | PM | EI | SD | SO | LC | R1 | Verdict |
|---|---|---|---|---|---|---|---|---|
| `$CODEX_HOME/{hooks.json,config.toml}` (nan-024) | **N** (no per-project surface exists) | Y | Y | Y | hooks Y (shim predicate); **MCP N** (exit 0 → "failed to start" in every non-Unimatrix project) | Y | Y | **Chosen; named exception**; MCP needs the SO fix below |
| `.codex/hooks.json` + `.codex/config.toml` | Y | **N** | Y (trust + per-hook approval) | Y | **N** (trusted clone without shim: failures) | **N** (Windows/abs paths; tracked in many repos) | Y | Reject |

**OpenCode MCP (and hooks per #998)**

| Option | PS | PM | EI | SD | SO | LC | R1 | Verdict |
|---|---|---|---|---|---|---|---|---|
| `$OPENCODE_CONFIG_DIR \|\| $XDG_CONFIG_HOME/opencode \|\| ~/.config/opencode` `opencode.json` `mcp.unimatrix` | **N** | Y | Y | Y (after G1) | **N** for MCP until the SO fix | Y | Y | **Chosen for MCP; named exception** |
| Project `opencode.json` `mcp` | Y | N | **N** (no approval gate; cloners spawn it) | Y | N | N | Y | Reject |
| Per-project overlay `~/.config/opencode/project/<id>/` | Y | Y | Y | Y | Y | Y | **N** (requires enabling an experimental flag in the user's config; off in releases) | Revisit when the flag graduates |
| Hooks (#998): `.opencode/plugins/unimatrix.js` as a byte-fixed, version-independent stub that resolves `~/.unimatrix` at runtime and no-ops when absent | Y | Y (behaviour) | Y | Y | Y (plugin code can self-silence) | Y (identical text on every machine and across upgrades) | Y | **Recommended for #998** (directional) |

The SO fix: a user-scope MCP entry fires in every project. `mcp-stdio`'s "exit 0, silent" (D-043) is silent in Unimatrix's own output, but the harness renders it as a failed server (Claude measured; Codex from source). Outside a Unimatrix project, `mcp-stdio` should complete the MCP `initialize` handshake as a server with zero tools and exit when stdin closes. This changes ADR-004's `mcp-stdio` outcome, and it is the one launcher change that placement forces.

**Recommendation**: Keep the nan-024 destinations for all three harnesses. Name Claude MCP (`~/.claude.json`), Codex (both) and OpenCode MCP as the global-file exceptions, and add the SO fix for user-scope MCP.

### Q: 4. Local vs cloud mode: does placement differ? How does explicit intent map onto each surface?
**Answer**: Placement does not differ.
- Local `init` and `init --bundle` run the same `wire()` and write identical entries (D-040 holds; verified in `init.js` `initRemote` → `wire(..., {cloud: true})`).
- Explicit intent does not live in the wiring. It lives in per-machine state that only an explicit command creates: the local store `~/.unimatrix/<hash>/` (from `init`) or the credential `~/.unimatrix/<hash>/remote.json` (from `init --bundle`). The shared project predicate reads that state (D-045, D-052).
- Every chosen surface is per-machine, so a clone carries no wiring. Even a copied wiring file is inert without the credential. OAuth (#1014) adds server-side approval and changes no harness surface, because MCP stays stdio to the local bridge.
- One caveat: Claude cloud sessions read neither `settings.local.json` nor `~/.claude.json` (settings docs, "Settings in cloud sessions"). Claude-on-the-web is therefore reachable only through claude.ai connectors (#1014), not through this wiring.

**Recommendation**: Keep one placement for both modes, and record in the ADR that the credential or store is the intent gate. Wiring placement must never be the gate.

### Q: 5. Migration from nan-024 placement
**Answer**: Yes. Init can migrate idempotently on top of the existing machinery:
- `lib/hook-ownership.js` provides `isUnimatrixCommand` and `isUnimatrixMcpEntry`: shape-based, name-anchored ownership.
- Writers already report `unchanged` when the content is equal.
- The D-065 rule already handles foreign entries at the `unimatrix` key.

Because the destinations stay the same, migration is limited to the new gaps.

Migration rules for init:
1. **Resolve destinations first.**
   - Claude dir: `CLAUDE_CONFIG_DIR` if absolute, else `~`, for `.claude.json` (as `$dir/.claude.json`) and the user `settings.json`.
   - Codex: `CODEX_HOME` (existing).
   - OpenCode: `OPENCODE_CONFIG_DIR`, else `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`.
   - Reject relative values with a warning (the D-085 pattern).
2. **Write before remove.** Write or confirm the single destination entry. Only then remove owned entries elsewhere, so a failed write never leaves the harness without wiring.
3. **Removal set (owned entries only; foreign entries are preserved byte-for-byte):**
   - the existing nan-024 set;
   - G2: top-level `mcpServers.unimatrix` in the resolved Claude `.claude.json`;
   - the default-location twin when a config-dir variable points elsewhere: `~/.claude.json` `projects[root].mcpServers.unimatrix`, `~/.config/opencode/opencode.json` `mcp.unimatrix`, and `~/.claude/settings.json` owned hooks;
   - G3: `@dug-21/unimatrix-opencode-plugin` in project `opencode.json` `plugin[]`.
4. **Ownership detection.** An MCP entry is owned when its name is `unimatrix` and its shape matches an `isUnimatrixMcpEntry` form. Hooks match `OWNED_FORMS` with `hook <KnownEvent>` adjacency. Add no marker fields, so the entry text stays identical (D-040).
5. **Idempotency.**
   - Equal content means no rewrite and no mtime change.
   - Across all scopes, at most one owned entry per event and per harness MCP key (D-010).
   - A second run produces an all-`unchanged` manifest (test this).
6. **Verify after write.** Re-read the destination and confirm the entry is present. If it is missing (lost update), retry once, then exit 1 naming the file.
7. **Dry-run parity.** List every write and removal, including the twin-location removals (FR-12d).

**Recommendation**: Implement rules 1–7 in the existing writers. Do not add new ownership markers.

### Q: 6. Write safety for unavoidable global/user files (recommend, not design)
**Answer and recommendations**:
- **Preserve symlinks.** `realpath` the destination, create the temp file in the target's directory, and rename onto the resolved target, never onto the link path. Measured: Claude's own writer keeps the link and Unimatrix's current writer breaks it.
- **Temp name.** Open with `O_EXCL` and a random suffix (`crypto.randomBytes`), mode 0600, then fsync before rename. This closes the #1035 "predictable temp name" item.
- **Races.** Keep the existing compare-and-swap (mtime + size) and add a content hash. Re-check immediately before rename, then verify after write (migration rule 6).
  - A lost update against a running Claude can't be fully excluded: no lock file was observed and none is documented.
  - So `unimatrix status` must keep detecting a missing local-scope entry, and re-running init fixes it.
- **Windows.** Retry rename on `EPERM`/`EBUSY` with a short backoff, because the file may be open in the harness.
- **CLI delegation.** Do not delegate to `claude mcp add-json` as the primary path.
  - An update needs remove + add, which is non-atomic.
  - D-065 still requires reading the file.
  - There is no dry-run.
  - `claude` may not be on PATH (IDE/desktop installs).
  - One hardened direct writer keeps a single code path.

---

## Verdicts on nan-024 decisions

| Decision | Verdict | Change |
|---|---|---|
| D-026 (Claude hooks → `settings.local.json`; Codex hooks → user `hooks.json` only; owned and dry-run user writes) | **Revise (rationale + config dir); placement kept** | Replace the "R5 waived" rationale with: Claude has a per-machine project surface for hooks; Codex has none, so it is the named exception. User-scope removal uses the resolved `CLAUDE_CONFIG_DIR` |
| D-040 (identical config per OS family) | **Keep, clarify** | Clarify it as: local and cloud write identical entries on a given machine; MCP text is identical across users per OS family, except the Windows fallback (D-072); hook text embeds the user's absolute home path |
| D-046 (R5 dropped; every install per-machine) | **Keep** | Promote it to the placement rule: write to a project-scoped, per-machine surface. Use a harness global file only where no such surface exists, name it in the ADR, and keep writes owned, surgical and reversible |
| D-060 (MCP per-machine only: Claude local scope in `~/.claude.json`, Codex user scope; never `.mcp.json`; remove owned project entries) | **Revise (mechanism + removal set); placement kept** | Honour `CLAUDE_CONFIG_DIR` and `OPENCODE_CONFIG_DIR`/XDG; add the G2/G3/twin removals; symlink-preserving write; make user-scope MCP silent outside a project via the zero-tool handshake (ADR-004 amendment) |

---

## Placement table (harness × mode → surface)

| Harness | Local (Linux) | Cloud (Linux/macOS/Windows) | Evidence |
|---|---|---|---|
| Claude hooks | `<main root>/.claude/settings.local.json` | same | Settings docs (local overlay, untracked, main-checkout root for worktrees); hooks merge and dedupe (hooks docs); `.mcp.json`/`settings.json` rejected (Q3) |
| Claude MCP | `${CLAUDE_CONFIG_DIR:-~}/.claude.json` `projects[<main root>].mcpServers.unimatrix` | same (Windows: D-072 fallback form) | `settings.local.json` `mcpServers` ignored (measured); `.mcp.json` fails silence (measured `✘ Failed to connect`); worktree key = main root (measured); `CLAUDE_CONFIG_DIR` moves the file (measured) |
| Codex hooks | `$CODEX_HOME/hooks.json` | same | No per-project overlay (`ProjectConfig` = `trust_level` only); project layer is shared, trust-gated (`loader/mod.rs`) |
| Codex MCP | `$CODEX_HOME/config.toml` `[mcp_servers.unimatrix]` + zero-tool handshake outside a project | same | `mcp_servers` allowed in project config but shared; startup failure surfaced (`startup.rs:142`) |
| OpenCode MCP | `${OPENCODE_CONFIG_DIR:-$XDG_CONFIG_HOME/opencode}/opencode.json` `mcp.unimatrix` + zero-tool handshake | same | `global.ts:79`; no project approval gate; overlay flag off in releases (`flag.ts`) |
| OpenCode hooks (#998) | `.opencode/plugins/unimatrix.js` byte-fixed self-silencing stub | same | `.opencode/` auto-load (`discovery.ts`); plugin code can no-op without install (directional) |

---

## Proposed feature scope for #1036

**Title**: feat(install): harden per-machine harness wiring placement (config dirs, removal set, write safety, silent user-scope MCP)

**In**
1. A placement ADR that supersedes the placement rationale in nan-024 ADR-001/ADR-003. It states the D-046 placement rule and names the global-file exceptions: Claude MCP, Codex, OpenCode MCP.
2. Config-dir resolution:
   - `CLAUDE_CONFIG_DIR` covers `.claude.json` and the user `settings.json`.
   - `OPENCODE_CONFIG_DIR`, then `XDG_CONFIG_HOME`, cover OpenCode.
   - `CODEX_HOME` is unchanged.
   - `status` prints each resolved file.
3. Removal set extension: G2 user-scope Claude MCP, default-location twins, and G3 legacy OpenCode `plugin[]` entry.
4. A hardened writer (one module) used by every user/global JSON and TOML write: symlink-preserving, `O_EXCL` random temp, compare-and-swap with content hash, verify after write, Windows rename retry.
5. `mcp-stdio` outside a Unimatrix project completes the MCP handshake with zero tools. This amends ADR-004 and D-043; it needs a human decision.
6. `status` reports a missing Claude local-scope entry (for example after `claude purge`) and names the file.

**Out**: shim, trust check, launcher hook path, latency budgets; OpenCode hooks (#998, placement guidance above); Claude plugin packaging; uninstall; `.gitignore` edits.

**Acceptance criteria**
- AC-1: With `CLAUDE_CONFIG_DIR=<d>`, init writes MCP only to `<d>/.claude.json` and removes an owned twin from `~/.claude.json`. A real `claude mcp get unimatrix` run from the project shows the entry.
- AC-2: With `OPENCODE_CONFIG_DIR` or `XDG_CONFIG_HOME` set, the OpenCode MCP entry lands in the resolved dir and an owned twin at `~/.config/opencode` is removed.
- AC-3: A symlinked `.claude.json`, `$CODEX_HOME/config.toml`, `hooks.json` or `opencode.json` is still a symlink after init, and its target holds the change.
- AC-4: Seeded owned entries are removed and foreign entries are preserved byte-for-byte. The seeded entries are: user-scope `mcpServers.unimatrix`, legacy `opencode.json` `plugin[]`, and every nan-024 legacy location.
- AC-5: A second `init` run reports every leg `unchanged` and changes no file's mtime.
- AC-6: Across all resolved scopes, at most one owned entry exists per hook event and per harness MCP key.
- AC-7: When the destination changes between read and rename (simulated), init retries. If the verify step still finds no entry, init exits 1 naming the file and never leaves the harness without wiring.
- AC-8: Outside a Unimatrix project, `mcp-stdio` answers `initialize` and `tools/list` (empty), exits 0 on stdin close, and writes nothing to stderr. `claude mcp list` from a non-Unimatrix directory shows it connected, not failed.
- AC-9: `--dry-run` lists exactly the writes and removals of a real run, including twins.
- AC-10: Temp files use a random name created with `O_EXCL`, and none remain after a failed write.

---

## Unanswered Questions

- **Live Codex behaviour** (no Codex binary in this environment). Not observed: the exit-0 MCP display and how user-scope hooks behave in an untrusted project. The source evidence is strong; delivery should confirm with a real Codex run in CI.
- **OpenCode display** of a failed or exit-0 user-scope MCP server was not observed live. Inferred by analogy and from the absence of any silent-failure path.
- **Lost update against a running Claude session**: whether Claude re-reads `~/.claude.json` before each write was not determined. Testing it needs a long-running interactive session, which was not done here.
- **Claude cloud environments**: whether a `settings.local.json` written by a setup script is read inside a Claude cloud session. The docs say local settings are "not read", but that refers to the user's machine copy.
- **OpenCode #998 stub**: viability (plugin load order, and whether the stub can self-silence with zero output) stays directional.

---

## Out-of-Scope Discoveries

- **Claude hook exec form** (`command` + `args`, no shell) now exists. It would remove shell quoting, the D-041 PowerShell limitation and the Windows `.cmd` quoting edge cases. Worth a follow-up against ADR-001's hook string; placement does not force it.
- **Claude local-scope plugin** (`enabledPlugins` in `settings.local.json`) could carry hooks and MCP together, with `${CLAUDE_PLUGIN_ROOT}`. It still declares the marketplace in user settings. It could be a future single-surface option for Claude.
- **OpenCode `project_overlay`** (`~/.config/opencode/project/<id>/`) is an exact per-project, per-machine surface. Re-evaluate OpenCode placement when the flag ships on by default.
- **OpenCode `{env:VAR}` substitution** means a home-agnostic OpenCode argv form is possible on Unix.
- **`claude purge <path>`** deletes the project's `~/.claude.json` entry, including local-scope MCP. It is a user-triggered dark path that only `status` can catch.
- **Explicit intent lives in the credential/store, not the wiring.** This may deserve a pattern entry after delivery validates it.

---

## Recommendations Summary

- Q1 Inventory: nan-024 writes one project overlay (Claude hooks), one Claude global file (`~/.claude.json`) and the Codex/OpenCode user configs. Fix G1 (`CLAUDE_CONFIG_DIR`, `OPENCODE_CONFIG_DIR`/XDG ignored), G2 (user-scope Claude MCP not removed) and G3 (legacy OpenCode `plugin[]` not removed).
- Q2 Capabilities:
  - Claude `.mcp.json` expands `${VAR}`, but its entries need approval and show `✘ Failed to connect` on uninstalled clones.
  - `settings.local.json` cannot hold MCP.
  - Codex has trust-gated shared project config and no per-machine overlay.
  - OpenCode's per-project overlay is flag-gated off in releases.
- Q3 Options: keep `settings.local.json` for Claude hooks. Keep Claude local scope, Codex user scope and OpenCode user scope for MCP and Codex hooks as named global-file exceptions. Reject every shared project file.
- Q4 Modes: one placement for local and cloud. The intent gate is the per-machine store/credential, never the wiring.
- Q5 Migration: write-then-remove with existing shape-based ownership. Add config-dir twins, G2 and G3 to the removal set. Verify after write. A second run must be all `unchanged`.
- Q6 Write safety: one hardened direct writer (realpath + same-dir `O_EXCL` random temp + CAS with hash + verify + Windows retry). Do not delegate to `claude mcp add-json`.
- Verdicts: D-026 revise (rationale, config dir); D-040 keep (clarify); D-046 keep (it becomes the placement rule); D-060 revise (mechanism, removal set, silent user-scope MCP).
- #1036: harden the existing placement, plus the zero-tool `mcp-stdio` handshake outside a project (needs a human decision).
