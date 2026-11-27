# C4 — Wiring plan (`lib/wire-plan.js`, new)

## Purpose
One ordered step table drives writes, removals, gating and dry-run (ADR-004 #5959, FR-10/11/60). Both modes call the same `step.run(dryRun)`.

## Exports
`buildWirePlan(ctx) -> Step[]`, `runWirePlan(steps, {dryRun}) -> {manifest, warnings}`, `OK_ACTIONS`.

## Types
`ctx = {root, home, platform, env, node, dirs: ConfigDirs, harnessSel, writeMcp, hooksOnly, dryRun}`
`Step = {id, harness, surface, file, source, gate?, run(dryRun) -> WireLeg[]}` (§6 signature). Gates are evaluated by the executor from prior legs; `run` never sees them.

## buildWirePlan

```
OK_ACTIONS = new Set(["created","updated","unchanged"])
want = h => !ctx.harnessSel or ctx.harnessSel === h
steps = []; d = ctx.dirs; overrideClaude = d.claude.source !== "default"

if want("claude-code"):
  add W1 {harness:"claude-code", surface:"hooks", file: root/.claude/settings.local.json,
          run: dr => C8.wireProjectHooks({root, platform, home, dryRun: dr})}            // returns legs (one per command)
  if ctx.writeMcp:
    add W2 {surface:"mcp", file:d.claude.claudeJson, source:d.claude.source,
            run: dr => C7.claudeMcpLegs({file:d.claude.claudeJson, projectRoot:root, entry:buildMcpEntry(platform,home,node), platform, dryRun:dr})}
            // emits mcp leg + (R1) removed leg reason "owned user-scope MCP entry removed" from the same mutate
  add R2 {surface:"hooks", file: root/.claude/settings.json, gate:"W1",
          run: dr => C8.removeOwnedHooksLeg(file, {dryRun:dr, containWithin:root, reason:"owned hooks removed (cross-scope)"})}
  add R3 {file:d.claude.userSettings, gate:"W1", run: dr => C8.removeOwnedHooksLeg(d.claude.userSettings, {dryRun:dr, reason:"owned hooks removed (cross-scope)"})}
  if overrideClaude and !sameRealFile(d.legacy.userSettings, d.claude.userSettings):
    add R4 {file:d.legacy.userSettings, gate:"W1", run: dr => C8.removeOwnedHooksLeg(d.legacy.userSettings, {dryRun:dr, reason:"owned hooks removed (legacy location)"})}
  if !ctx.hooksOnly:
    add R6 {surface:"mcp", file:root/.mcp.json, run: dr => C7.removeOwnedMcpJsonLeg(root, dr)}    // no gate (D-060)

if !ctx.hooksOnly and want("codex-cli"):
  add CX {harness:"codex-cli", file:d.codex.dir, run: dr => C9.maybeWireCodex({home, env, platform, node, projectRoot:root, harnessSel, dryRun:dr, dirs:d})}
  // W3, W4, R7, R8, R9 stay inside maybeWireCodex in that order (existing internal sequence, unchanged gating "none").
  // One step keeps Codex's never-throw-except-SafeWriteError wrapper intact.

if !ctx.hooksOnly and want("opencode") and (harnessSel === "opencode" or detectsOpenCode(root)):
  legacyOc = d.opencode.source !== "default" and !sameRealFile(d.legacy.opencodeJson, d.opencode.configJson) ? d.legacy.opencodeJson : null
  add W5 {harness:"opencode", surface:"retrieval", file:d.opencode.configJson, source:d.opencode.source,
          run: dr => [C10.writeOpencodeMcp({file:d.opencode.configJson, legacyFile: legacyOc, platform, home, node, harnessSelected: harnessSel==="opencode", dryRun:dr})]}
  if legacyOc: add R10 {file:legacyOc, gate:"W5", run: dr => C10.removeOwnedOpencodeUserMcpLeg(legacyOc, dr)}

if !ctx.hooksOnly:
  // R11+R12: project opencode.json, regardless of opencode detection (today's removeProjectFileMcp behaviour)
  add R11 {harness:"opencode", surface:"mcp", file:root/opencode.json, run: dr => C10.removeOwnedOpencodeProjectLegs(root, dr)}

if want("claude-code"):
  add X1 {harness:"claude-code", surface:"git-exclude", gate:"W1", file:root,
          run: dr => [C5.ensureSettingsLocalIgnored(root, {dryRun:dr})]}
return steps
```

Order equals ARCHITECTURE §3.3 after store: W1, W2(+R1), R2, R3, R4, R6, Codex, W5, R10, R11(+R12), X1.

Note on today's behaviour: OpenCode project removal currently runs after Codex/OpenCode together with `.mcp.json` removal; moving R6 before Codex is the architecture order — legs order changes, contents don't.

Dedup: before returning, drop any removal step whose `file` is `sameRealFile` with an earlier step's destination file in the same harness (e.g. `root === home` makes R2 vs R3 different files still; R3 vs W1 never same). Apply for R2/R3/R4 pairwise: if R4 file same as R3 → drop R4 (already done by condition); if R2 same as R3 (project root is `CLAUDE_CONFIG_DIR` parent edge case) → drop R3.

## runWirePlan

```
function runWirePlan(steps, {dryRun}) -> {manifest, warnings}
  manifest=[]; warnings=[]; byId = {}
  for s in steps:
    if s.gate:
      g = byId[s.gate]
      if !g or !g.length or !g.every(l => OK_ACTIONS.has(l.action)):
        // W2 skipped-foreign does not gate R1: R1 is inside W2's mutate (only removed when W2 not skipped-foreign)
        legs = [{harness:s.harness, surface:s.surface, action:"skipped-dest-unwired", path:s.file,
                 reason:`destination ${g && g[0] ? g[0].path : s.gate} not wired`}]
        byId[s.id]=legs; manifest.push(...legs); continue
    legs = s.run(dryRun)          // throws => propagate; nothing later runs (SR-11)
    warnings.push(...(legs.warnings || []))          // warnings with no leg (foreign-at-key, D-065)
    for l in legs: if l.warning: warnings.push(l.warning); if s.source && !l.source: l.source = s.source
    byId[s.id]=legs; manifest.push(...legs)
  return {manifest, warnings}
```

Gate semantics: W1 legs are one per command, all with the same action. A W1 failure throws (exit 1), so `skipped-dest-unwired` from W1 occurs only if W1 returns a skip leg (not today; kept for completeness). W5 `skipped-intent|skipped-foreign|skipped-malformed` → R10 `skipped-dest-unwired`.

## Error handling
No catching. `SafeWriteError`, `ClaudeJsonError`, parse-abort Errors propagate to `wire()`.

## Key tests
- Table-driven: for each fixture, `buildWirePlan` step ids equal expected list (no override; each override; same-file override → no R4/R10).
- Gate: W5 `skipped-intent` → R10 `skipped-dest-unwired` leg, legacy file bytes unchanged (both modes).
- Thrown error in W1 → no later step runs (R3 file untouched).
- Dry-run manifest deep-equals real manifest (AC-9) across all fixtures.
- `alternates` never produce a step (assert no step file equals any alternate file).
