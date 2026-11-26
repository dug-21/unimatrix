# codex-install (`lib/codex-install.js`) — JS

## Purpose
Codex writer: hooks to `~/.codex/hooks.json` only (D-026), MCP to `~/.codex/config.toml` `[mcp_servers.unimatrix]` only (D-060/D-062), removal of owned entries from project `.codex/hooks.json`, `~/.codex/config.toml` `[hooks]`, and project `.codex/config.toml` `[mcp_servers.unimatrix]`. Surgical TOML helpers (Wave A exports) are retained and extended.

## Changed / new functions
```
writeCodexHooks({home, platform, dryRun}) -> WireLeg[]                  // one leg per event, command = buildShimCommand(event, "codex-cli", platform, home)
removeOwnedCodexHooks({home, projectRoot, dryRun}) -> {actions, legs}   // project .codex/hooks.json + ~/.codex/config.toml [hooks]
writeCodexMcpToml({home, platform, node, harnessSel, dryRun}) -> WireLeg   // user scope
removeOwnedCodexProjectMcp(projectRoot, dryRun) -> {actions, warnings}  // project .codex/config.toml
removeTomlTable(raw, tablePath) -> {text, changed, malformed}           // NEW surgical helper (Wave A style, pure)
tomlTableToEntry(bodyLines) -> {command, args} | null                   // parse command="..." / args=[...] string values only
maybeWireCodex(opts) -> WireLeg[]                                       // orchestrates the above; fail-safe
codexApprovalStep() -> string                                           // constant text, replaces codexTrustNote; shared by init output and status
```
`codexHome = join(home, ".codex")` (see open question on `CODEX_HOME`).

### writeCodexHooks
```
hooksPath = join(home,".codex","hooks.json")
read = readJsonSafe(hooksPath)                       // malformed -> skipped-malformed leg, file untouched (fail-safe, no throw)
content = read.parsed || {}; hooks must be object else skipped-malformed
for event of CODEX_EVENTS:
   cmd = requireProviderFlag(buildShimCommand(event, "codex-cli", platform, home))     // fail-loud invariant kept
   if upsertHookEntry(content.hooks, event, EVENT_MATCHERS[event], cmd): changed = true // owned-scoped upsert (uses isUnimatrixCommand via isUnimatrixHook), dedups extra owned entries in this file
action = present ? (changed ? "updated" : "unchanged") : "created"
if !dryRun and action != "unchanged": mkdirp(dirname); atomic tmp+rename write at detectIndent
return one leg per event {surface:"hooks", action, path:hooksPath, command}
```
Codex events/matchers unchanged (`CODEX_EVENTS`, `EVENT_MATCHERS`). Result string is the same on every call/upgrade (D-028); re-init over current entry writes nothing.

### removeOwnedCodexHooks
- Project `.codex/hooks.json`: same removal idiom as `removeOwnedHooks` in merge-settings (filter `isUnimatrixHook` across ALL event keys; drop emptied groups/events; unchanged -> not rewritten). The project-containment guard (`isWithinProject`) stays.
- `~/.codex/config.toml` `[hooks]`: using `scanTables`, for every table whose path starts with `hooks` and that contains a `command = "<string>"` line whose decoded value satisfies `isUnimatrixCommand`, remove that table region (`ownedRegion`) with `removeTomlTable`; comments/other tables/CRLF preserved; nothing owned -> file not rewritten; malformed (unterminated header/multiline string) -> skipped-malformed leg, untouched. Empty parent `[[hooks.X]]` group tables left by removal are removed only if they have no remaining children/keys (otherwise left).

### MCP (user scope)
```
cfg = join(home,".codex","config.toml"); raw = readTextSafe(cfg)
existing = readTomlTable(raw, OWNED_TABLE)            // malformed -> skipped-malformed leg
entry = buildMcpEntry(platform, home, node)           // {command,args,env}; env omitted from TOML
if existing.present:
   cur = tomlTableToEntry(existing.bodyLines)
   if cur == null or !isUnimatrixMcpEntry("unimatrix", cur):
       warning (stderr, also in leg.reason): `unimatrix: ${cfg} key mcp_servers.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written`  (D-065); return leg action "skipped-foreign"
elif harnessSel != "codex-cli": return skipped-intent leg (existing AC-11 opt-in gate retained: new user-owned entry needs --harness codex-cli)
body = ['command = ' + tomlString(entry.command), 'args = [' + entry.args.map(tomlString).join(", ") + ']']    // every string through tomlString (R-10)
result = upsertTomlTable(raw, OWNED_TABLE, body); !changed -> "unchanged" (no write); else write unless dryRun
```
Cloud and local produce the identical table (AC-01); `buildTomlEmit`/`resolveCodexTransport` bridge/binary variants are deleted.

### removeOwnedCodexProjectMcp
Project `.codex/config.toml`: table `mcp_servers.unimatrix` present and `isUnimatrixMcpEntry("unimatrix", tomlTableToEntry(body))` -> `removeTomlTable`; foreign table named unimatrix -> warning, untouched; no owned -> not rewritten; foreign `[mcp_servers.other]` preserved byte-for-byte; file never deleted.

### removeTomlTable (pure)
Mirror `upsertTomlTable`: locate the owned table via `scanTables`/`findOwned`, delete its lines through `ownedRegion` end (including one trailing blank line if it keeps spacing sane), return `{changed:false}` when absent, `{malformed:true}` for ambiguous files. Never reformat outside the region.

## Error handling
All legs fail-safe: malformed or unexpected throw becomes a `skipped-malformed` leg; nothing partial is written. `requireProviderFlag` throws on a missing `--provider codex-cli` (defect, not degrade). Warnings never change exit code.

## Key test scenarios
- Hooks only in `~/.codex/hooks.json`; seed all three scopes + foreign: exactly one owned per event, all in user file; foreign byte-equal; dry-run lists each user-level edit (AC-13).
- Byte-identical `~/.codex/hooks.json` across init, simulated upgrade, re-init from another cwd (D-028); init output contains the approval step; no trust hash written.
- MCP: new entry needs opt-in; owned existing updates in place (Windows node re-record rewrites); foreign `unimatrix` table untouched + warning; TOML comments/CRLF/foreign tables preserved; project `.codex/config.toml` owned removed, foreign kept, only-foreign not rewritten.
- Existing `codex-install.test.js`, `codex-toml-surgical.test.js` updated to new destinations/strings, count not reduced.
