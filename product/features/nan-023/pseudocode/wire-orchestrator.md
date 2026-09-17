# Component: Wire Orchestrator — `lib/wire.js` (new, < 300 lines)

> ADR-001 (per-harness wiring layer + manifest), ADR-005 (intent gate). Orchestrator ONLY: detect → intent-gate → dispatch → aggregate. No harness file-format logic inlined here (that lives in the writers).

## Purpose

Single entry the CLI and `init` both call. Detects which harnesses a consumer repo has, applies the #960 intent gate per surface, dispatches the per-`(harness × surface)` writers, and aggregates every `WireLeg` into the manifest that the C14 verifier and `--dry-run` printer consume. Returns `{ actions, manifest }`. Never throws for a wire leg (fail-safe posture; AC-15).

## Imports (reuse — do not reinvent)

```
detectProjectRoot            from ./init.js            (caller usually passes root already resolved)
writeMcpJson, writeMcpBridgeEntry  from ./init.js
mergeSettings, buildHookClientCommand, HOOK_EVENTS  from ./merge-settings.js
maybeProvisionOpenCode, isWithinProject, readJsonSafe  from ./opencode-install.js
writeOpencodeMcp             from ./opencode-install.js   (new fn, this feature)
maybeWireCodex               from ./codex-install.js      (new, this feature)
computeProjectHash           from ./hook-client/config.js (cloud bridge hash)
```

## Public signatures (Integration Surface — exact)

```
wire(projectRoot, { harness?, clientPath, binaryPath?, mcp?, dryRun }) -> { actions: string[], manifest: WireLeg[] }
detectHarnesses(dir) -> { "claude-code": bool, "opencode": bool, "codex-cli": bool }
```

`mcp` (when present) = `{ url, token }` → cloud deployment. `binaryPath` present → local. Exactly one of the two governs the transport (see OVERVIEW Transport descriptor).

## `detectHarnesses(dir)` — project-local markers only (FR-15, AC-13)

```
function detectHarnesses(dir):
  # Best-effort existence checks; never throw (mirror opencode detectsOpenCode).
  claudeCode = existsSafe(dir/".mcp.json") OR existsSafe(dir/".claude")
  opencode   = existsSafe(dir/"opencode.json") OR existsSafe(dir/".opencode")
  codexCli   = existsSafe(dir/".codex")
  # gemini (.gemini) is intentionally NOT detected — out of scope.
  return { "claude-code": claudeCode, "opencode": opencode, "codex-cli": codexCli }

function existsSafe(p): try return fs.existsSync(p) catch return false
```

Notes:
- claude-code is the historically-wired common path. Even when no marker exists on a truly fresh repo, `init` (not `wire`) still ensures the claude-code common path via the reused writers — detection here governs the `wire` verb and the codex/opencode intent gate, not claude-code back-compat (ADR-005 §3).
- A marker present but the config file absent/empty is still "detected"; the writer then *creates* the entry (subject to the intent gate for user-owned configs).

## Transport resolution

```
function resolveTransport(opts):
  if opts.binaryPath is set:
    return { kind: "stdio-binary", binaryPath: opts.binaryPath }
  if opts.mcp is set:                          # cloud
    bridgePath  = resolveBridgePath()          # require.resolve("./hook-client/mcp-bridge.js") w/ path fallback (mirror init.js:487)
    projectHash = computeProjectHash(projectRoot)
    return { kind: "stdio-bridge", bridgePath, projectHash }
  # Neither: wire was asked to run with no transport source. Legs that need a
  # command still emit a WireLeg but MCP writers report skipped with reason
  # "no transport (binaryPath/mcp absent)". Do NOT throw.
  return { kind: "none" }
```

## `wire(projectRoot, opts)` — main algorithm

```
function wire(projectRoot, opts):
  dryRun     = opts.dryRun === true
  harnessSel = opts.harness || null            # explicit --harness selects one + is the opt-in signal
  clientPath = opts.clientPath                 # abs path to lib/hook-client/index.js (caller resolves)
  transport  = resolveTransport(opts)
  manifest   = []

  detected = detectHarnesses(projectRoot)

  # 1. Validate an explicit --harness value (ADR-005 §2). Unknown value → NO writes,
  #    surface help (the CLI prints usage; wire returns an empty manifest + a note action).
  if harnessSel is set AND harnessSel NOT in {"claude-code","opencode","codex-cli"}:
    return { actions: [helpLine("unknown --harness: " + harnessSel)], manifest: [] }

  # 2. Decide the target set.
  targets = harnessSel ? [harnessSel] : ["claude-code","opencode","codex-cli"]

  # 3. Dispatch per harness. A named-but-undetected harness → a reported skipped-undetected leg (AC-13), not an error.
  for h in targets:
    if not detected[h]:
      # For an explicit --harness that is undetected, still report it (visible skip).
      # For the "process all detected" path, an undetected harness contributes nothing.
      if harnessSel is set:
        manifest.push(undetectedLeg(h, projectRoot))
      continue
    dispatch(h, projectRoot, { transport, clientPath, harnessSel, dryRun, manifest })

  # 4. Build action lines from the manifest (single source of truth — ADR-001 §4).
  actions = manifest.map(leg => legToActionLine(leg, dryRun))
  return { actions, manifest }
```

