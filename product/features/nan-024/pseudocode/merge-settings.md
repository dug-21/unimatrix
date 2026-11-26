# merge-settings (`lib/merge-settings.js`, new `lib/claude-json.js`, new `lib/mcp-entry.js`) — JS

## Purpose
Claude-side writers: hook entries in `.claude/settings.local.json` (D-026), cross-scope removal of owned hook entries, `buildShimCommand`, per-machine Claude MCP in `~/.claude.json`, project `.mcp.json` owned-entry removal (D-060), and the shared MCP-entry builder. Files are split to stay far under PL-10 (merge-settings.js currently 428 lines).

## lib/merge-settings.js

### Deleted / changed
- Delete `UNIMATRIX_PATTERNS`, the string arm of `normalizeCommandSource` (D-008, FR-09), and `buildHookClientCommand` from the wiring path. `normalizeCommandSource(cs)` returns `cs` (object only; throws TypeError on a string so no legacy string can sneak back).
- `isUnimatrixHook(hookEntry)` stays as a wrapper: `typeof hookEntry?.command == "string" && isUnimatrixCommand(hookEntry.command)` (imports `./hook-ownership.js`).
- Keep exports `HOOK_EVENTS`, `EVENT_MATCHERS`, `PRETOOLUSE_CYCLE_MATCHER`, `subagentStopEnabled`.

### buildShimCommand
```
buildShimCommand(event, provider, platform, home) -> string
  shim = join(home, ".unimatrix", "bin", platform=="win32" ? "unimatrix.cmd" : "unimatrix")
  if platform == "win32":
     p = shim.replace(/\\/g, "/")                              // forward slashes
     if p includes "%" or '"': throw new UnsafeShimPathError(p, ch)
     base = `"${p}" hook ${event}`
  else:
     if shim includes "'": throw new UnsafeShimPathError(shim, "'")
     base = `'${shim}' hook ${event}`
  return provider ? base + " --provider " + provider : base       // Codex passes "codex-cli"
```
`home` parameter is accepted as additive (D-080). `UnsafeShimPathError` imported from `./install-store.js`. Claude never passes a provider. The string depends only on `home` + event + provider (byte-fixed, D-028).

### mergeSettings(filePath, commandSource, options) — now targets settings.local.json
```
filePath = <root>/.claude/settings.local.json
optInFile = filePath                         // subagent_stop key lives in the same file now (was the sibling)
options = {dryRun, crossScopeFiles?: string[]}      // [<root>/.claude/settings.json, ~/.claude/settings.json]
```
Algorithm = existing Steps 1-4 with these changes:
1. Parse failure still throws the "Malformed ..." Error naming the file (abort untouched).
2. Steps 3/3c/3b unchanged (object-identity keep test, managed vs opt-out partition). Ownership via `isUnimatrixHook`.
3. After the destination merge, for each `crossScopeFiles` entry call `removeOwnedHooks(file, {dryRun})`:
```
removeOwnedHooks(file, {dryRun}) -> {actions, changed}
  if !exists(file): return {[], false}
  raw = read; if blank: return nothing
  parsed = JSON.parse(raw)   // failure -> throw Error("Malformed <file>: ..."); init aborts, file untouched (R-08)
  for event in keys(parsed.hooks || {}) with array value: for group of array: group.hooks = group.hooks.filter(h => !isUnimatrixHook(h)); drop emptied groups; delete event key if empty (same idiom as pruneUnimatrixEvent but over ALL event keys, not just HOOK_EVENTS — covers future events)
  if nothing removed: return unchanged (file NOT rewritten: bytes/mtime preserved)
  else write via atomicWriteJson(file, parsed, detectIndent(raw)); actions.push("Removed unimatrix hook from <file>: <event>")
```
`hooks` key left as `{}` when emptied (not deleted) to preserve user structure; foreign entries untouched byte-semantically.
4. Write-if-changed: serialise `JSON.stringify(content, null, detectIndent(raw)) + "\n"`; if equal to existing raw, skip write (idempotent, mtime stable); else tmp+rename (`atomicWriteJson`). `detectIndent` from `opencode-install.js` (kept helper).
5. Returned `actions` as today (`[dry-run] ` prefix in dry-run). Result also exposes `commands: [{event, command}]` for wire manifest fidelity.
Result guarantee: after merge, exactly one owned entry per managed event across all three Claude scopes, located in settings.local.json (AC-13).

## lib/mcp-entry.js (shared by claude-json, codex-install, opencode-install)
```
buildMcpEntry(platform, home, node) -> {command, args, env}
  win32 (D-072, always the D-047 fallback): {command: node, args: [toFwd(home)+"/.unimatrix/bin/shim-entry.js", "mcp-stdio"], env: {}}
  else: {command:"sh", args:["-c", 'exec "$HOME/.unimatrix/bin/unimatrix" mcp-stdio'], env:{}}
buildMcpArgv(platform, home, node) -> [command, ...args]           // OpenCode
isFallbackMcpEntry(entry) -> boolean                                // D-047 shape: args[0] ends .unimatrix/bin/shim-entry.js and args[1]=="mcp-stdio"
```
`node` = the recorded absolute node from `installStore().` / `install.json`. Text is identical for every user on an OS family except the Windows node/home paths (D-040, D-047 exception). `rerecordedNode == true` forces rewrite of the Windows entry in every destination (D-057): because the writer compares desired vs existing by deep-equal, a changed node path naturally rewrites it.

