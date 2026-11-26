# hook-ownership (`lib/hook-ownership.js`) — new, JS

## Purpose
Single ownership predicate for hook commands and MCP entries (ADR-003, FR-13/14). Consumed by merge-settings, claude-json, codex-install, opencode-install, wire-init, status. Local pattern arrays and `normalizeCommandSource` string arm are deleted elsewhere (D-008). No fs access; pure.

## Exports
```
OWNED_FORMS: {name: string, re: RegExp}[]
KNOWN_EVENTS: string[]     // 9 Claude events (SessionStart Stop UserPromptSubmit PreToolUse PostToolUse PostToolUseFailure PreCompact SubagentStart SubagentStop) U Codex events (CODEX_EVENTS)
isUnimatrixCommand(cmd: string) -> boolean
isUnimatrixMcpEntry(name: string, entry: object) -> boolean
normalizeMcpArgv(entry) -> string[] | null
```
`KNOWN_EVENTS` is defined here (not imported from merge-settings) to avoid a require cycle; merge-settings/codex-install assert in tests that their event lists are subsets.

## Regex building blocks
```
EV      = (?:SessionStart|Stop|...|SubagentStop|<codex events>)     // built from KNOWN_EVENTS
ANYEV   = [A-Z][A-Za-z]+                                           // used ONLY by arms anchored on the distinctive shim path (future events, R-03)
PROV    = (?:\s+--provider(?:\s+|=)[\w-]+)?
LDP     = (?:LD_LIBRARY_PATH=(?:"[^"]*"|'[^']*'|\S+)\s+)?
SEP     = [/\\]
END     = PROV \s* $
```

## OWNED_FORMS (one row per form; each requires `hook\s+EV` or hook-client adjacency)
| name | pattern (anchored ^ ... END) |
|---|---|
| L1-bare | `^LDP unimatrix(?:-server)?\s+hook\s+EV END` |
| L2-abs | `^LDP(?:"[^"]*"\|'[^']*'\|\S*)SEP unimatrix(?:-server)?(?:\.exe)?["']?\s+hook\s+EV END` (basename must be exactly unimatrix[-server]; preceded by a separator) |
| L3-node-client | `(^\|[\s"'/\\])node(?:\.exe)?\s+(?:"[^"]*SEPhook-clientSEPindex\.js"\|'...'\|\S+SEPhook-clientSEPindex\.js)\s+[A-Z]\w*` (existing arm, kept semantics; includes L4 npx-cache absolute paths) |
| L5-npx | `^npx(?:\s+-y)?\s+(?:@dug-21/)?unimatrix(?:@\S+)?\s+hook\s+EV END` |
| L6-home-shim | `^["']?(?:\$HOME\|\$\{HOME\}\|~)SEP\.unimatrixSEPbinSEPunimatrix(?:\.cmd)?["']?\s+hook\s+ANYEV END` |
| abs-shim | `^(["'])[^"']*?SEP\.unimatrixSEPbinSEPunimatrix(?:\.cmd)?\1\s+hook\s+ANYEV END` (single/double quoted, either slash, `.cmd`, spaces in path, `--provider`) |

```
function isUnimatrixCommand(cmd):
  if typeof cmd != "string" or cmd == "": return false
  s = cmd.trim()
  return OWNED_FORMS.some(f => f.re.test(s))
```
Negatives that MUST be false: `my-unimatrix-hook.sh X`, `echo unimatrix hook Stop`, `unimatrix-backup hook Stop`, `node other/index.js Stop`, any string containing "unimatrix" without `hook\s+<event>` or `hook-client/index.js` adjacency. New forms = one table row + tests.

## MCP ownership
```
function normalizeMcpArgv(entry):
  if entry is not plain object: return null
  if Array.isArray(entry.command): return entry.command.map(String)        // OpenCode shape
  if typeof entry.command != "string": return null
  args = Array.isArray(entry.args) ? entry.args.map(String) : []
  return [entry.command, ...args]

function isUnimatrixMcpEntry(name, entry):
  if name != "unimatrix": return false
  argv = normalizeMcpArgv(entry); if argv == null or argv.length == 0: return false
  [cmd, ...a] = argv; base = basename(cmd) lowercased, strip .exe
  // owned forms:
  1. base == "node" and a[0] matches SEPmcp-bridge\.js$                      // legacy bridge (node <path>/mcp-bridge.js <hash>)
  2. base == "npx" and a contains ("unimatrix" or "@dug-21/unimatrix", optional @ver) followed by "mcp-bridge"
  3. base in {"unimatrix","unimatrix-server"} and (a.length == 0 or a[0] in {"serve","mcp-stdio"})   // legacy local binary
  4. base == "sh" and a[0]=="-c" and a[1] matches /exec\s+"\$HOME\/\.unimatrix\/bin\/unimatrix"\s+mcp-stdio/
  5. base == "cmd" and joined a matches /\.unimatrix\\bin\\unimatrix\.cmd"\s+mcp-stdio/i        // retained: recognise the rejected primary Windows form so re-init cleans it
  6. a[0] matches SEP\.unimatrixSEPbinSEPshim-entry\.js$ and a[1]=="mcp-stdio" (any command)   // D-047 fallback, owned on every OS
  return any
```
Foreign entry named `unimatrix` (e.g. `command: "my-tool"`) -> false (caller warns, leaves in place). Project-scope callers pass `(name, entry)` for every key of `mcpServers`; only name `unimatrix` can match.

## Error handling
Never throws; non-string / malformed input returns false.

## Key test scenarios
- Positive table: L1..L6, abs shim single/double, `\` and `/`, `.cmd`, space in path, `--provider codex-cli`, each KnownEvent, future event on shim arms.
- Negative table above plus `LD_LIBRARY_PATH=x /bin/echo unimatrix hook Stop`.
- Round trip: `isUnimatrixCommand(buildShimCommand(e,p,platform,home))` for all e, p, platform, home incl. spaces (R-03 property).
- MCP: each owned form true; foreign `unimatrix` false; foreign names with owned argv (name != "unimatrix") false.
