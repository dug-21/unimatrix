# nan-023 Pseudocode — OVERVIEW

> Feature: nan-023 · Capability: C14 (multi-LLM harness parity) · Goal: `personal-cloud` (#4946)
> Source of truth: ARCHITECTURE.md, ADR-001..006, SPECIFICATION.md, RISK-TEST-STRATEGY.md, ACCEPTANCE-MAP.md.
> This is the thin interaction/data-flow map. Each component's algorithm lives in its own file.

## Components (Component Map)

| Component | File(s) touched | Pseudocode |
|-----------|-----------------|-----------|
| Wire orchestrator | `lib/wire.js` **new** | `wire-orchestrator.md` |
| Skills installer | `lib/init.js` (`copySkills`→`installSkills`), thin `wire()` call | `skills-installer.md` |
| opencode retrieval writer | `lib/opencode-install.js` (`writeOpencodeMcp` **new fn**) | `opencode-retrieval.md` |
| Codex installer | `lib/codex-install.js` **new** | `codex-install.md` |
| Surgical TOML helper | `lib/codex-install.js` internal (`upsertTomlTable`/`readTomlTable`) | `toml-surgical.md` |
| hook-client `--provider` hint | `lib/hook-client/index.js` (`parseHookArgs`), `lib/merge-settings.js` (`buildHookClientCommand` 3rd arg) | `hook-client-provider.md` |
| CLI routing | `bin/unimatrix.js` | `cli-routing.md` |
| C14 verifier | `test/` **new** | `c14-verifier.md` |

## Shared Type — `WireLeg` (the manifest element)

The single contract every writer returns and the verifier + dry-run consume (ADR-001 §3, SR-09). It defeats path-divergence: what is *asserted* and *printed* is exactly what was *written*.

```
WireLeg = {
  harness:  "claude-code" | "opencode" | "codex-cli",
  surface:  "mcp" | "retrieval" | "hooks" | "plugin",
  action:   "created" | "updated" | "unchanged"
          | "skipped-undetected" | "skipped-malformed" | "skipped-intent",
  path:     string,     // absolute file the writer targeted
  command?: string,     // hooks ONLY: the EXACT command string written (e.g. "node <abs>/lib/hook-client/index.js PreToolUse --provider codex-cli")
  entry?:   object,     // mcp/retrieval ONLY: the exact entry object written (used by the verifier to spawn/connect the MCP target)
  reason?:  string      // REQUIRED for any "skipped-*"; a human-visible explanation (SR-08 — never a silent skip)
}
```

Invariants (assert on every dispatch path, including all `skipped-*`):
- `harness` and `surface` always set; `action` always one of the six literals.
- `command` present iff `surface === "hooks"` and `action ∈ {created, updated, unchanged}`.
- `entry` present iff `surface ∈ {mcp, retrieval}` and `action ∈ {created, updated, unchanged}`.
- `reason` present iff `action` starts with `skipped-`.
- A well-formed `WireLeg` is returned even for skips — the verifier and the summary printer both iterate the manifest; a missing/malformed leg breaks the contract (integration risk).

## Transport descriptor (local vs cloud) — resolved by the orchestrator

`wire()` receives `binaryPath?` (local) and/or `mcp?:{url, token}` (cloud). Writers do not decide deployment; the orchestrator resolves one **transport descriptor** and hands each MCP/retrieval writer the concrete shape to emit:

```
Transport =
  | { kind: "stdio-binary", binaryPath }                    // LOCAL: command = <absolute binaryPath>
  | { kind: "stdio-bridge", bridgePath, projectHash }       // CLOUD: command = "node", args = [bridgePath, projectHash] — token-free (Q1)
```

- Cloud NEVER emits `url = "<mcpUrl>"` into `.codex/config.toml` — a bearer token in TOML violates Principle 8 (ADR-006 Q1, R-03). Cloud codex mirrors the proven token-free `.mcp.json` bridge (`init.js:257`): the bridge resolves the credential from `projectHash`, so no secret enters any file.
- claude-code `.mcp.json` keeps its existing local (`writeMcpJson`) and cloud (`writeMcpBridgeEntry`) writers unchanged — those are reused, not re-derived.

> **OPEN QUESTION (flag for Stage 3b / architect):** The Integration-Surface signatures state `writeOpencodeMcp(dir, {binaryPath?, url?}, dryRun)` and `writeCodexMcpToml(dir, {binaryPath?, url?}, dryRun)`. Q1 (DECIDED) forbids emitting `url=` for codex cloud and mandates the token-free bridge form, which needs `bridgePath` + `projectHash`, not `url`. The per-component pseudocode honors the DECIDED Q1 shape by threading a `Transport` descriptor into the MCP writers in place of a bare `url`. Recommend the implementer extend the codex/opencode MCP writer opts to carry `{ transport }` (or `{ bridgePath, projectHash }`) rather than `url`. This is a signature reconciliation, not a behavior change — the emitted bytes are the Q1-decided bytes either way.

## Data flow

```
bin/unimatrix.js
  ├─ "init"  → init.init(opts)
  │             local:  resolveRoot → resolveBinary → writeMcpJson → mergeSettings
  │                     → installSkills(force?) → wire(root,{clientPath,binaryPath,dryRun}) → DB/validate
  │             cloud:  initRemote(...) → ... → installSkills(force?) → wire(root,{clientPath,mcp:{url,token},dryRun})
  └─ "wire"  → init/wire path: resolveRoot → wire(root,{harness?,clientPath,binaryPath?|mcp?,dryRun})
                                (NO installSkills, NO DB — ADR-005)

wire(root, opts) -> { actions: string[], manifest: WireLeg[] }
  1. detectHarnesses(root)                 // project-local markers only (AC-13)
  2. resolve Transport (local|cloud)       // from binaryPath | mcp
  3. select target harnesses               // --harness filter, else all detected
  4. for each target harness:
       intentGate(harness, surface, existingConfig, harnessFlag)   // ADR-005 (AC-11)
       dispatch writer(s) → each returns a WireLeg
  5. manifest = [ ...all WireLegs ]         // the verifier's & printer's sole input (ADR-006)
  6. actions = manifest.map(legToActionLine)   // "[dry-run] " prefix applied when dryRun
  return { actions, manifest }
```

### Per-harness surfaces & writers

| Harness | Surface | Writer | Status |
|---------|---------|--------|--------|
| claude-code | mcp | `writeMcpJson` / `writeMcpBridgeEntry` (reuse) | wraps result into a `WireLeg` in `wire.js` |
| claude-code | hooks | `mergeSettings` (reuse) | wraps into `WireLeg` |
| opencode | retrieval | `writeOpencodeMcp` **new** | `WireLeg` |
| opencode | plugin | `maybeProvisionOpenCode` (reuse) | wraps into `WireLeg(s)` |
| codex-cli | mcp | `writeCodexMcpToml` **new** | `WireLeg` |
| codex-cli | hooks | `writeCodexHooks` **new** | `WireLeg` |

`maybeWireCodex` fans out to `writeCodexMcpToml` + `writeCodexHooks` and returns `WireLeg[]`.

### Error posture (two coexisting, documented boundaries — ADR-001 §5)

- **Wire-layer writers are fail-safe**: warn-and-skip on malformed input, NEVER throw (AC-15). Malformed → `action:"skipped-malformed"`, file byte-preserved. Mirrors `opencode-install.js`.
- **init's pre-existing claude checkpoints stay loud**: `writeMcpJson`/`mergeSettings` still throw on malformed `.mcp.json`/`settings.json` (backward compat, SR-07). This throw is caught in `bin` (init is interactive). The boundary is documented so a swallowed wire-layer warning is never read as success.
- Every writer is containment-guarded by `isWithinProject`; a path escaping root → `skipped`, never followed (AC-12).

## Wave / dependency ordering (for Stage 3b planning)

Build order is driven by leaf-first dependencies. Legend: → "depends on".

```
Wave A (leaves, no intra-feature deps):
  A1  toml-surgical         (upsertTomlTable / readTomlTable)     — pure string ops
  A2  hook-client-provider  (parseHookArgs + buildHookClientCommand 3rd arg)  — size-gated, guard first
  A3  skills-installer      (installSkills)                        — self-contained in init.js

Wave B (writers, depend on Wave A + reused helpers):
  B1  opencode-retrieval    (writeOpencodeMcp)      → reuses readJsonSafe/detectIndent/isWithinProject
  B2  codex-install         (writeCodexMcpToml, writeCodexHooks, maybeWireCodex)
                            → A1 (toml-surgical), A2 (buildHookClientCommand 3rd arg)

Wave C (orchestration + routing):
  C1  wire-orchestrator     (wire, detectHarnesses, intent gate, WireLeg aggregation)
                            → B1, B2, and reused writeMcpJson/mergeSettings/maybeProvisionOpenCode
  C2  cli-routing           (wire verb, --harness, --dry-run; --force gates installSkills only)
                            → C1, A3

Wave D (verification, depends on everything producing a manifest):
  D1  c14-verifier          → C1 (consumes WireLeg manifest), A2 (fires the exact command)
```

Golden-file capture (SR-07) happens BEFORE C1 lands (before the claude-code writers are routed through `wire.js`) — see `c14-verifier.md` §Golden.

Gate-0 (codex trust feasibility, Q2) is confirmed by the tester BEFORE Wave A — it decides whether AC-09/AC-10 codex-cloud is HARD or documented-conditional.

## Integration surfaces (exact names — do not reinvent; from ARCHITECTURE Integration Surface)

Reused (existing): `detectProjectRoot`, `writeMcpJson`, `writeMcpBridgeEntry`, `mergeSettings`, `buildHookClientCommand`, `isUnimatrixHook`, `UNIMATRIX_PATTERNS`, `maybeProvisionOpenCode`, `isWithinProject`, `readJsonSafe`, `detectIndent`, `resolveBinary`, `KNOWN_PROVIDERS`, `computeProjectHash` (`hook-client/config.js`), `normalize.normalizeEventName` / `EVENT_MATCHERS` / `PRETOOLUSE_CYCLE_MATCHER`.

New (this feature): `wire`, `detectHarnesses`, `WireLeg`, `installSkills`, `writeOpencodeMcp`, `maybeWireCodex`, `writeCodexMcpToml`, `writeCodexHooks`, `upsertTomlTable`, `readTomlTable`, `parseHookArgs`, `buildHookClientCommand(clientPath, event, providerHint?)`.

## Modularity budget (do not exceed — ARCHITECTURE §Modularity)

| File | Now | After | Gate |
|------|-----|-------|------|
| `lib/wire.js` | new | < 300 | 1,000-line cap |
| `lib/codex-install.js` | new | < 450 | 1,000-line cap; NOT under hook-client size gate |
| `lib/init.js` | 691 | < 800 (+`installSkills`, thin `wire()` call) | do NOT absorb wire/codex logic |
| `lib/opencode-install.js` | 362 | +~70 | 1,000-line cap |
| `lib/hook-client/index.js` | 464 / raw 189,946 B | lean `parseHookArgs` | **hook-client SIZE GATE — 110 KB stripped PRIMARY / 200 KB raw backstop; ~10 KB raw headroom; NEVER raise the gate (#5372, #4780)** |
| `lib/merge-settings.js` | 417 | +~3 | 1,000-line cap |
