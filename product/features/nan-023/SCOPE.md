# nan-023: Install Options — Per-Harness Idempotent Wiring, Non-Destructive Definition Install

> Tracking: GitHub Issue #990. Goal: `personal-cloud` (#4946). Capability: advances **C14 (multi-LLM harness parity)** — proving each supported harness is wired AND that retrieval actually returns post-wiring. Related/evolved: C17 (server-side config seeding — done_when already proven; kept as a cross-link, not the target here). Folds in the vnc-049 OpenCode retrieval-provisioning gap.

## Problem Statement

`unimatrix init` (`packages/unimatrix/lib/init.js`) handles its install concerns inconsistently and offers no way to re-ensure *wiring* without also overwriting *definitions*:

- **MCP** (`.mcp.json`, `writeMcpJson` init.js:74) — merged, preserves foreign servers. Idempotent.
- **Hooks** (`.claude/settings.json`, `mergeSettings`) — merged by prefix-match ownership, preserves foreign hooks. Idempotent.
- **Skills** (`copySkills` init.js:130) — blanket `fs.copyFileSync` on every run (docstring: "Overwrites existing unimatrix skills"). **Clobbers user edits** every time init runs.
- **No wire-only path** — the only entry point is full `init`, so refreshing MCP/hooks forces the skill overwrite.
- **OpenCode retrieval not provisioned** — `maybeProvisionOpenCode` (opencode-install.js) provisions only the observation *plugin*; it deliberately never creates the `mcp.unimatrix` retrieval entry in `opencode.json` (vnc-049 ADR-006 preserves an existing one but a fresh env must hand-author it).
- **Codex not wired by the package** — the installation package (delivered via the JS client / dogfood mechanism) writes nothing for Codex into a consumer repo: no `.codex/config.toml` `[mcp_servers.unimatrix]` and no Codex hooks. A consumer's Codex MCP/retrieval and hooks exist only as hand-authored reference configs.

Why now (C14): C17 (server-side config seeding) is already proven, but **C14 — multi-LLM harness parity** — is not: parity requires that the installer wires each supported harness (claude-code, opencode, codex-cli) into a consumer repo AND that a `context_*` retrieval call against the wired slug actually **returns** afterward, and that harness hooks actually **fire**. Codex is the gap that keeps C14 unproven — the package does not wire it, so a Codex consumer has neither retrieval nor firing hooks. Codex hooks, like Claude hooks, must be wired to target the **JS hook client** (`packages/unimatrix/lib/hook-client/`); that is the target the installer writes.

Who is affected: every developer who re-runs `init` (loses skill edits), every OpenCode or Codex consumer of the package (retrieval/MCP not wired, Codex hooks not provisioned), and the `personal-cloud` promise that "one container, one command" holds across all supported harnesses without clobbering user-owned files.

## Goals

1. **Prove C14 harness parity — wiring plus a working return path.** For each supported harness (claude-code, opencode, codex-cli), not only write the MCP/retrieval config but verify that a `context_*` retrieval call against the wired slug **returns** after wiring, and that harness hooks actually **fire**. Wiring that writes config but yields no retrieval/hook activity does not satisfy this goal.
2. **Codex to full parity, including hooks routed through the JS hook client.** The package wires Codex as a fully verified leg into a consumer repo: `[mcp_servers.unimatrix]` in `.codex/config.toml` plus hooks written in a Claude-like, event-driven shape that targets the **JS hook client** (the same target Claude hooks use) with `--provider codex-cli` on every command. This is the intended install behavior.
3. **Split wiring from definition install.** Provide a wire-only path (a distinct `unimatrix wire` verb) that idempotently ensures MCP + hooks + retrieval for each detected harness and touches **zero** definition files.
4. **Non-destructive definition install (decided policy).** Default is **install-if-absent** — never overwrite an existing Unimatrix-owned definition. `--force` overwrites Unimatrix-owned definitions with shipped versions. Scope is **skills only** (protocols/agents stay out). Rationale: definitions are git-tracked and recoverable; install-if-absent + `--force` beats hash-tracking / 3-way merge on simplicity.
5. **Consistent detection + intent model** across harnesses, reconciled with #960 ("explicit intent, never silent install; help on ambiguity"): detection triggers wiring, but writing a **new** MCP/retrieval entry into a user-owned config requires explicit `--harness`/opt-in.
6. **Idempotent, dry-run-aware, malformed-safe** for every harness leg.

