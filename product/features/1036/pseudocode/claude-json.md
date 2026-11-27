# C7 — Claude MCP (`lib/claude-json.js`)

## Purpose
Pure compute over the resolved `.claude.json` (from C1) through `updateFileSync`; W2 + R1 (G2) in one mutate; `.mcp.json` removal (R6); read-only `readClaudeMcpState` for status. Never builds `home` paths. Never touches the default `~/.claude.json` under `CLAUDE_CONFIG_DIR` (D-028) — guaranteed because only `o.file` is used.

## Removed
`atomicWrite` import, `atomicWriteText` export, `statOrNull`, `sameStat`, the 3-attempt loop (now in C3), `o.home`.

## Functions

```
function parseClaudeJson(file, raw) -> object
  if raw === null or raw.trim() === "": return {}
  try j = JSON.parse(raw) catch: throw ClaudeJsonError(file + " is not valid JSON; fix it manually (not modified)")
  if !plain(j): throw ClaudeJsonError(file + " is not a JSON object; fix it manually (not modified)")
  return j

function computeClaudeMcp(raw, o) -> {text, detail:{key, skip?, mcpAction, removedUserScope}}
  parsed = parseClaudeJson(o.file, raw)
  key = resolveProjectKey(parsed.projects, o.projectRoot, o.platform)
  existing = parsed.projects?.[key]?.mcpServers?.unimatrix (plain-guarded)
  if existing !== undefined and !isUnimatrixMcpEntry("unimatrix", existing):
     return {text:null, detail:{key, skip:{action:"skipped-foreign", warning:"unimatrix: "+o.file+' key projects["'+key+'"].mcpServers.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written'}}}
     // R1 NOT applied when W2 is skipped-foreign (gate in §3.3)
  changed = false
  mcpAction = existing === undefined ? "created" : deepEqual(existing, o.entry) ? "unchanged" : "updated"
  if mcpAction !== "unchanged": ensure path; parsed.projects[key].mcpServers.unimatrix = o.entry; changed = true
  top = plain(parsed.mcpServers) ? parsed.mcpServers.unimatrix : undefined
  removedUserScope = false
  if top !== undefined:
     if isUnimatrixMcpEntry("unimatrix", top): delete parsed.mcpServers.unimatrix; removedUserScope = changed = true
        // keep `mcpServers: {}` if it becomes empty (do not delete foreign-structure keys)
     else detail.userScopeForeignWarning = "unimatrix: "+o.file+" key mcpServers.unimatrix holds a non-Unimatrix entry; left unchanged"   // D-065
  text = changed ? JSON.stringify(parsed, null, raw ? detectIndent(raw) : 2) + "\n" : null
  return {text, detail:{key, mcpAction, removedUserScope, userScopeForeignWarning}}

/** W2 + R1 legs. o: {file, projectRoot, entry, platform, dryRun} */
function claudeMcpLegs(o) -> WireLeg[]
  res = updateFileSync(o.file, raw => computeClaudeMcp(raw, o).text, {dryRun:o.dryRun, newMode:0o600, platform:o.platform})
  d = computeClaudeMcp(res.before, o).detail
  if d.skip: return [{harness:"claude-code", surface:"mcp", action:"skipped-foreign", path:o.file, reason:d.skip.warning, warning:d.skip.warning}]
  legs = [{harness:"claude-code", surface:"mcp", action:d.mcpAction, path:o.file, entry:o.entry}]
  if d.removedUserScope: legs.push({harness:"claude-code", surface:"mcp", action:"removed", path:o.file, reason:"owned user-scope MCP entry removed"})
  if d.userScopeForeignWarning: legs[0].warning = d.userScopeForeignWarning
  return legs
```

Leg action for the mcp leg is `d.mcpAction` (entry-level), not `res.action` — a file update caused only by R1 keeps the mcp leg `unchanged`. Same in both modes (parity).

`writeClaudeMcp(o)` (§6 signature) is kept as a thin wrapper returning `{action: d.skip ? "skipped-foreign" : d.mcpAction, file, key, warning?, removedUserScope}` for existing tests; `claudeMcpLegs` uses the same internals.

```
function computeMcpJsonRemoval(raw, file) -> {text, detail:{changed, warnings}}
  if raw === null or raw.trim()==="": return {text:null, detail:{changed:false, warnings:[]}}
  parse or throw ClaudeJsonError(file+" is not valid JSON; fix it manually (not modified)")
  (existing owned/foreign logic) -> delete owned keys
  text = owned.length ? JSON.stringify(parsed,null,detectIndent(raw))+"\n" : null

function removeOwnedMcpJson(projectRoot, dryRun) -> {actions, warnings, changed}   // existing shape kept
  file = join(projectRoot, ".mcp.json")
  res = updateFileSync(file, raw => computeMcpJsonRemoval(raw,file).text, {dryRun, containWithin: projectRoot})
  d = computeMcpJsonRemoval(res.before, file).detail
  return {actions: d.changed ? [pre(dryRun,"Removed Unimatrix MCP entry from .mcp.json")] : [], warnings:d.warnings, changed:d.changed}

function removeOwnedMcpJsonLeg(root, dryRun) -> WireLeg[]
  r = removeOwnedMcpJson(root, dryRun)
  legs = r.changed ? [{harness:"claude-code", surface:"mcp", action:"removed", path:join(root,".mcp.json"), reason:"owned project MCP entry removed"}] : []
  return Object.assign(legs, {warnings: r.warnings})   // warnings without a leg ride on legs.warnings (C4 reads it)
```

Note: `.mcp.json` absent → `updateFileSync` with mutate returning null on `raw===null` → `unchanged`, no file created. Correct.

```
function readClaudeMcpState(file, projectRoot, platform) -> {status, file, key}   // read-only, never throws
  try raw = fs.readFileSync(file,"utf8") catch: return {status:"missing", file, key: resolveProjectKey(undefined, projectRoot, platform)}
  try j = raw.trim()===""? {} : JSON.parse(raw) catch: return {status:"unreadable", file, key:null}
  key = resolveProjectKey(j.projects, projectRoot, platform)
  e = j.projects?.[key]?.mcpServers?.unimatrix
  if e === undefined: return {status:"missing", file, key}
  return {status: isUnimatrixMcpEntry("unimatrix", e) ? "wired" : "foreign", file, key}
```
(`n/a` is decided by status, not here.)

## Error handling
`ClaudeJsonError` from compute propagates out of `updateFileSync` unchanged → exit 1 (FR-07j). File untouched (no temp created before mutate).

## Key tests
- G2: file with owned top-level + no local → one write, legs `[created, removed]`; dry-run identical.
- Foreign top-level `unimatrix` kept, warning.
- Foreign local entry → skipped-foreign, owned top-level stays (R1 not applied).
- Unparseable → ClaudeJsonError, bytes unchanged, no temp.
- Indent tab/4/2 preserved; foreign key order kept.
- `CLAUDE_CONFIG_DIR` set: default `~/.claude.json` bytes+mtime unchanged.
- `readClaudeMcpState`: wired / missing (file absent, projects key absent) / foreign / unreadable.
