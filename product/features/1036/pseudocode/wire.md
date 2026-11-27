# C6 — Orchestrator (`lib/wire.js`, shrinks)

## Purpose
Transport → preflight → installStore → resolve dirs → plan → run → render. No paths built here (FR-01) except project-relative ones used for the no-transport legs.

## Deleted
`gitIgnored`, `GITIGNORE_WARNING` (and export), `dispatchClaude`, `dispatchOpencode`, `dispatchCodex`, `removeProjectFileMcp`, `storeLegs` regex parsing, `execFileSync` and `isDeepStrictEqual` imports.

## wire(projectRoot, opts)

```
function wire(projectRoot, opts) -> {actions, manifest, warnings}
  o = opts||{}; home = o.home||os.homedir(); platform = o.platform||process.platform
  env = o.env||process.env; dryRun = o.dryRun===true; harnessSel = o.harness||null
  if harnessSel and !HARNESSES.includes(harnessSel): return {actions:["unknown --harness: "+harnessSel], manifest:[], warnings:[]}
  dirs = resolveConfigDirs({env, home, platform})                    // once (FR-07)
  warnings = [...dirs.warnings]
  transport = resolveTransport(projectRoot, o)
  manifest = []
  if transport.kind === "none":
     reason = "no transport (binaryPath/credential absent)"
     manifest.push(skippedLeg("mcp", dirs.claude.claudeJson, reason), skippedLeg("hooks", join(projectRoot,".claude","settings.local.json"), reason))
     emitWarnings(warnings)
     return {actions: manifest.map(l => legToActionLine(projectRoot,l,dryRun)), manifest, warnings}
  node = preflight as today (warnings.push(pf.warning))
  store = installStore({...as today})
  manifest.push(...store.legs)                                       // C12 now returns legs (FR-26a)
  steps = buildWirePlan({root:projectRoot, home, platform, env, node, dirs, harnessSel,
                         writeMcp: o.mcp!==false && !o.hooksOnly, hooksOnly: !!o.hooksOnly, dryRun})
  r = runWirePlan(steps, {dryRun})
  manifest.push(...r.manifest); warnings.push(...r.warnings)
  actions = []
  actions.push(...resolvedLines(dirs, steps, harnessSel, dryRun))     // FR-50
  note = sameEnvNote(dirs, steps); if note: actions.push(pre(dryRun, note))   // FR-51
  actions.push(...manifest.map(l => legToActionLine(projectRoot, l, dryRun)))
  n = claudeRunningNotice(manifest, dirs.claude.claudeJson); if n: actions.push(pre(dryRun, n))   // FR-52
  for w in warnings: actions.push(w)                                 // FR-53 "actions list"
  emitWarnings(warnings)                                             // stderr, best effort
  return {actions, manifest, warnings}
```

Note: today warnings are not pushed into `actions` except the gitignore one. FR-53 requires warnings in the actions list; init's `printSummary` must not double-print (check C13 note).

```
function resolvedLines(dirs, steps, harnessSel, dryRun) -> string[]
  // one line per destination of each harness that has a step in the plan
  has = h => steps.some(s => s.harness === h)
  src = b => b.source === "default" ? "default" : `${b.source}=${b.value}`
  L = []
  if has("claude-code"):
     if steps.some(s=>s.id==="W2"): L.push(`resolved: claude-code mcp -> ${dirs.claude.claudeJson} (${src(dirs.claude)})`)
     L.push(`resolved: claude-code hooks -> ${dirs.claude.userSettings} (${src(dirs.claude)})`)
  if has("codex-cli"):
     L.push(`resolved: codex-cli hooks -> ${dirs.codex.hooksJson} (${src(dirs.codex)})`)
     L.push(`resolved: codex-cli mcp -> ${dirs.codex.configToml} (${src(dirs.codex)})`)
  if steps.some(s=>s.id==="W5"): L.push(`resolved: opencode mcp -> ${dirs.opencode.configJson} (${src(dirs.opencode)})`)
  return L.map(x => pre(dryRun, x))

function sameEnvNote(dirs, steps)
  vars = unique sources (≠ default) among harnesses with a step
  return vars.length ? `note: start each harness with the same ${vars.join(", ")} so it reads the files above; a harness started without them reads the default location` : null

function claudeRunningNotice(manifest, claudeJson)
  hit = manifest.some(l => l.harness==="claude-code" and l.path===claudeJson and ["created","updated","removed"].includes(l.action))
  return hit ? `If Claude Code was running during this install, it may overwrite ${claudeJson}; re-run 'npx unimatrix init' or check 'npx unimatrix status'` : null

pre(dryRun, s) = dryRun ? "[dry-run] " + s : s
```

`legToActionLine` unchanged (FR-46 rendering already `harness surface: action path (reason)`).

## Exports
`{wire, detectHarnesses, resolveTransport, legToActionLine, HARNESSES}`. Tests importing `GITIGNORE_WARNING` must be updated (C17).

## Error handling
Everything thrown from installStore / runWirePlan propagates; init prints `unimatrix init failed: <msg>` and exits 1 (existing handler in init.js). Verify that handler prints `e.message` for `SafeWriteError` (message already names file).

## Key tests
- No-transport path uses `dirs.claude.claudeJson` (CLAUDE_CONFIG_DIR honoured).
- Resolved lines + note identical between dry-run and real (prefix only).
- D-013 notice once when `.claude.json` created/updated/removed; absent on all-unchanged second run.
- Relative override: warning in actions and stderr; destinations as no-override.