## Non-Goals

- **Hash-tracked / 3-way-merge definition updates.** Explicitly rejected in favor of install-if-absent + `--force`.
- **Writing to any global / user-level config** outside the project root (`~/.codex/config.toml`, `~/.claude/`, `~/.config/`, `~/.gemini/`). Wiring is project-scoped only.
- **Changing the dogfood definition source-of-truth flow.** Definitions flow *into* the package via `/uni-release`; this feature governs *consumer* repos.
- **New observation/retrieval capability per harness.** Depth of what each harness observes is separate per-harness feature work (e.g. C18/vnc-049). This feature only *wires* existing surfaces.
- **gemini-cli parity (deferred).** Gemini is out of scope this feature; claude-code, opencode, and codex-cli are the three parity legs. Gemini's hand-authored reference config stands until a later feature.
- **Expanding definition install beyond skills.** Protocols (`.claude/protocols/`) and agents (`.claude/agents/`) are not brought under install-if-absent/`--force` here; skills only.

## Background Research

Findings are from reading the installer code, the reference configs, the vnc-049 ADR (Unimatrix #5743), and Codex/Gemini config documentation.

### Current installer anatomy (`packages/unimatrix/lib/`)
- `init.js` — `init()` (local) and `initRemote()` (bundle/legacy). Local flow: resolve root (`.git` walk) → resolve binary → `writeMcpJson` → `mergeSettings` → `copySkills` → `maybeProvisionOpenCode` → pre-create DB → validate. No CLAUDE.md block (uni-init owns it).
- `merge-settings.js` — `mergeSettings` is the reference for **non-clobbering merge**: prefix-match ownership (`UNIMATRIX_PATTERNS`), per-event matcher groups, dedup, opt-in/opt-out pruning, object-identity keep test. Reuse this *principle* per harness; the *surface* differs per harness (matcher groups vs plugin arrays vs TOML tables).
- `opencode-install.js` — additive, fail-safe (warn-and-skip, never throw), containment-guarded (`isWithinProject`), indent-preserving. Provisions the plugin only; **retrieval entry is out of scope by design there** — this feature adds it.
- `bin/unimatrix.js` — arg routing for `init`; adding a `wire` verb or flags lands here.
- Definition install is currently **skills-only** (`copySkills`). Protocols (`.claude/protocols/`) and agents (`.claude/agents/`) are **not** copied by init today — scope must decide whether "definition install" expands to them or stays skills-only.

### Harness config-surface matrix (verified)

| Harness | MCP / retrieval surface | Hooks / observation surface | Format | Written by init today? |
|---|---|---|---|---|
| **claude-code** | `.mcp.json` `mcpServers.unimatrix` (stdio) | `.claude/settings.json` `hooks` | JSON | Yes (both) |
| **opencode** | `opencode.json` `mcp.unimatrix` | `.opencode/plugins/` shim + `opencode.json` `plugin[]` (in-process TS plugin) | JSON | Plugin only — **retrieval gap** |
| **codex-cli** | `.codex/config.toml` `[mcp_servers.unimatrix]` (`command`=stdio / `url`=HTTP) | Claude-like, event-driven hooks targeting the **JS hook client** with `--provider codex-cli` | **TOML** + JSON | **No — full gap; package must wire it** |
| **gemini-cli** | `.gemini/settings.json` `mcpServers` | `.gemini/settings.json` `hooks` (`BeforeTool`/`AfterTool`/`SessionEnd`) | JSON | Reference config hand-authored; not written by init |

### Codex-cli specifics (full parity leg)
- **MCP config**: project-local `.codex/config.toml` `[mcp_servers.<name>]` (snake_case, TOML), loaded when the project's `.codex/` layer is trusted; global fallback is `~/.codex/config.toml` (out of scope — project-local only). Transport selected by key: `command` → stdio, `url` → streamable HTTP. Introducing a **TOML writer** is new for the JS installer (all current writers are JSON).
- **Hooks — Claude-like shape, JS-hook-client target (intended install behavior)**: Codex hooks are configured very similarly to Claude Code. The package writes hooks that mirror the Claude hooks shape/approach — event-driven, `type: "command"` — and each command targets the **JS hook client** (`packages/unimatrix/lib/hook-client/`), the same executable Claude hooks use (not the unimatrix binary). Every hook command carries `--provider codex-cli`. This is what the installer provisions into a consumer repo, not a fix to any local config.
- **#16732 is FIXED**: Codex MCP-tool-call hooks now fire upstream. It is no longer a blocker or a non-goal; Codex hooks are treated as a working, verifiable leg (not "wired-but-inactive").
- **Provider attribution**: Codex shares event names with Claude Code, so every hook command MUST carry `--provider codex-cli` (vnc-013 ADR-006); omitting it silently mislabels events as `claude-code`.
- **Trust-gating (constraint, not a hedge)**: project-local `.codex/config.toml` and hooks load when the `.codex/` layer is trusted. This is noted as a truthful operational constraint; the ship target is functional Codex hooks + returning retrieval, verified to fire.

### Cross-references
- **#960** ("explicit intent, never silent install; help on ambiguity") — the intent model this feature must stay consistent with. #960 governs client attach vs full install; nan-023 extends the same principle to per-harness wiring (writing into a user-owned harness config is an intent-bearing action).
- **vnc-049 / ADR-006 (#5743)** — the non-clobbering OpenCode branch and the C17 retrieval regression sentinel (preserve `mcp.unimatrix`, the Ollama `provider` block, all foreign keys byte-for-byte).
- **C14 (multi-LLM harness parity)** — the capability this feature advances (wiring + working retrieval return path + firing hooks across the three legs).
- **C17 provisioning (#5582)** — related/evolved; its done_when (server-side config seeding) is already proven and is not this feature's target.
- **JS hook client** (`packages/unimatrix/lib/hook-client/`) — where all client-init install changes live; the package (dogfood mechanism) delivers it. The executable Codex hooks target (mirroring Claude).

## Proposed Approach

A per-harness **wiring layer** parallel to the existing detection-gated OpenCode branch, plus a definition-install policy change:

1. **Per-harness wiring modules**, one non-clobbering writer per (harness × surface). Each reuses the `mergeSettings`/`opencode-install` principle: detect → parse-safe → additive/install-if-absent merge → indent/format-preserving write → warn-and-skip on malformed. Adds a TOML reader/writer for Codex.
2. **Retrieval provisioning per harness**: write the harness's MCP/retrieval entry (claude-code `.mcp.json` — exists; opencode `opencode.json` `mcp.unimatrix` — **new**; codex `.codex/config.toml` `[mcp_servers.unimatrix]` — **new**), each idempotent and preserving any existing entry.
3. **Definition install = install-if-absent + `--force`.** Replace `copySkills`' blanket overwrite with per-file existence check; `--force` restores shipped versions of Unimatrix-owned files only.
4. **Wire-only path** that runs the wiring layer and skips all definition copying and DB steps.
5. **Detection by project-local markers** (consistent with the current OpenCode branch): `.mcp.json`/`.claude/` (claude-code), `opencode.json`/`.opencode/` (opencode), `.codex/` (codex), `.gemini/` (gemini). An undetected harness is a no-op.

**Command surface (decided):** a distinct `unimatrix wire` verb, with `--harness <name>` for targeted wiring — chosen for explicit intent per #960 and a natural home for harness selection.

**Intent model (decided):** detection triggers wiring, but writing a **new** MCP/retrieval entry into a user-owned config (`opencode.json`, `.codex/config.toml`) requires explicit `--harness`/opt-in (aligns with #960). Merging into surfaces the user already has stays additive.

**C14 verification (decided):** each harness leg asserts a working return path — after wiring, a `context_*` retrieval call against the wired slug returns — and, where feasible per harness, that hooks fire. Feasibility per harness is stated explicitly in Acceptance Criteria.

## Acceptance Criteria

**Definition install (skills only)**
- **AC-01**: `unimatrix init` never overwrites an existing Unimatrix-owned **skill** file by default; a modified existing skill survives a re-run byte-for-byte.
- **AC-02**: `unimatrix init --force` overwrites Unimatrix-owned **skill** files with the shipped versions; foreign (non-Unimatrix) files under the same directories are never touched. `--force` is **definitions-only** — it never re-asserts or overwrites wiring, which stays always-additive.

**Wiring (`unimatrix wire`)**
- **AC-03**: `unimatrix wire` ensures MCP + hooks + retrieval for every detected harness and writes/modifies **zero** definition files.
- **AC-04**: Running the wire path twice in succession produces byte-identical config output for every supported harness (idempotent).
- **AC-05**: When OpenCode is detected, wiring ensures the `mcp.unimatrix` retrieval entry in `opencode.json`; an existing `mcp.unimatrix`, the Ollama `provider` block, and all foreign keys are preserved byte-for-byte (vnc-049 sentinel).
- **AC-06**: When Codex is detected, wiring writes/merges `[mcp_servers.unimatrix]` into project-local `.codex/config.toml`, preserving foreign `[mcp_servers.*]` tables and all other TOML keys.
- **AC-07**: When Codex is detected, wiring provisions Codex hooks in a **Claude-like, event-driven shape** with `--provider codex-cli` on every command, preserving foreign hook entries.

**Codex hook install behavior + firing**
- **AC-08**: The package writes Codex hook commands that target the **JS hook client** (`packages/unimatrix/lib/hook-client/`), not the unimatrix binary — asserted against the written command, not just its presence.
- **AC-09**: Package-wired Codex hooks are verified to **actually fire** — a behavioral assertion that a wired Codex hook event reaches the JS hook client, not merely that hook config is present. Asserted in **both local and cloud** deployment where feasible.

**C14 parity — return path**
- **AC-10**: After wiring, a `context_*` retrieval call against the wired slug **returns** (not merely that config was written), asserted in **both local and cloud** deployment where feasible. Coverage and feasibility per harness:
  - **claude-code** — return-path assertion required (stdio MCP retrieval).
  - **codex-cli** — return-path assertion required (`.codex/config.toml` stdio MCP retrieval, trust-gated environment).
  - **opencode** — return-path assertion required via the `mcp.unimatrix` entry; hook activity is via the in-process TS plugin (observation), so opencode's "firing" assertion is plugin-level, not JS-hook-client-level.

**Intent model (#960)**
- **AC-11**: Writing a **new** MCP/retrieval entry into a user-owned config (`opencode.json`, `.codex/config.toml`) requires explicit `--harness`/opt-in; merging into a surface the user already has stays additive and needs no extra opt-in.

**Cross-cutting**
- **AC-12**: No wiring or definition-install path writes to any config outside the resolved project root (asserted for `~/.codex/`, `~/.claude/`, `~/.gemini/`, `~/.config/`).
- **AC-13**: Harness wiring is gated on project-local detection markers; a harness with no marker present produces no writes and no error.
- **AC-14**: Both definition install and wiring honor `--dry-run`, printing intended actions with a `[dry-run]` prefix and writing nothing.
- **AC-15**: A malformed config for any harness (invalid JSON/TOML) is preserved unchanged and that leg is skipped with a warning; init/wire never throws or partially writes because of it.
- **AC-16**: The command surface and per-harness wiring behavior are consistent with #960 — no path silently installs a surface the user did not intend; ambiguous invocations surface help rather than falling through.

## Constraints

- **Project-scoped only** — root resolved via the existing `.git` walk (`detectProjectRoot`); never write global/user config.
- **New TOML dependency/writer** — Codex `.codex/config.toml` is TOML; the JS installer currently has no TOML reader/writer. Format-preservation for foreign keys is required; choice of library vs. minimal in-house writer is a design decision.
- **Codex hooks target the JS hook client** — the package writes every Codex hook command to invoke `packages/unimatrix/lib/hook-client/` (mirroring Claude), never the unimatrix binary. This is the intended install output.
- **JS-client install path + Codex hook wiring must function in both local and cloud deployment** — binding. The client-init install changes (delivered via the JS client / dogfood mechanism), including Codex hook wiring and the retrieval return path, must work under both local and cloud deployment; ACs assert both where feasible.
- **Codex trust-gating** — project-local `.codex/config.toml` and hooks load only for a trusted `.codex/` layer. Truthful operational constraint; the ship target is functional hooks + returning retrieval, and firing/return assertions run in a trusted environment.
- **Codex `--provider codex-cli` is mandatory** on every hook command (vnc-013 ADR-006) — attribution correctness depends on it.
- **C14 return path is load-bearing** — a leg is not "wired" for parity purposes unless a `context_*` retrieval call returns after wiring; config presence alone is insufficient.
- **vnc-049 retrieval sentinel** — `mcp.unimatrix` + Ollama `provider` block in `opencode.json` must survive byte-for-byte; regression test asserts retrieval still returns after wiring.
- **Non-clobbering / fail-safe** — every harness leg warns-and-skips on malformed input, never throws mid-init, mirrors `opencode-install.js`.
- **Backward compatibility** — the existing claude-code `.mcp.json` + `.claude/settings.json` output must remain byte-identical for the common path; definition install-if-absent must not break first-run installs.
- **Idempotence + dry-run + containment guard (`isWithinProject`)** apply to every new writer.

## Decided (formerly Open Questions)

1. **Command surface** — DECIDED: a distinct `unimatrix wire` verb, with `--harness <name>` for targeted wiring.
2. **Intent model** — DECIDED: detection triggers wiring, but writing a **new** MCP/retrieval entry into a user-owned config (`opencode.json`, `.codex/config.toml`) requires explicit `--harness`/opt-in (aligns with #960). Merging into a surface the user already has stays additive.
3. **Definition-install scope** — DECIDED: **skills only** for now; protocols and agents stay out.
4. **Codex hook shape** — DECIDED: mirror the Claude hooks shape/approach (event-driven, `type: "command"`, `--provider codex-cli` on every command), targeting the JS hook client.
5. **Codex trust-gating** — DECIDED: Codex hooks are a working/verifiable leg, not "wired-but-inactive." Trust-gating is noted as a truthful operational constraint; firing/return assertions run in a trusted environment.
6. **gemini-cli** — DECIDED: deferred, out of scope this feature.
7. **#16732** — DECIDED: FIXED upstream; removed as a blocker/non-goal. Codex MCP-tool-call hooks fire.
8. **`--force` blast radius** — DECIDED: `--force` is definitions-only; wiring stays always-additive.

## Open Questions

- **TOML writer implementation** — library vs. minimal in-house writer for `.codex/config.toml`, subject to the format-preservation requirement. Design-phase decision, not a scope blocker.
- **Codex hook event coverage** — which specific Codex lifecycle events the writer emits (the exact Claude-mirroring event set). Design-phase detail within the decided Claude-like approach.

## Tracking

- GitHub Issue: **#990** — `feat(nan-023): install options — per-harness idempotent wiring, non-destructive definition install (C14 parity)`.
- Capability: **C14 (multi-LLM harness parity)** — the outcome this feature proves (wiring + returning retrieval + firing hooks across claude-code, opencode, codex-cli).
- Related/evolved: **C17 provisioning (#5582)** — server-side config seeding, already proven; cross-link only. #960 (explicit-intent install), vnc-049 / ADR-006 (#5743, OpenCode wiring + retrieval sentinel).
- Goal: `personal-cloud` (#4946).
- GH Issue link to be updated after Session 1.
