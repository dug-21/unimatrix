# C9 — Codex (`lib/codex-install.js`)

## Purpose
All writes through `updateFileSync`; `codexHome` delegated to C1 (re-exported, same signature); `maybeWireCodex` rethrows `SafeWriteError` (FR-31). Behaviour/legs otherwise unchanged (W3, W4, R7, R8, R9; gating "none").

## Changes

```
const { codexHome } = require("./config-dirs.js")     // delete local codexHome; keep it in module.exports
const { updateFileSync, SafeWriteError } = require("./safe-write.js")
// delete: const { atomicWriteText } = require("./claude-json.js")
```

Optional `o.dirs` (ConfigDirs) accepted by `maybeWireCodex`; when given, `hooksPath = dirs.codex.hooksJson`, `cfg = dirs.codex.configToml`; else computed from `codexHome(env, home).dir` (back-compat for direct tests).

### writeCodexHooks (W3)
```
compute(raw):
  read = parse raw like readJsonSafe (raw null → present:false)
  if malformed or hooks not object: return {text:null, detail:{skip:"<existing reason>"}}
  upsert per CODEX_EVENTS -> changed
  return {text: changed||!present ? JSON.stringify(content,null,detectIndent(raw||""))+"\n" : null, detail:{commands, present, changed}}
res = updateFileSync(hooksPath, raw=>compute(raw).text, {dryRun, newMode:0o600})
d = compute(res.before).detail
if d.skip: return [leg("hooks","skipped-malformed",hooksPath,{reason:d.skip})]
return d.commands.map(c => leg("hooks", res.action, hooksPath, {command:c}))
```
Note: today `present && !changed` → `unchanged`; new path: `res.action` gives same values (`created` / `updated` / `unchanged`).

### writeCodexMcpToml (W4)
```
compute(raw): existing logic over raw ("" when null)
  malformed → skip TOML_MALFORMED_REASON; foreign → skip foreignWarning (action skipped-foreign)
  result = upsertTomlTable(raw, OWNED_TABLE, body); text = result.changed ? result.text : null
res = updateFileSync(cfg, ..., {dryRun, newMode:0o600})
d = compute(res.before).detail; if skip -> leg(skip.action,...); else leg("mcp", res.action, cfg, {entry})
```
`res.action` "created" when file absent, "updated" when present (matches `existing.present`? No — present file without table yields `updated` today only if existing.present; today an existing config.toml lacking the table reports `created`). Preserve today's semantics: action = `d.tablePresent ? "updated" : "created"` when `res.action !== "unchanged"`.

### removeOwnedCodexHooks (R7 user config.toml `[hooks]`; R8 project `.codex/hooks.json`)
- R8: `removeOwnedHooks(projHooks, {dryRun, containWithin: projectRoot})` (C8) replaces `isWithinProject` guard; keep the `path.resolve` same-file check but use `sameRealFile(projHooks, userHooks)`. Malformed → existing skipped-malformed leg (catch only non-`SafeWriteError`; rethrow `SafeWriteError`).
- R7: compute `removeOwnedHooksToml(raw)`; malformed → skipped-malformed leg; `updateFileSync(cfg, ...)`.

### removeOwnedCodexProjectMcp (R9)
`updateFileSync(cfg, compute, {dryRun, containWithin: projectRoot})`; same-file check via `sameRealFile(cfg, userCfg)`; drop `isWithinProject`.

### maybeWireCodex
```
try { ...unchanged sequence... }
catch e:
  if e instanceof SafeWriteError: throw e                       // FR-31
  legs.push(leg("mcp","skipped-malformed", cfg, {reason:"codex wiring error: "+e.message}))
```
`ch.warning` handling unchanged (but wire.js already reports `dirs.warnings` — to avoid a duplicate stderr line, `maybeWireCodex` skips writing `ch.warning` to stderr when `o.dirs` is passed; still appends to leg reasons as today).

## Error handling
Malformed TOML/JSON → skip legs (exit 0). `SafeWriteError` → rethrow → exit 1.

## Key tests
- Existing codex-install / codex-toml-surgical suites green.
- TOML bytes outside owned table identical after W4/R7.
- Symlinked `.codex/config.toml` outside root → `outside-project` exit 1.
- `fault` injected `SafeWriteError` inside writeCodexHooks → propagates out of maybeWireCodex.
- `CODEX_HOME` set: `~/.codex/*` untouched.
