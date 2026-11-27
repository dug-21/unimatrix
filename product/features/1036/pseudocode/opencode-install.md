# C10 — OpenCode (`lib/opencode-install.js`)

## Purpose
Write the resolved user `opencode.json` (C1); D-015 prior-opt-in; legacy `~/.config/opencode` removal (R10); project removal R11 + G3 R12 in one mutate; rethrow `SafeWriteError`.

## Removed
`atomicWrite` import, `atomicWriteJson`, `isWithinProject` usage on writes (export kept only if other code/tests import it — grep; otherwise delete), the `try/catch → "skipped"` around the write.

## Functions

```
function computeUserMcp(raw, o) -> {text, detail}
  read = parseJsonRaw(raw)        // {present, malformed, parsed}; same rules as readJsonSafe
  if read.malformed: return {text:null, detail:{skip:{action:"skipped-malformed", reason:"opencode.json is not valid JSON — preserved unchanged",
                              warning:"unimatrix: "+o.file+" is not valid JSON; OpenCode left without Unimatrix wiring until it is fixed"}}}   // D-024 stderr warning
  cfg = read.parsed||{}
  if cfg.mcp !== undefined and !isPlainObject(cfg.mcp): skip malformed "`mcp` key is not an object — preserved unchanged" (+warning)
  existing = cfg.mcp?.unimatrix
  if existing !== undefined and !isUnimatrixMcpEntry("unimatrix", existing): skip skipped-foreign (existing reason + warning)
  if existing === undefined and !o.harnessSelected and !o.priorOptIn:
     skip {action:"skipped-intent", reason:"new mcp.unimatrix entry requires explicit --harness opencode opt-in"}
  entry = buildOpencodeEntry(o)
  if existing !== undefined and deepEqual(existing, entry): return {text:null, detail:{entry, entryAction:"unchanged"}}
  cfg.mcp = cfg.mcp||{}; cfg.mcp.unimatrix = entry
  return {text: JSON.stringify(cfg,null, read.present? detectIndent(raw):2)+"\n", detail:{entry, entryAction: existing!==undefined?"updated":"created"}}

/** W5. o: {file, legacyFile?, platform, home, node, harnessSelected, dryRun} */
function writeOpencodeMcp(o) -> WireLeg
  o.priorOptIn = o.legacyFile ? ownedAt(o.legacyFile) : false          // D-015 / FR-13 (owned entry at R itself is covered by `existing`)
  res = updateFileSync(o.file, raw => computeUserMcp(raw,o).text, {dryRun:o.dryRun, newMode:0o600, platform:o.platform})
  d = computeUserMcp(res.before, o).detail
  if d.skip: return opencodeLeg(d.skip.action, o.file, {reason:d.skip.reason, warning:d.skip.warning})
  return opencodeLeg(d.entryAction, o.file, {entry:d.entry})

function ownedAt(file) -> boolean     // read-only, never throws
  try j = JSON.parse(fs.readFileSync(file,"utf8")) catch: return false
  return isPlainObject(j?.mcp) and isUnimatrixMcpEntry("unimatrix", j.mcp.unimatrix)
```
Back-compat: if `o.file` absent, compute `join(o.home,".config","opencode","opencode.json")` — NO: FR-01 forbids that join outside C1. Callers must pass `file`; update opencode-install.test.js to pass `resolveConfigDirs(...).opencode.configJson`.

