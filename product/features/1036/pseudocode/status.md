# C15 — Status (`lib/status.js`)

## Purpose
Injectable `collect(ctx)`; all paths from C1; adds `configDirs`, `claudeMcp`, `opencodeConfig`, `staleLegacy`, `alternateProfiles` (ADR-007 #5961, FR-34..FR-38). Read-only; exit 0 always.

## Removed
Private `codexHome`; `path.join(home, ".claude.json")` in `mcpNode`.

## collect(ctx)

```
function collect(ctx = {}) -> Info
  env = ctx.env || process.env; home = ctx.home || os.homedir()
  platform = ctx.platform || process.platform; cwd = ctx.cwd || process.cwd()
  dirs = safe(() => resolveConfigDirs({env, home, platform}), null)    // never throws anyway
  ...existing fields, but:
    hooks["codex-cli"] uses dirs.codex.hooksJson
    mcpNode(dirs, root, platform)   // reads dirs.claude.claudeJson, dirs.codex.configToml
  configDirs = {claude:{claudeJson, userSettings, source}, opencode:{configJson, source}, codex:{hooksJson, configToml, source}, warnings:dirs.warnings}
     // plus `value` per harness kept internal as `_values` for text rendering (not in JSON unless Q3 resolves otherwise)
  claudeMcp = project ? safe(() => C7.readClaudeMcpState(dirs.claude.claudeJson, root, platform), {status:"unreadable", file:dirs.claude.claudeJson, key:null})
                      : {status:"n/a", file:null, key:null}
  opencodeConfig = ocState(dirs.opencode.configJson)       // {status:"ok"|"absent"|"unreadable", file}
  staleLegacy = safe(() => findStale(dirs, root, platform), [])
  alternateProfiles = safe(() => findAlternates(dirs, root, platform), [])
  return {...existing, configDirs, claudeMcp, opencodeConfig, staleLegacy, alternateProfiles, _dirs: dirs}

function ocState(file)
  raw = readText(file); if raw === null: return {status:"absent", file}
  try JSON.parse(raw); return {status:"ok", file} catch: return {status:"unreadable", file}

function findStale(dirs, root, platform) -> {harness, file, key, unreadable?}[]
  out = []
  if dirs.claude.source !== "default" and !sameRealFile(dirs.legacy.userSettings, dirs.claude.userSettings):
     r = readJsonOr(dirs.legacy.userSettings)       // {absent}|{unreadable}|{json}
     if r.unreadable: out.push({harness:"claude-code", file, key:null, unreadable:true})
     else if r.json and hasOwnedCommand(r.json.hooks): out.push({harness:"claude-code", file, key:"hooks"})
  if dirs.opencode.source !== "default" and !sameRealFile(dirs.legacy.opencodeJson, dirs.opencode.configJson):
     r = readJsonOr(dirs.legacy.opencodeJson)
     if r.unreadable: push {harness:"opencode", file, key:null, unreadable:true}
     else if owned(r.json?.mcp?.unimatrix): push {harness:"opencode", file, key:"mcp.unimatrix"}
  return out

function findAlternates(dirs, root, platform) -> {harness, file, key, var}[]
  out = []
  for a in dirs.alternates:
    dest = a.harness==="claude-code" ? dirs.claude.claudeJson : a.harness==="opencode" ? dirs.opencode.configJson
         : join(dirs.codex.dir, basename(a.file))
    if sameRealFile(a.file, dest): continue
    if a.harness === "claude-code":
       r = readJsonOr(a.file); if r.unreadable: push {..., key:null, unreadable:true}; continue  // text "other profile: <file> unreadable (not valid JSON)"
       if a.key.startsWith("projects"):
          k = resolveProjectKey(r.json?.projects, root, platform)
          if owned(r.json?.projects?.[k]?.mcpServers?.unimatrix): push {harness, file:a.file, key:`projects["${k}"].mcpServers.unimatrix`, var:a.var}
       else if owned(r.json?.mcpServers?.unimatrix): push {..., key:"mcpServers.unimatrix"}
    else if a.harness === "opencode": owned(json.mcp.unimatrix) -> push
    else codex: hooks.json → hasOwnedCommand; config.toml → tomlOwned(ci, raw) -> push
  return out
```
`owned(e) = own.isUnimatrixMcpEntry("unimatrix", e)`.

## renderText additions (FR-34/35/38, order after existing lines)

```
src = (b, v) => b.source === "default" ? "default" : `${b.source}=${v}`
L.push(`config claude-code mcp: ${cd.claude.claudeJson} (${src})`)
L.push(`config claude-code user hooks: ${cd.claude.userSettings} (${src})`)
L.push(`config codex-cli: ${cd.codex.hooksJson}, ${cd.codex.configToml} (${src})`)
L.push(`config opencode mcp: ${cd.opencode.configJson} (${src})`)
for w in cd.warnings: L.push("warning: " + w)
L.push(claudeMcpLine(i.claudeMcp))      // n/a | wired (<file>) | MISSING projects["<key>"].mcpServers.unimatrix in <file>; run 'npx unimatrix init' | foreign entry at ... ; not managed by Unimatrix | unreadable (<file> is not valid JSON)
if i.opencodeConfig.status === "unreadable": L.push(`opencode mcp: ${file} is not valid JSON; OpenCode is left without Unimatrix wiring until it is fixed`)
for s in staleLegacy: L.push(s.unreadable ? `stale: ${s.file} unreadable (not valid JSON)` : `stale: ${s.file} ${s.key} holds an owned Unimatrix entry the harness no longer reads; re-run 'npx unimatrix init' to remove it`)
for a in alternateProfiles: L.push(a.unreadable ? `other profile: ${a.file} unreadable (not valid JSON)` : `other profile: ${a.file} ${a.key} (${a.var} points elsewhere; separate ${a.harness} config profile, init leaves it in place)`)
```
MISSING with `key` from `readClaudeMcpState` (computed even when file absent).

## Key tests
- SR-09 parity: `collect(ctx).configDirs` paths equal init manifest paths for each override.
- Claude MISSING after removing `projects[root]`; wired; foreign; unreadable; n/a outside project.
- Stale legacy listed under override, absent when same-file.
- AP-3 seeded owned entry under `CLAUDE_CONFIG_DIR` → `alternateProfiles` entry; status exit 0.
- No writes: run status on fixtures, snapshot mtimes unchanged.
- Grep test (FR-01) passes for status.js.