### `dispatch(harness, ...)` — per-harness writer fan-out

```
function dispatch(h, root, ctx):
  switch h:
    case "claude-code":
      # Reuse existing writers; wrap their output into WireLegs.
      # NOTE: these writers keep their loud-throw posture (SR-07). init catches;
      # in the wire-only path they are the claude common path and are NOT intent-gated.
      wrapClaudeMcp(root, ctx)      -> push mcp WireLeg
      wrapClaudeHooks(root, ctx)    -> push hooks WireLeg

    case "opencode":
      # retrieval is a NEW entry into a user-owned config → intent-gated (AC-11).
      leg = intentGuardedRetrieval(
              surface="retrieval", harness="opencode",
              cfgPath=root/"opencode.json", cfgKeyPath=["mcp","unimatrix"],
              harnessSel=ctx.harnessSel,
              write=() => writeOpencodeMcp(root, transportOpts(ctx.transport), ctx.dryRun))
      ctx.manifest.push(leg)
      # plugin surface is additive-into-existing-.opencode (not user-owned MCP) → NOT intent-gated.
      for pluginLeg in wrapOpencodePlugin(root, ctx): ctx.manifest.push(pluginLeg)

    case "codex-cli":
      # maybeWireCodex returns WireLeg[] (mcp + hooks). The MCP leg is a NEW entry
      # into a user-owned config → intent-gated. Hooks are additive into .codex/hooks.json (Unimatrix-owned pattern), NOT gated on new-entry, but still only run when codex detected.
      legs = maybeWireCodex(root, {
               clientPath: ctx.clientPath,
               transport:  ctx.transport,        # see OVERVIEW open-question note
               harnessSel: ctx.harnessSel,       # passed so the MCP leg can self-gate
               dryRun:     ctx.dryRun })
      for leg in legs: ctx.manifest.push(leg)
```

## Intent gate (ADR-005 §3, #960, AC-11)

Classify each write into a user-owned MCP/retrieval config as **additive-into-existing** vs **new-entry**.

```
function intentGuardedRetrieval({surface, harness, cfgPath, cfgKeyPath, harnessSel, write}):
  read = readJsonSafe(cfgPath)                 # never throws
  if read.malformed:
    return skippedLeg(harness, surface, cfgPath, "skipped-malformed",
                      "config is not valid JSON — preserved unchanged")
  entryAlreadyPresent = read.present AND hasKeyPath(read.parsed, cfgKeyPath)   # e.g. parsed.mcp?.unimatrix

  if entryAlreadyPresent:
    # additive-into-existing → proceeds without extra opt-in (idempotent update).
    return write()                             # writer returns created/updated/unchanged WireLeg
  else:
    # NEW entry into a user-owned config → requires explicit --harness <that harness>.
    if harnessSel === harness:
      return write()                           # explicit opt-in granted → write the new entry
    else:
      return skippedLeg(harness, surface, cfgPath, "skipped-intent",
                        "new " + harness + " entry needs opt-in: run `unimatrix wire --harness " + harness + "`")
```

Rules:
- claude-code `.mcp.json` is **never** intent-gated (ADR-005 §3, backward compat). `init`/`wire` ensure it as today.
- The codex MCP leg self-gates the same way inside `maybeWireCodex` (it receives `harnessSel`), or the orchestrator gates it before calling — pick one site and keep it single (recommend gating inside `maybeWireCodex` for cohesion, mirroring the opencode call above; document the chosen site so it is asserted once).
- `skipped-intent` is a first-class, visible leg with a help line naming the exact command (SR-11).

## WireLeg constructors (uniform — every path returns a well-formed leg)

```
function undetectedLeg(h, root):
  return { harness:h, surface: primarySurface(h), action:"skipped-undetected",
           path: markerPathFor(h, root), reason: h + " marker not present" }

function skippedLeg(h, surface, path, action, reason):
  return { harness:h, surface, action, path, reason }

# writers themselves construct created/updated/unchanged legs with command?/entry? set.
```

