# codex-install (`lib/codex-install.js`) — JS

## Purpose
Codex writer: hooks to `$CODEX_HOME/hooks.json` (default `~/.codex`) only (D-026, D-081), MCP to `$CODEX_HOME/config.toml` `[mcp_servers.unimatrix]` only (D-060/D-062, D-081), removal of owned entries from project `.codex/hooks.json`, `$CODEX_HOME/config.toml` `[hooks]`, and project `.codex/config.toml` `[mcp_servers.unimatrix]`. Below, `<codexHome>` means `codexHome(env, home)`. Surgical TOML helpers (Wave A exports) are retained and extended.

## Codex home and detection (D-081)
```
codexHome(env, home) -> {dir: abs, warning?: string}
  c = env.CODEX_HOME
  if nz(c):
     if isAbsolute(c) and not containsNul(c): return {dir: c}                      // honoured; existence NOT required here (detection decides)
     return {dir: join(home, ".codex"), warning: "unimatrix: CODEX_HOME is not an absolute path; ignoring it and using ~/.codex"}   // D-085: relative/invalid -> ignore, use ~/.codex, warn; exists-but-not-dir counts as absent
  return {dir: join(home, ".codex")}                                                // no XDG (D-081)

detectsCodex({root, env, home, harnessSel, pathLookup?}) -> boolean
  harnessSel == "codex-cli"                                  // 4. explicit flag
  or isDir(join(root, ".codex"))                             // 1. project .codex/
  or isDir(codexHome(env, home).dir)                         // 2. an EXISTING $CODEX_HOME dir (default ~/.codex counts only if it exists)
  or onPath("codex", env.PATH, platform)                     // 3. `codex` (win32: codex.exe/.cmd/.bat) found in a PATH entry; fs.stat only, no subprocess
```
Any one trigger suffices; none -> Codex is not written to at all (no hooks, no MCP) and a skipped leg is reported. Replaces the old `.codex/`-only detection in wire-init.
Decided (D-084, D-085):
 1. Relative or invalid `$CODEX_HOME` is ignored, `~/.codex` used, warning printed (stderr + leg reason; exit code unchanged).
 2. The Codex MCP write follows the SAME detection rule as hooks (written to `<codexHome>/config.toml` whenever Codex is detected). This replaces the earlier `--harness codex-cli` opt-in gate for creating a new MCP entry: detection already requires a Codex signal, so the gate would only recreate the D-044 gap for MCP. Foreign `unimatrix` key rule (D-065) is unchanged.
 3. `$CODEX_HOME` that exists but is not a directory: treated as not existing for trigger 2 and as the write target only if another trigger fired (write then fails fail-safe as a skipped-malformed leg).

## Changed / new functions
```
codexHome(env, home), detectsCodex(...)                                  // NEW, above
writeCodexHooks({home, env, platform, dryRun}) -> WireLeg[]                  // one leg per event, command = buildShimCommand(event, "codex-cli", platform, home)
removeOwnedCodexHooks({home, env, projectRoot, dryRun}) -> {actions, legs}   // project .codex/hooks.json + ~/.codex/config.toml [hooks]
writeCodexMcpToml({home, env, platform, node, dryRun}) -> WireLeg      // user scope; no harnessSel gate (D-084)
removeOwnedCodexProjectMcp(projectRoot, dryRun) -> {actions, warnings}  // project .codex/config.toml
removeTomlTable(raw, tablePath) -> {text, changed, malformed}           // NEW surgical helper (Wave A style, pure)
tomlTableToEntry(bodyLines) -> {command, args} | null                   // parse command="..." / args=[...] string values only
maybeWireCodex(opts) -> WireLeg[]                                       // opts = {home, env, platform, node, projectRoot, harnessSel, dryRun}; if !detectsCodex(...) -> one skipped-not-detected leg; else orchestrates the above; fail-safe
codexApprovalStep() -> string                                           // constant text, replaces codexTrustNote; shared by init output and status
```
`codexHome` resolution: see above (D-081). The codexHome warning, if any, is added to every leg's `reason` once and printed once.

### writeCodexHooks
```
hooksPath = join(codexHome(env, home).dir, "hooks.json")
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
- `<codexHome>/config.toml` `[hooks]`: using `scanTables`, for every table whose path starts with `hooks` and that contains a `command = "<string>"` line whose decoded value satisfies `isUnimatrixCommand`, remove that table region (`ownedRegion`) with `removeTomlTable`; comments/other tables/CRLF preserved; nothing owned -> file not rewritten; malformed (unterminated header/multiline string) -> skipped-malformed leg, untouched. Empty parent `[[hooks.X]]` group tables left by removal are removed only if they have no remaining children/keys (otherwise left).

### MCP (user scope)
```
cfg = join(codexHome(env, home).dir, "config.toml"); raw = readTextSafe(cfg)
existing = readTomlTable(raw, OWNED_TABLE)            // malformed -> skipped-malformed leg
entry = buildMcpEntry(platform, home, node)           // {command,args,env}; env omitted from TOML
if existing.present:
   cur = tomlTableToEntry(existing.bodyLines)
   if cur == null or !isUnimatrixMcpEntry("unimatrix", cur):
       warning (stderr, also in leg.reason): `unimatrix: ${cfg} key mcp_servers.unimatrix holds a non-Unimatrix entry; left unchanged, Unimatrix MCP not written`  (D-065); return leg action "skipped-foreign"
// no existing entry: create (Codex was detected by maybeWireCodex; D-084: no separate opt-in gate; owned entries only, listed in dry-run)
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
- Detection (D-081): each of {project `.codex/`, existing `$CODEX_HOME` dir, fake `codex` on PATH, `--harness codex-cli`} alone writes `<codexHome>/hooks.json` AND the MCP table; none of them -> nothing written under any Codex path; custom absolute `$CODEX_HOME` honoured (files land there, `~/.codex` untouched); relative `$CODEX_HOME` -> `~/.codex` used + warning; no XDG path consulted.
- Hooks only in `<codexHome>/hooks.json`; seed all three scopes + foreign: exactly one owned per event, all in user file; foreign byte-equal; dry-run lists each user-level edit (AC-13).
- Byte-identical `~/.codex/hooks.json` across init, simulated upgrade, re-init from another cwd (D-028); init output contains the approval step; no trust hash written.
- MCP: new entry written when Codex is detected (D-084); owned existing updates in place (Windows node re-record rewrites); foreign `unimatrix` table untouched + warning; TOML comments/CRLF/foreign tables preserved; project `.codex/config.toml` owned removed, foreign kept, only-foreign not rewritten.
- Existing `codex-install.test.js`, `codex-toml-surgical.test.js` updated to new destinations/strings, count not reduced.