```
function computeLegacyRemoval(raw, file) -> {text, detail}
  read = parseJsonRaw(raw)
  if !read.present: return {text:null, detail:{}}
  if read.malformed: return {text:null, detail:{skip:{action:"skipped-malformed", reason:"opencode.json is not valid JSON — preserved unchanged", warning:"unimatrix: "+file+" is not valid JSON; owned entry (if any) not removed"}}}  // FR-14, exit 0
  mcp = read.parsed?.mcp
  if !isPlainObject(mcp) or mcp.unimatrix === undefined: none
  if !isUnimatrixMcpEntry(...): {text:null, detail:{warning:"unimatrix: "+file+' has a non-Unimatrix mcp entry named "unimatrix"; left unchanged'}}
  delete mcp.unimatrix; return {text: JSON.stringify(read.parsed,null,detectIndent(raw))+"\n", detail:{changed:true}}

/** R10 step body */
function removeOwnedOpencodeUserMcpLeg(file, dryRun) -> WireLeg[]
  res = updateFileSync(file, raw=>computeLegacyRemoval(raw,file).text, {dryRun})
  d = computeLegacyRemoval(res.before,file).detail
  if d.skip: return [opencodeLeg("skipped-malformed", file, d.skip)]
  if d.changed: return [opencodeLeg("removed", file, {reason:"owned MCP entry removed (legacy location)"})]
  return Object.assign([], {warnings: d.warning ? [d.warning] : []})

function computeProjectRemoval(raw, file) -> {text, detail:{mcpRemoved, removedPlugin, warnings}}
  read = parseJsonRaw(raw); if !read.present: none
  if read.malformed: throw Error(file+" is not valid JSON; fix it manually (not modified)")     // D-029 exit 1
  p = read.parsed; changed=false; warnings=[]
  // R11
  if isPlainObject(p.mcp) and p.mcp.unimatrix !== undefined:
     if isUnimatrixMcpEntry("unimatrix", p.mcp.unimatrix): delete p.mcp.unimatrix; mcpRemoved=changed=true
     else warnings.push('unimatrix: '+file+' has a non-Unimatrix mcp entry named "unimatrix"; left unchanged')
  // R12 (G3)
  if Array.isArray(p.plugin):
     kept = p.plugin.filter(el => !isLegacyOpencodePlugin(el))
     if kept.length !== p.plugin.length: p.plugin = kept; removedPlugin=changed=true   // [] kept as []
  return {text: changed ? JSON.stringify(p,null,detectIndent(raw))+"\n" : null, detail:{mcpRemoved, removedPlugin, warnings}}

function removeOwnedOpencodeProjectMcp(root, dryRun) -> {actions, warnings, changed, removedPlugin}   // §6 signature
  file = join(root,"opencode.json")
  res = updateFileSync(file, raw=>computeProjectRemoval(raw,file).text, {dryRun, containWithin:root})
  d = computeProjectRemoval(res.before,file).detail
  actions = []; if d.mcpRemoved: actions.push(pre(dryRun,"Removed Unimatrix MCP entry from opencode.json"))
  if d.removedPlugin: actions.push(pre(dryRun,"Removed legacy Unimatrix plugin entry from opencode.json"))
  return {actions, warnings:d.warnings, changed: !!d.mcpRemoved, removedPlugin: !!d.removedPlugin}

/** R11+R12 step body */
function removeOwnedOpencodeProjectLegs(root, dryRun) -> WireLeg[]
  r = removeOwnedOpencodeProjectMcp(root, dryRun); f = join(root,"opencode.json"); legs=[]
  if r.changed: legs.push({harness:"opencode", surface:"mcp", action:"removed", path:f, reason:"owned project MCP entry removed"})
  if r.removedPlugin: legs.push({harness:"opencode", surface:"mcp", action:"removed", path:f, reason:"legacy Unimatrix plugin entry removed"})
  return Object.assign(legs, {warnings:r.warnings})
```

`readJsonSafe` kept for read-only users (status); add `parseJsonRaw(raw)` (pure) and make `readJsonSafe` = read + `parseJsonRaw`.

## Error handling
User/legacy malformed → skip + warning, exit 0. Project malformed → throw, exit 1. `SafeWriteError` → propagate (the old catch-all is gone). In wire.js the old `dispatchOpencode` try/catch is deleted (C6), so non-writer errors from `writeOpencodeMcp` now propagate: compute never throws for user files, so only writer errors escape.

## Key tests
- D-015: owned entry only at `~/.config/opencode` + `XDG_CONFIG_HOME` set, no `--harness` → W5 `created` at XDG file, R10 `removed`.
- No opt-in anywhere → `skipped-intent`, R10 `skipped-dest-unwired`.
- JSONC resolved file → skipped-malformed + stderr warning, exit 0, bytes unchanged.
- G3: `plugin: ["x", "@dug-21/unimatrix-opencode-plugin@1.2.3", "y"]` → `["x","y"]`; single entry → `[]`.
- Same file (`OPENCODE_CONFIG_DIR=<home>/.config/opencode`) → no R10 step.