## lib/claude-json.js (per-machine Claude MCP + project `.mcp.json` migration)
```
writeClaudeMcp({home, projectRoot, entry, dryRun}) -> {action: "created"|"updated"|"unchanged"|"skipped-foreign", file, key, warning?}
removeOwnedMcpJson(projectRoot, dryRun) -> {actions, warnings, changed}
class ClaudeJsonError extends Error        // init exit 1
```
```
function writeClaudeMcp(o):
  file = join(o.home, ".claude.json")
  loop up to 3 times (R-37 guard against concurrent Claude Code writes):
    raw = readOrNull(file); st1 = statOrNull(file)
    if raw != null: try parsed = JSON.parse(raw) catch -> throw ClaudeJsonError(`${file} is not valid JSON; fix it manually (not modified)`)   // abort, never overwrite, exit 1
    else parsed = {}
    if !plain(parsed): throw ClaudeJsonError(...)
    key = resolveProjectKey(parsed.projects, o.projectRoot)          // see below
    existing = parsed.projects?.[key]?.mcpServers?.unimatrix
    if existing !== undefined and !isUnimatrixMcpEntry("unimatrix", existing):
        warn: `unimatrix: ${file} key projects["${key}"].mcpServers.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written`  (D-065, init exit unaffected)
        return {action:"skipped-foreign", ...}
    if existing !== undefined and deepEqual(existing, o.entry): return {action:"unchanged"}      // no write, mtime preserved
    if o.dryRun: return {action: existing? "updated":"created"}
    set parsed.projects = parsed.projects||{}; parsed.projects[key] = parsed.projects[key]||{}; ...mcpServers = ...||{}; mcpServers.unimatrix = o.entry   // ONLY this key changes
    out = JSON.stringify(parsed, null, detectIndent(raw)) + "\n"
    st2 = statOrNull(file); if st2 differs from st1 (mtimeMs/size): continue loop (Claude wrote meanwhile; re-read)
    tmp = file + "." + pid + ".tmp"; write tmp (mode 0o600 like the original if existing: copy mode); rename(tmp, file); return created/updated
  after 3 tries: throw ClaudeJsonError("~/.claude.json changed while writing; re-run init")
```
`resolveProjectKey(projects, root)`: if some existing key `k` has `normalizeKey(k) == normalizeKey(root)` (realpath when it exists, strip trailing separator, win32: lowercase drive letter and `\`->`/`) return `k` (reuse Claude's own spelling); else posix: `root` (realpath'd project root from `walkToProjectRoot`); win32: root with forward slashes, drive-letter case as realpath returns. Exact Claude key spelling (symlink vs realpath, `C:\` vs `c:/`) is verified in CI against a key Claude writes itself (R-38; ci-and-docs E2E note).

```
function removeOwnedMcpJson(projectRoot, dryRun):          // project .mcp.json
  file = join(projectRoot, ".mcp.json"); if !exists: return nothing
  raw = read; parsed = JSON.parse(raw) (failure -> throw ClaudeJsonError naming file; untouched)
  servers = parsed.mcpServers; if !plain(servers): return nothing
  owned = keys k where isUnimatrixMcpEntry(k, servers[k])
  foreignNamed = servers.unimatrix exists and not owned -> warnings.push(`unimatrix: ${file} has a non-Unimatrix server named "unimatrix"; left unchanged`)  // AC-16
  if owned empty: return {changed:false}                    // file untouched byte/mtime
  delete owned keys (key order of the rest preserved); keep mcpServers:{} when emptied; file never deleted
  write (unless dry-run) tmp+rename at detectIndent(raw); action "Removed Unimatrix MCP entry from .mcp.json"
```

## Error handling
`UnsafeShimPathError`, `ClaudeJsonError`, malformed settings JSON -> thrown to wire-init, which prints and exits 1; nothing is partially written because every writer is tmp+rename and parse happens before any write. Warnings (D-065, AC-16) go to stderr and do not alter exit code.

## Key test scenarios
- Update (not delete) existing `merge-settings.test.js` cases to the new string forms and `settings.local.json` destination; `.mcp.json` foreign-preservation guard renamed "removal preserving foreign servers" with per-machine fixtures (AC-12).
- Seed owned entries (L1-L6, abs shim) in settings.local.json + settings.json + `~/.claude/settings.json`: after merge exactly one owned per event, in settings.local.json; foreign entries and unrelated keys byte-equal; foreign-only user file not rewritten (mtime).
- `buildShimCommand` byte test across platforms/homes incl. space; refused chars; unchanged across simulated upgrade; round trip with `isUnimatrixCommand`.
- `~/.claude.json`: other projects/keys untouched; foreign entry at key -> unchanged + D-065 warning; malformed -> exit 1, file byte-equal, no tmp left; concurrent writer simulation (50 inits vs rewriter) leaves valid JSON with writer's latest keys; kill mid-write leaves original.
- Project key table {plain, symlink, trailing slash, subdir cwd, nested repo, Windows `C:\` vs `c:/`}.
- `.mcp.json`: owned-only -> keys removed, `{"mcpServers":{}}` valid; owned+foreign -> foreign identical; foreign-only -> bytes/mtime unchanged; foreign `unimatrix` -> warning; dry-run lists each removal.
