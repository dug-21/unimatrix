# C1 — Config-dir resolver (`lib/config-dirs.js`, new)

## Purpose
Only builder of harness config paths (FR-01, ADR-002 #5960). Never throws, never reads `process.env`, never mutates `env`.

## Exports
`resolveConfigDirs`, `sameRealFile`, `codexHome`, `realPathForm`, `VARS`.

## Functions

```
VARS = ["CLAUDE_CONFIG_DIR","OPENCODE_CONFIG_DIR","XDG_CONFIG_HOME","CODEX_HOME"]

function pathMod(platform): platform === "win32" ? path.win32 : path.posix
  // use pathMod for isAbsolute/join so injected platform is honoured

function readVar(env, name, platform, fallbackLabel, warnings) -> string|null
  v = env ? env[name] : undefined
  if typeof v !== "string" or v === "": return null            // FR-06 unset/empty: no warning
  if v.includes("\0") or v.trim() === "" or !pathMod(platform).isAbsolute(v):
     warnings.push(`unimatrix: ${name} is not an absolute path (${JSON.stringify(v)}); ignoring it and using ${fallbackLabel}`)
     return null
  return stripTrailingSep(v)   // keep root "/" or "C:\" intact
  // NOTE: "~/x" is not absolute -> warning (FR-06)

function codexHome(env, home, platform?) -> {dir, warning?}
  // kept signature; D-085 warning text unchanged
  w = []
  d = readVar(env, "CODEX_HOME", platform||process.platform, "~/.codex", w)
  // FR-05: keep today's exact D-085 text, not the generic FR-06 text (Q4)
  if d: return {dir: d}
  if w.length: return {dir: join(home,".codex"), warning: CODEX_LEGACY_WARNING}
  return {dir: join(home,".codex")}

function resolveConfigDirs({env, home, platform}) -> ConfigDirs
  warnings = []
  P = pathMod(platform)
  // Claude
  cd = readVar(env, "CLAUDE_CONFIG_DIR", platform, "~/.claude.json and ~/.claude/settings.json", warnings)
  claude = cd
    ? {dir: cd, claudeJson: P.join(cd,".claude.json"), userSettings: P.join(cd,"settings.json"), source:"CLAUDE_CONFIG_DIR", value: cd}
    : {dir: home, claudeJson: P.join(home,".claude.json"), userSettings: P.join(home,".claude","settings.json"), source:"default", value:null}
  // OpenCode (measured precedence)
  od = readVar(env, "OPENCODE_CONFIG_DIR", platform, nextOpencodeLabel(env), warnings)
  xd = readVar(env, "XDG_CONFIG_HOME", platform, "~/.config/opencode", warnings)
  if od:      opencode = {dir: od, source:"OPENCODE_CONFIG_DIR", value: od}
  else if xd: opencode = {dir: P.join(xd,"opencode"), source:"XDG_CONFIG_HOME", value: xd}
  else:       opencode = {dir: P.join(home,".config","opencode"), source:"default", value:null}
  opencode.configJson = P.join(opencode.dir, "opencode.json")
  // Codex
  ch = codexHome(env, home, platform); if ch.warning: warnings.push(ch.warning)
  cv = ch.warning ? null : (nonEmpty(env.CODEX_HOME) ? stripTrailingSep(env.CODEX_HOME) : null)
  codex = {dir: ch.dir, hooksJson: P.join(ch.dir,"hooks.json"), configToml: P.join(ch.dir,"config.toml"),
           source: cv ? "CODEX_HOME" : "default", value: cv}
  legacy = {userSettings: P.join(home,".claude","settings.json"),
            opencodeJson: P.join(home,".config","opencode","opencode.json")}
  alternates = []
  if claude.source !== "default":
     alternates.push({harness:"claude-code", file:P.join(home,".claude.json"), key:"projects[<root>].mcpServers.unimatrix", var:"CLAUDE_CONFIG_DIR"})
     alternates.push({harness:"claude-code", file:P.join(home,".claude.json"), key:"mcpServers.unimatrix", var:"CLAUDE_CONFIG_DIR"})
  if od and xd:
     alternates.push({harness:"opencode", file:P.join(xd,"opencode","opencode.json"), key:"mcp.unimatrix", var:"OPENCODE_CONFIG_DIR"})
  if codex.source === "CODEX_HOME":
     for f in ["hooks.json","config.toml"]: alternates.push({harness:"codex-cli", file:P.join(home,".codex",f), key: f==="hooks.json"?"hooks":"mcp_servers.unimatrix", var:"CODEX_HOME"})
  // alternates are candidates; callers (status) drop those sameRealFile with the destination
  return {claude, opencode, codex, legacy, alternates, warnings}
```

`nextOpencodeLabel(env)`: if XDG valid → `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode` (FR-06 "next step").

```
function realPathForm(p) -> string
  // realpath of deepest existing ancestor + remaining components (FR-12, FR-22)
  rest = []; cur = path.resolve(p)
  loop:
    try return join(fs.realpathSync.native(cur), ...rest.reverse())
    catch ENOENT/ENOTDIR: rest.push(basename(cur)); parent = dirname(cur)
          if parent === cur: return path.resolve(p); cur = parent
    catch other: return path.resolve(p)

function sameRealFile(a, b, platform?) -> boolean     // D-007 / FR-12
  sa = statOrNull(a); sb = statOrNull(b)   // fs.statSync, follows links
  if sa and sb: return sa.dev === sb.dev and sa.ino === sb.ino
  return norm(realPathForm(a), platform) === norm(realPathForm(b), platform)
  // norm on win32: "\" -> "/", lowercase drive letter, case-insensitive compare (lowercase whole)
```

## Error handling
None thrown. Bad values -> `warnings[]`. stat/realpath errors fall back as above.

## Key tests
- Each var × {relative, `""`, `"  "`, `~/x`, NUL}: same dirs as unset; warning count (0 for `""`, 1 otherwise) and text.
- `OPENCODE_CONFIG_DIR` beats `XDG_CONFIG_HOME`; XDG beats default; alternates contains XDG file only when both valid.
- Trailing slash stripped; `CLAUDE_CONFIG_DIR=<home>` gives claudeJson == default path (and status/plan treat it same-file).
- `sameRealFile`: both exist via symlink → true; only one exists, ancestor symlinked → true; win32 drive-letter case (injected platform).
- `env` object frozen in test → no throw (never mutated).
- `codexHome` legacy warning string byte-identical to today.
