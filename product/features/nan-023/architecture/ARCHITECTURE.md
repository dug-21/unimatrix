# nan-023 Architecture — Per-Harness Idempotent Wiring + Non-Destructive Definition Install

> Feature: nan-023 (Issue #990) · Capability: C14 (multi-LLM harness parity) · Goal: `personal-cloud` (#4946)
> Scope: `product/features/nan-023/SCOPE.md` · Risk lens: `SCOPE-RISK-ASSESSMENT.md`

## System Overview

`unimatrix init` (the JS installation package under `packages/unimatrix/`, delivered via the
dogfood mechanism) wires a **consumer** repo for Unimatrix. Today it handles each concern with
inconsistent idempotence (MCP/hooks merge cleanly; skills clobber every run) and wires only
claude-code fully, opencode partially (plugin, no retrieval entry), and codex-cli not at all.

nan-023 introduces two changes to that package:

1. **A per-harness wiring layer** — one non-clobbering writer per `(harness × surface)`, driven by a
   new `wire` orchestrator, reusing the `mergeSettings` / `opencode-install` "detect → parse-safe →
   additive/install-if-absent → format-preserving write → warn-and-skip → containment-guard"
   principle. Brings codex-cli to full parity (TOML MCP + JS-hook-client hooks) and closes the
   opencode retrieval gap.
2. **A non-destructive definition-install policy** — `copySkills`' blanket overwrite becomes
   install-if-absent, with `--force` as the only path that overwrites Unimatrix-owned skill files.

The load-bearing architectural stance (SR-09): a leg is "wired" for C14 **only** when a `context_*`
retrieval call **returns** and a hook **fires** *from the exact command the wiring wrote* — never
because config is present. The wiring layer therefore returns a **structured manifest** of the exact
commands/entries it wrote, and the verifier executes that manifest rather than reconstructing paths.
This makes the parity assertion non-tautological.

### Where this fits

```
consumer repo
  ├── .mcp.json                claude-code MCP        (writeMcpJson — exists)
  ├── .claude/settings.json    claude-code hooks      (mergeSettings — exists)
  ├── .claude/skills/          Unimatrix definitions  (installSkills — install-if-absent, NEW policy)
  ├── opencode.json            opencode MCP+plugin[]   (mcp.unimatrix retrieval — NEW; plugin — exists)
  ├── .opencode/plugins/       opencode observation   (opencode-install — exists)
  ├── .codex/config.toml       codex MCP (TOML)       (codex-install — NEW; first non-JSON writer)
  └── .codex/hooks.json        codex hooks (JSON)     (codex-install — NEW; targets JS hook client)

packages/unimatrix/lib/hook-client/index.js   ← every hook command's target (claude + codex)
```

## Component Breakdown

| Component | File | Status | Responsibility |
|---|---|---|---|
| `init` orchestrator | `lib/init.js` | modify (thin) | Local/remote init flow; calls skills-install then the wire layer. No harness logic inlined. |
| Skills installer | `lib/init.js` `copySkills` → `installSkills` | modify | install-if-absent; `--force` overwrites Unimatrix-owned skills only (ADR-004). |
| Wire orchestrator | `lib/wire.js` | **new** | Detect harnesses, apply intent gate (#960), dispatch per-harness writers, aggregate the **wire manifest** (ADR-001/005). |
| claude-code MCP writer | `lib/init.js` `writeMcpJson` | reuse | `.mcp.json` `mcpServers.unimatrix`. Golden-filed before refactor (SR-07). |
| claude-code hooks writer | `lib/merge-settings.js` `mergeSettings` | reuse | `.claude/settings.json` hooks. |
| opencode plugin writer | `lib/opencode-install.js` | reuse | `.opencode/plugins/` + `plugin[]`. |
| opencode retrieval writer | `lib/opencode-install.js` `writeOpencodeMcp` | **new fn** | Additive `mcp.unimatrix` in `opencode.json`; preserves Ollama `provider` sentinel byte-for-byte (ADR-001, SR-06). |
| codex writer | `lib/codex-install.js` | **new** | `.codex/config.toml` `[mcp_servers.unimatrix]` (surgical TOML, ADR-002) + `.codex/hooks.json` (JS-hook-client hooks, ADR-003). |
| Surgical TOML helper | `lib/codex-install.js` (internal) | **new** | Byte-preserving upsert of a single owned TOML table (ADR-002). |
| hook-client `--provider` | `lib/hook-client/index.js` + `merge-settings.js` `buildHookClientCommand` | modify | Parse `--provider <name>` argv → provider **hint** (not inference), so codex events attribute correctly (ADR-003, SR-10). Size-gated. |
| CLI routing | `bin/unimatrix.js` | modify | `wire` verb, `--harness <name>`, `--dry-run`; wire skips definition copy + DB (ADR-005). |
| C14 verifier (tests) | `test/` | **new** | Executes the wire manifest: retrieval-returns + hook-fires, local and cloud (ADR-006). |

## Component Interactions & Data Flow

```
bin/unimatrix.js
  ├─ "init"  → init.js: resolveRoot → resolveBinary → writeMcpJson → mergeSettings
  │             → installSkills(force?) → wire(root, {clientPath, binaryPath, dryRun})   [+DB, validate]
  └─ "wire"  → init.js/wire.js: resolveRoot → wire(root, {harness?, clientPath, binaryPath, dryRun})
                                (NO skills, NO DB)

wire(root, opts) -> { actions, manifest }
  detect(root) → [claude-code?, opencode?, codex-cli?]      (project-local markers only; AC-13)
  for each detected harness:
     intent gate (ADR-005): new-entry-into-user-owned-config requires --harness/opt-in (AC-11)
     dispatch writer(s) → each returns a WireLeg { harness, surface, action, path, command? }
  manifest = [WireLeg...]     ← the C14 verifier's input (ADR-006)
```

`WireLeg` (the manifest element — the contract that defeats path-divergence, SR-09):

```
WireLeg = {
  harness:  "claude-code" | "opencode" | "codex-cli",
  surface:  "mcp" | "retrieval" | "hooks" | "plugin",
  action:   "created" | "updated" | "unchanged" | "skipped-undetected"
          | "skipped-malformed" | "skipped-intent",
  path:     string,             // absolute file written
  command?: string,             // for hooks: the EXACT command string written
  entry?:   object,             // for mcp/retrieval: the exact entry written
  reason?:  string              // for any "skipped-*"
}
```

The verifier and `--dry-run` output both consume this manifest, so what is *asserted*/*printed* is
what was *written*. A skipped leg is a first-class, visibly-reported outcome (SR-08), never a silent
pass.

## Technology Decisions (see ADRs)

- **ADR-001** — Per-harness wiring layer: orchestrator + one non-clobbering writer per `(harness ×
  surface)`; wiring returns a structured manifest.
- **ADR-002** — Codex TOML: **minimal in-house surgical block writer**, not a round-trip library
  (byte-for-byte foreign-key/comment preservation, zero new dependency).
- **ADR-003** — Codex hooks target the **JS hook client** via a new `--provider <name>` argv hint;
  defines the emitted event set.
- **ADR-004** — Definition install-if-absent + `--force`, skills-only.
- **ADR-005** — `unimatrix wire` verb, `--harness` routing, #960 intent model.
- **ADR-006** — C14 verification spine: assert retrieval-returns / hook-fires *from the wired
  command's manifest*, in local and cloud; pre-tag real-server exercise.

## Integration Points

- **JS hook client** (`lib/hook-client/index.js`) — the target of every hook command (claude + codex).
  nan-023 adds `--provider` argv parsing here; size-gated (see Modularity).
- **`normalize.js`** — `KNOWN_PROVIDERS` already includes `codex-cli`, and a `--provider` **hint**
  path already exists for opencode; ADR-003 reuses that hint contract for codex.
- **Rust server attribution** — `--provider` stamps `ImplantEvent.provider`, but `source_domain` is
  event-derived and the hook ingress forces `DEFAULT_HOOK_SOURCE_DOMAIN="claude-code"` (Unimatrix
  #5737). Provider stamping is in scope; deeper per-provider `source_domain` resolution is **not**
  (SCOPE non-goal). Flagged so the firing assertion (AC-09) is read as "event reaches the client with
  the right `provider`", not "source_domain is codex".
- **vnc-049 sentinel** — `opencode.json` `mcp.unimatrix` + Ollama `provider` block must survive
  byte-for-byte; opencode retrieval writer is additive only (ADR-001, SR-06).
- **`.git`-walk root** (`detectProjectRoot`) — the single project-scope oracle; every writer is
  containment-guarded by `isWithinProject` (AC-12). Assumed identical local vs cloud (SR-12).

## Integration Surface

Exact names/types so downstream agents do not invent them.

### Existing (reuse — do not reinvent)

| Integration Point | Type / Signature | Source |
|---|---|---|
| Project root | `detectProjectRoot(startDir: string) -> string` | `lib/init.js` |
| claude MCP writer | `writeMcpJson(projectRoot, binaryPath, dryRun) -> string[]` | `lib/init.js` |
| claude hooks merge | `mergeSettings(filePath, commandSource, {dryRun}) -> {actions:string[], content:object}` | `lib/merge-settings.js` |
| hook command builder | `buildHookClientCommand(clientPath, event) -> string` (`"node <path> <EVENT>"`) | `lib/merge-settings.js` |
| ownership test | `isUnimatrixHook(hookEntry) -> boolean` | `lib/merge-settings.js` |
| ownership regex | `UNIMATRIX_PATTERNS` (pattern 5 matches `node …/hook-client/index.js <EVENT> …`) | `lib/merge-settings.js` |
| opencode plugin | `maybeProvisionOpenCode(dir, {dryRun}) -> string[]` | `lib/opencode-install.js` |
| containment guard | `isWithinProject(dir, target) -> boolean` | `lib/opencode-install.js` |
| safe JSON read | `readJsonSafe(filePath) -> {present, parsed, raw, malformed}` | `lib/opencode-install.js` |
| indent preserve | `detectIndent(raw) -> number\|string` | `lib/opencode-install.js` |
| binary resolve | `resolveBinary() -> string` | `lib/resolve-binary.js` |
| hook client entry | `node <lib/hook-client/index.js> <EVENT> [--provider <name>]` | `lib/hook-client/index.js` |
| provider set | `KNOWN_PROVIDERS = ["claude-code","gemini-cli","codex-cli","opencode"]` | `lib/hook-client/normalize.js` |

### New (this feature introduces)

| Integration Point | Type / Signature | File |
|---|---|---|
| Wire orchestrator | `wire(projectRoot, {harness?:string, clientPath:string, binaryPath?:string, mcp?:{url,token}, dryRun:boolean}) -> {actions:string[], manifest:WireLeg[]}` | `lib/wire.js` |
| Wire manifest element | `WireLeg` (see Data Flow) | `lib/wire.js` |
| Harness detection | `detectHarnesses(dir) -> {"claude-code":bool,"opencode":bool,"codex-cli":bool}` | `lib/wire.js` |
| Skills install | `installSkills(projectRoot, {force:boolean, dryRun:boolean}) -> string[]` | `lib/init.js` |
| opencode retrieval | `writeOpencodeMcp(dir, {binaryPath?, url?}, dryRun) -> WireLeg` (additive `mcp.unimatrix`; preserves Ollama/foreign keys) | `lib/opencode-install.js` |
| codex entry (top) | `maybeWireCodex(dir, {clientPath, binaryPath?, url?, dryRun}) -> WireLeg[]` | `lib/codex-install.js` |
| codex MCP (TOML) | `writeCodexMcpToml(dir, {binaryPath?, url?}, dryRun) -> WireLeg` | `lib/codex-install.js` |
| codex hooks | `writeCodexHooks(dir, {clientPath, dryRun}) -> WireLeg` (JSON, targets JS hook client, `--provider codex-cli`) | `lib/codex-install.js` |
| surgical TOML upsert | `upsertTomlTable(raw:string, tablePath:string, body:string[]) -> {text:string, changed:boolean}` | `lib/codex-install.js` (internal) |
| surgical TOML read | `readTomlTable(raw:string, tablePath:string) -> {present:boolean, body:string}` | `lib/codex-install.js` (internal) |
| hook command w/ provider | `buildHookClientCommand(clientPath, event, providerHint?) -> string` (appends `" --provider <hint>"`) | `lib/merge-settings.js` |
| provider argv parse | `parseHookArgs(argv:string[]) -> {event:string, providerHint:(string\|null)}` | `lib/hook-client/index.js` |

### Codex config shapes written

`.codex/config.toml` (local) — surgical upsert of exactly this owned table, foreign tables untouched:

```toml
[mcp_servers.unimatrix]
command = "<absolute binaryPath>"
# cloud variant instead uses:  url = "<mcpUrl>"
```

`.codex/hooks.json` (JSON, same matcher-group shape as `.claude/settings.json`) — each command targets
the JS hook client, **not** the binary, `--provider codex-cli` on every command (ADR-003):

```json
{ "hooks": { "PreToolUse": [ { "matcher": "^context_cycle$|^mcp__unimatrix__context_cycle$",
  "hooks": [ { "type": "command",
    "command": "node <abs>/lib/hook-client/index.js PreToolUse --provider codex-cli" } ] } ] } }
```

### Data flow — where errors originate and propagate

- Per-leg writers are **fail-safe** (warn-and-skip, never throw) mirroring `opencode-install.js`;
  malformed input → `action:"skipped-malformed"`, file preserved (AC-15). The *only* loud checkpoints
  stay where they are today: claude `.mcp.json`/`settings.json` malformed still throws in `init`
  (backward compat, SR-07). Wire-path legs never throw (AC-15).
- Intent-blocked new entries → `action:"skipped-intent"` with a help line (AC-11/16), not a write.
- Every writer is guarded by `isWithinProject`; a path escaping root → `skipped`, never followed (AC-12).

## Modularity (capability PL-10, #693)

Cap: 1,000 code lines/file (tests excluded). Current state and plan:

| File | Now (lines) | Plan | Flag |
|---|---|---|---|
| `lib/init.js` | 691 | Wire logic goes to **new** `lib/wire.js`; codex to **new** `lib/codex-install.js`. `init.js` gains only `installSkills` + a thin `wire()` call (~+60). Stays < 800. | Monitor — **do not** absorb wire/codex logic into init.js. |
| `lib/wire.js` | new | Orchestrator only (detect, intent gate, dispatch, manifest). Target < 300. | OK |
| `lib/codex-install.js` | new | TOML surgical writer + hooks writer. Target < 450. Not under the hook-client size gate. | OK |
| `lib/opencode-install.js` | 362 | +`writeOpencodeMcp` (~+70). | OK |
| `lib/hook-client/index.js` | 464 lines / **hook-client raw total 189,946 B vs 200,000 B backstop** | `--provider` argv parse is small. **The binding constraint is the byte size gate, not the line cap.** | **FLAG (tight):** ~10 KB raw headroom. Add lean code, trim comment prose; **never raise the gate** (Unimatrix #5372, lesson #4780). |
| `lib/merge-settings.js` | 417 | Optional 3rd arg on `buildHookClientCommand` (~+3). | OK |

No file is pushed over 1,000 lines by this design. The single tight resource is the **hook-client
size gate** (`test/check-hook-client-size.js`: 110 KB stripped / 200 KB raw); the `--provider`
addition must be minimal. No ad-hoc mid-delivery refactor is planned; all growth lands as new modules.

## Backward Compatibility & Golden Files (SR-07)

Before any per-harness refactor: **golden-file the current claude-code output** (`.mcp.json` +
`.claude/settings.json`) from a fixture repo and assert byte-identical after refactor. claude-code
writers are *reused* (not rewritten) precisely to keep this trivial. opencode `mcp.unimatrix` +
Ollama `provider` gets its own byte-for-byte sentinel + retrieval-still-returns regression (SR-06).

## Open Questions

1. **Codex MCP transport in cloud** — local uses `command=<binaryPath>`; cloud has no local binary
   (the JS bridge model). Does codex-cli support an `url=` streamable-HTTP MCP entry pointing at the
   same target the `.mcp.json` bridge uses, or must cloud codex retrieval route through a local
   `node <mcp-bridge.js> <hash>` `command`? Resolution needed before AC-10 (codex cloud return path).
   Recommendation: mirror `.mcp.json`'s token-free `command="node" args=[bridge, hash]` shape as a
   TOML `command`/`args` entry — reuses the proven cloud path, no new transport. (For the
   design-leader/human.)
2. **Codex trust-gating in the C14 verifier env** — AC-09/AC-10 require a *trusted* `.codex/` layer.
   The pre-tag real-server exercise (ADR-006) must run in a trusted fixture; confirm the local+cloud
   CI harness can mark `.codex/` trusted, else firing is unmeetable (SR-02). (For the tester/risk-strategist.)
