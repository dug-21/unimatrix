# opencode-install (`lib/opencode-install.js`) — JS

## Purpose
ADR-006: `init` writes no OpenCode provisioning; `wire --harness opencode` writes `mcp.unimatrix` to `~/.config/opencode/opencode.json` (per-machine) and removes an owned `mcp.unimatrix` from the project `opencode.json` (D-064). The file shrinks (608 lines).

## Deleted
`provisionPluginDir`, `provisionPluginArray`, `provisionOpenCode`, `maybeProvisionOpenCode`, `appendPluginEntry`, `mergePackageDep`, `resolvePluginVersion`, `PLUGIN_PACKAGE`, `PLUGIN_EXPORT`, `PLUGIN_ENTRY_FILE`, `PLUGIN_SHIM_CONTENT`, and `buildOpencodeEntry`'s binary/bridge variants plus `resolveOpencodeTransport`. `init.js` stops re-exporting `maybeProvisionOpenCode`; `wire.js` `dispatchOpencode` drops the plugin leg. `opencode-plugin/lib/emit.js` is NOT touched.
## Kept (used by codex-install and others)
`detectsOpenCode`, `isWithinProject`, `detectIndent`, `readJsonSafe`, `writeJson`, `isPlainObject`, `deepEqual`, `opencodeLeg`.

## New / changed functions
```
buildOpencodeEntry({platform, home, node}) -> {type:"local", command: buildMcpArgv(platform, home, node), enabled: true}
writeOpencodeMcp({home, platform, node, harnessSelected, dryRun}) -> WireLeg   // target: join(home,".config","opencode","opencode.json")
removeOwnedOpencodeProjectMcp(projectRoot, dryRun) -> {actions, warnings, changed}
```
No `environment.LD_LIBRARY_PATH` any more (the launcher sets it when spawning the Rust server). Unix argv `["sh","-c","exec \"$HOME/.unimatrix/bin/unimatrix\" mcp-stdio"]`; Windows (D-072) `["<recorded node>","<abs home>/.unimatrix/bin/shim-entry.js","mcp-stdio"]`.

### writeOpencodeMcp
```
cfgPath = join(home,".config","opencode","opencode.json")
read = readJsonSafe(cfgPath)                         // malformed -> skipped-malformed leg, file preserved (fail-safe)
cfg = read.parsed || {}
existing = cfg.mcp?.unimatrix
if existing !== undefined:
   if !isUnimatrixMcpEntry("unimatrix", existing):   // normalizeMcpArgv handles array command
       warning `unimatrix: ${cfgPath} key mcp.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written` (D-065); return leg "skipped-foreign"
   // owned -> update in place, no opt-in needed
elif !harnessSelected: return leg "skipped-intent"      // AC-11 retained: new entry only via --harness opencode
entry = buildOpencodeEntry(...)
if deepEqual(existing, entry): return "unchanged" (no write)
set cfg.mcp = cfg.mcp||{}; cfg.mcp.unimatrix = entry    // only this key; `plugin[]`, provider, permission untouched
write unless dryRun: mkdirp(dirname, recursive), tmp+rename at detectIndent
```
### removeOwnedOpencodeProjectMcp
Project `<root>/opencode.json` `mcp.unimatrix`: owned (`isUnimatrixMcpEntry`) -> delete that key only, keep other `mcp` entries, `plugin[]` untouched; foreign named unimatrix -> warning, untouched; nothing owned -> file not rewritten; malformed -> throw Error naming file (init exit 1, untouched) consistent with claude-json. Existing `plugin[]` entries are never modified (AC-09).

## wire.js interplay
OpenCode leg executes only when `detectsOpenCode(root)` (project `.opencode/` or `opencode.json`) or `--harness opencode`; `init` passes no `harness`, so with no existing owned user entry it yields `skipped-intent` and creates nothing. Project-file removal runs in wire-init regardless of harness selection.

## Error handling
Fail-safe legs; no throws except malformed project `opencode.json` in removal (documented abort). Containment guard (`isWithinProject`) for project paths. Silent-outside-project behaviour is provided by `mcp-stdio` (D-043), not by this writer.

## Key test scenarios
- `init` with `.opencode/` + `opencode.json` fixture: before/after diff shows no new files, `plugin[]` byte-equal (AC-09, R-14).
- `wire --harness opencode`: `mcp.unimatrix` in `~/.config/opencode/opencode.json` equals the argv form; project `opencode.json` not written; foreign `mcp.*`, `provider` block, indentation preserved; foreign `mcp.unimatrix` untouched + warning; owned project entry removed with other entries preserved.
- Windows re-record of node rewrites entry; `opencode-install.test.js` cases for provisioning removed are replaced by "no provisioning" assertions (count preserved by updating, not deleting).