`primarySurface`: claude-code→"mcp", opencode→"retrieval", codex-cli→"mcp".

## Action-line rendering (dry-run parity — NFR-10, AC-14)

The SAME manifest drives real and dry-run output; only the prefix differs, so the action set is identical by construction (defeats R-14 divergence).

```
function legToActionLine(leg, dryRun):
  base = describe(leg)          # e.g. "codex-cli mcp: created .codex/config.toml [mcp_servers.unimatrix]"
                                #      "opencode retrieval: skipped-intent (needs --harness opencode)"
  return dryRun ? "[dry-run] " + base : base
```

## Wrapping reused claude-code writers into WireLegs

`writeMcpJson`/`writeMcpBridgeEntry`/`mergeSettings` return action strings / `{actions,content}`, not WireLegs. Wrap without changing them (SR-07 keeps their bytes identical):

```
function wrapClaudeMcp(root, ctx):
  actions = ctx.transport.kind === "stdio-bridge"
    ? writeMcpBridgeEntry(root, ctx.transport.bridgePath, ctx.transport.projectHash, ctx.dryRun)
    : writeMcpJson(root, ctx.transport.binaryPath, ctx.dryRun)
  action = actions.some(a => a.includes("Created")) ? "created" : "updated"   # unchanged not distinguished by these fns → treat as updated (idempotent bytes)
  entry  = mcpServersUnimatrixEntry(ctx.transport)                            # the exact object written
  return { harness:"claude-code", surface:"mcp", action, path: root/".mcp.json", entry }

function wrapClaudeHooks(root, ctx):
  settingsPath = root/".claude/settings.json"
  result = mergeSettings(settingsPath,
             ctx.transport.kind==="stdio-binary" ? ctx.transport.binaryPath      # legacy string form (local)
                                                  : { events: HOOK_EVENTS, commandForEvent: e => buildHookClientCommand(ctx.clientPath, e) },
             { dryRun: ctx.dryRun })
  # hooks WireLeg for claude carries no single `command` (multi-event); represent
  # the managed set via command? = the SessionStart command as the exemplar the
  # verifier fires, OR omit command? and let the verifier read settings.json for
  # the claude leg. RECOMMEND: emit one hooks WireLeg per managed event so the
  # verifier fires each exact command (keeps the manifest the sole source — ADR-006).
  return legsFromMergeResult(...)
```

> **Design note (flag):** claude-code hooks are multi-event, so a single `WireLeg.command` cannot carry them all. Recommend the claude hooks surface emit one `WireLeg` per managed event (each with its exact `command`), OR the verifier fires the claude leg from `settings.json` directly. The codex hooks surface (`writeCodexHooks`) has the same multi-event shape — resolve consistently. Pseudocode for `c14-verifier.md` assumes **one hooks WireLeg per event** for both claude and codex so the manifest stays the sole execution source (non-tautology, R-02).

## Error handling

- `detectHarnesses`, intent read, and every wire-layer writer are non-throwing (warn-and-skip). A thrown claude-code writer (malformed `.mcp.json`/`settings.json`) propagates in the `init` path only (caught in `bin`), preserving SR-07 back-compat; in the standalone `wire` path the claude writers are still the reused loud ones — document that `wire` on a malformed claude config throws exactly as `init` does today (no behavior change to claude), while opencode/codex legs never throw.
- Any unexpected throw inside `dispatch` for opencode/codex is caught and converted to a `skipped-malformed`/`skipped` leg with the error message (paths only, no secrets) — mirror `provisionOpenCode`'s try/catch.

## Key test scenarios (hints — full plan in test-plan/)

- Multi-harness fixture: all three detected → manifest has claude(mcp,hooks) + opencode(retrieval,plugin) + codex(mcp,hooks) legs; zero definition/DB writes (AC-03).
- `--harness codex-cli` on a repo with no `.codex/` → single `skipped-undetected` leg, exit success (AC-13).
- opencode detected, no `--harness`, `opencode.json` lacks `mcp.unimatrix` → `skipped-intent` leg with help line; `--harness opencode` → retrieval `created` (AC-11 both arms).
- opencode detected, `mcp.unimatrix` already present, no `--harness` → additive update proceeds (unchanged/updated), NOT gated (AC-11 exemption).
- Unknown `--harness foo` → help, empty manifest, nothing written (AC-16).
- Every dispatch path (including all skips) yields a well-formed `WireLeg` (manifest contract).
- Idempotence: `wire` twice → identical manifest actions + byte-identical files (AC-04).
