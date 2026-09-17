# Component: Codex Installer — `lib/codex-install.js` (new, < 450 lines)

> ADR-001 (writers return WireLegs), ADR-002 (surgical TOML — see toml-surgical.md), ADR-003 (hooks target the JS hook client, `--provider codex-cli` mandatory + emitted event set). NOT under the hook-client size gate. Fail-safe (warn-and-skip, never throw). Mirrors `opencode-install.js` structure/error posture.

## Purpose

Bring codex-cli to full C14 parity: write `[mcp_servers.unimatrix]` into `.codex/config.toml` (surgical, foreign-preserving) and Claude-like Codex hooks into `.codex/hooks.json` targeting the JS hook client with `--provider codex-cli` on every command.

## Public signatures (Integration Surface — exact)

```
maybeWireCodex(dir, { clientPath, binaryPath?, url?, dryRun }) -> WireLeg[]
writeCodexMcpToml(dir, { binaryPath?, url? }, dryRun) -> WireLeg
writeCodexHooks(dir, { clientPath, dryRun }) -> WireLeg
# internal (see toml-surgical.md):
upsertTomlTable(raw, tablePath, body) -> { text, changed }
readTomlTable(raw, tablePath) -> { present, body }
```

> **Transport reconciliation (OVERVIEW open-question).** Prefer threading a `transport` descriptor from the orchestrator in place of `url?`: `writeCodexMcpToml(dir, { transport }, dryRun)` where `transport` is `{kind:"stdio-binary", binaryPath}` (local) or `{kind:"stdio-bridge", bridgePath, projectHash}` (cloud). Per Q1/ADR-006 the cloud form is the token-free bridge (`command="node", args=[bridgePath, projectHash]`), NEVER `url=` (a bearer token in TOML violates Principle 8). Keep the emitted bytes = the Q1-decided bytes.

## Module constants / reuse

```
OWNED_TABLE      = "mcp_servers.unimatrix"
HOOKS_FILE       = ".codex/hooks.json"
CONFIG_FILE      = ".codex/config.toml"
CODEX_PROVIDER   = "codex-cli"                  # mandatory hint (C-04, NFR-07)

# reuse (do not reinvent):
isWithinProject, readJsonSafe, detectIndent    from ./opencode-install.js
buildHookClientCommand                          from ./merge-settings.js   (3rd arg = providerHint — hook-client-provider.md)
EVENT_MATCHERS, PRETOOLUSE_CYCLE_MATCHER        from ./merge-settings.js   (matcher shapes)
```

## Emitted Codex hook event set (ADR-003 §3, Q4 — 7 events)

| Event | Matcher |
|-------|---------|
| `SessionStart` | (none / "") |
| `UserPromptSubmit` | (none / "") |
| `PreToolUse` | `^context_cycle$\|^mcp__unimatrix__context_cycle$` (= `PRETOOLUSE_CYCLE_MATCHER`) |
| `PostToolUse` | `*` |
| `PreCompact` | (none / "") |
| `SubagentStart` | `*` |
| `Stop` | (none / "") |

```
CODEX_EVENTS = ["SessionStart","UserPromptSubmit","PreToolUse","PostToolUse","PreCompact","SubagentStart","Stop"]
# Excludes PostToolUseFailure and SubagentStop (ADR-003 §3). Matchers reused from EVENT_MATCHERS
# so codex and claude stay in step; PreToolUse uses PRETOOLUSE_CYCLE_MATCHER.
```

## `maybeWireCodex(dir, opts)` — top-level fan-out

```
function maybeWireCodex(dir, opts):
  legs = []
  # Detection is done by the orchestrator (detectHarnesses); this is only called
  # when .codex/ is present. Still guard defensively.
  try:
    legs.push( writeCodexMcpToml(dir, { transport: opts.transport, harnessSel: opts.harnessSel }, opts.dryRun) )
    legs.push( writeCodexHooks(dir, { clientPath: opts.clientPath, dryRun: opts.dryRun }) )
  catch e:
    # Fail-safe: never throw out of the wire layer (AC-15). Convert to a skipped leg.
    legs.push({ harness:"codex-cli", surface:"mcp", action:"skipped-malformed",
                path: dir/CONFIG_FILE, reason: "codex wiring error: " + e.message })
  return legs
```

> **Intent gate site (decide once — see wire-orchestrator.md).** The codex MCP entry is a NEW entry into a user-owned config → requires `--harness codex-cli` when absent (AC-11). Either the orchestrator gates before calling `writeCodexMcpToml`, OR `writeCodexMcpToml` self-gates using `harnessSel` + `readTomlTable(...).present`. Pick ONE site; this pseudocode shows self-gating inside `writeCodexMcpToml` for cohesion. Do not gate in both.

## Trust precondition (C-08, NFR-09, ADR-006 §3)

`.codex/config.toml` + hooks only load for a **trusted** `.codex/` layer. Wiring cannot make the layer trusted (that is a Codex/consumer action), so surface it, never silently no-op:

```
function codexTrustNote(dir):
  # Emit a precondition WARNING line (added to WireLeg.reason or an actions note) whenever
  # we write codex config, stating the leg is conditional on a trusted .codex/ layer.
  # This is a SURFACED precondition (fail-loud/warn), not a gate — we still write the config.
  return "codex wiring is active only in a trusted .codex/ layer (mark .codex/ trusted in Codex)"
```

Attach `codexTrustNote(dir)` to the actions summary for codex legs so an untrusted consumer is told (never a silent inert pass). The verifier asserts firing in a trusted fixture (Gate-0).

## `writeCodexMcpToml(dir, opts, dryRun)` — surgical TOML MCP

```
function writeCodexMcpToml(dir, opts, dryRun):
  cfgPath = dir/CONFIG_FILE
  H = "codex-cli"; S = "mcp"

  if not isWithinProject(dir, cfgPath):
    return leg(H,S,"skipped", cfgPath, reason="path escapes project root")     # AC-12

  raw = readTextSafe(cfgPath)                 # missing/unreadable file → "" (fail-safe)
                                              # (readTextSafe: try fs.readFileSync; catch → "")

  # Intent gate (self-gating variant): is this a NEW entry?
  existing = readTomlTable(raw, OWNED_TABLE)
  if existing.malformed:
    return leg(H,S,"skipped-malformed", cfgPath, reason="config.toml has an ambiguous table boundary — preserved unchanged")
  if not existing.present AND opts.harnessSel !== "codex-cli":
    return leg(H,S,"skipped-intent", cfgPath,
               reason="new codex entry needs opt-in: run `unimatrix wire --harness codex-cli`")   # AC-11

  bodyLines = buildTomlBody(opts.transport)   # see below; TOML-escaped values
  result = upsertTomlTable(raw, OWNED_TABLE, bodyLines)
  if result.malformed:
    return leg(H,S,"skipped-malformed", cfgPath, reason="config.toml has an ambiguous table boundary — preserved unchanged")

  entryObj = tomlBodyAsObject(bodyLines)      # structured mirror for WireLeg.entry (verifier connects with this)

  if not result.changed:
    return leg(H,S,"unchanged", cfgPath, entry=entryObj)     # idempotent — no write (AC-04)

  action = existing.present ? "updated" : "created"
  if dryRun:
    return leg(H,S,action, cfgPath, entry=entryObj)          # WireLeg only; no write
  else:
    mkdirp(dirname(cfgPath))                                 # .codex/ exists (detected) but be safe
    fs.writeFileSync(cfgPath, result.text, "utf8")
    return leg(H,S,action, cfgPath, entry=entryObj)
```

### `buildTomlBody(transport)` — the owned table's key lines (TOML-escaped, R-10)

```
function buildTomlBody(transport):
  switch transport.kind:
    case "stdio-binary":
      return [ 'command = ' + tomlString(transport.binaryPath) ]
    case "stdio-bridge":                     # cloud, token-free (Q1)
      return [ 'command = "node"',
               'args = [' + tomlString(transport.bridgePath) + ', ' + tomlString(transport.projectHash) + ']' ]
    default:                                 # transport.kind === "none"
      # No transport source. Do NOT emit a broken command. Surface as skipped.
      throw new SkipSignal("no transport (binaryPath/mcp absent)")   # caught → skipped leg

function tomlString(s):
  # Emit a TOML BASIC string with proper escaping (R-10 command-injection guard).
  # Escape backslash, double-quote, and control chars per TOML spec; wrap in double quotes.
  return '"' + s.replace(\\ -> \\\\).replace(" -> \\").replace(controlChars -> \uXXXX) + '"'
```

`tomlString` is the security-critical escaper: a `binaryPath`/`bridgePath` containing spaces, quotes, or backslashes must produce a valid TOML string that re-parses to the exact intended value (R-10). Never naive-concatenate a raw path into `command =`.

## `writeCodexHooks(dir, opts)` — JSON hooks targeting the JS hook client

Same matcher-group shape as `.claude/settings.json`. Fail-safe JSON merge (reuse `readJsonSafe`). Non-clobbering: preserves foreign hook entries; idempotent via the `UNIMATRIX_PATTERNS` ownership form (pattern 5 already matches `node …/hook-client/index.js <EVENT> …`).

```
function writeCodexHooks(dir, opts):
  hooksPath = dir/HOOKS_FILE
  H = "codex-cli"; S = "hooks"

  if not isWithinProject(dir, hooksPath):
    return leg(H,S,"skipped", hooksPath, reason="path escapes project root")    # AC-12

  read = readJsonSafe(hooksPath)              # never throws; absent → present:false
  if read.malformed:
    return leg(H,S,"skipped-malformed", hooksPath, reason=".codex/hooks.json is not valid JSON — preserved unchanged")
  content = read.parsed || {}
  if content.hooks is not a plain object:
    if content.hooks !== undefined:
      return leg(H,S,"skipped-malformed", hooksPath, reason="`hooks` key is not an object — preserved unchanged")
    content.hooks = {}

  changed = false
  commands = []                               # collect exact command strings for the WireLeg(s)

  for event in CODEX_EVENTS:
    matcher = EVENT_MATCHERS[event]           # "" | "*" | PRETOOLUSE_CYCLE_MATCHER
    command = buildHookClientCommand(opts.clientPath, event, CODEX_PROVIDER)   # 3rd arg — appends " --provider codex-cli"
    # C-04 fail-loud invariant: assert the flag is present on the built command.
    if not command.includes("--provider " + CODEX_PROVIDER):
      throw new Error("codex hook command missing --provider codex-cli (fail-loud, NFR-07)")
    commands.push({ event, command })
    changed = upsertHookEntry(content.hooks, event, matcher, command) OR changed
      # upsertHookEntry mirrors mergeSettings' matcher-group merge, scoped by isUnimatrixHook:
      #  - find the matcher group for `matcher`; within it find a Unimatrix-owned hook (isUnimatrixHook)
      #  - if present and command equal → no change; if present and different → update; if absent → append
      #  - dedup duplicate uni hooks (reverse splice), like mergeSettings Step 3
      #  - foreign hook entries and foreign matcher groups are never touched

  action = read.present ? "updated" : "created"
  # Determine "unchanged": if no entry changed AND file already existed with all 7 commands equal.
  if not changed AND read.present:
    action = "unchanged"

  if opts.dryRun:
    return hooksLeg(H, hooksPath, action, commands)          # no write
  else if action === "unchanged":
    return hooksLeg(H, hooksPath, action, commands)          # idempotent — skip write (AC-04)
  else:
    mkdirp(dirname(hooksPath))
    fs.writeFileSync(hooksPath, JSON.stringify(content, null, detectIndent(read.raw || "")) + "\n", "utf8")
    return hooksLeg(H, hooksPath, action, commands)
```

### Hooks WireLeg emission (multi-event — see wire-orchestrator.md flag)

The verifier fires the EXACT command per event (non-tautology, R-02). Emit **one hooks `WireLeg` per event** so each carries its own `command`:

```
function hooksLeg(H, path, action, commands):
  # RETURN one WireLeg per command so the manifest carries every exact command string.
  # maybeWireCodex flattens these into its WireLeg[] result.
  return commands.map(({event, command}) =>
    ({ harness:H, surface:"hooks", action, path, command, reason: undefined }))
```

> `writeCodexHooks`' declared return type is a single `WireLeg`; to carry all 7 commands, either (a) return `WireLeg[]` (recommend — update the signature note) OR (b) return one leg whose `command` is a representative + attach the full set on an auxiliary field the verifier reads. Recommend (a): `writeCodexHooks -> WireLeg[]`, and `maybeWireCodex` concatenates. This keeps the manifest the sole execution source and matches the claude-hooks per-event decision in the orchestrator. FLAG this signature widening for Stage 3b.

## `.codex/hooks.json` shape written (matches AC-07/AC-08 assertions)

```json
{ "hooks": {
  "PreToolUse": [ { "matcher": "^context_cycle$|^mcp__unimatrix__context_cycle$",
    "hooks": [ { "type": "command",
      "command": "node <abs>/lib/hook-client/index.js PreToolUse --provider codex-cli" } ] } ],
  "PostToolUse": [ { "matcher": "*", "hooks": [ { "type":"command",
      "command": "node <abs>/lib/hook-client/index.js PostToolUse --provider codex-cli" } ] } ],
  "SessionStart": [ { "matcher": "", "hooks": [ { "type":"command",
      "command": "node <abs>/lib/hook-client/index.js SessionStart --provider codex-cli" } ] } ],
  ...UserPromptSubmit, PreCompact, SubagentStart(*), Stop...
} }
```

- Command targets `node <clientPath>` — the JS hook client — NEVER the `unimatrix` binary (C-03, AC-08).
- `--provider codex-cli` present on EVERY command (C-04, AC-07, NFR-07); a missing flag is a fail-loud defect (the `throw` above).

## Error handling (fail-safe — mirrors opencode-install.js)

- Missing `.codex/config.toml` → treated as empty `""`; owned table appended (subject to intent gate).
- Malformed TOML boundary or malformed hooks JSON → `skipped-malformed`, file byte-preserved, no throw (AC-15, NFR-03).
- Path escaping root → `skipped`, never followed (AC-12).
- Any unexpected throw inside a writer is caught in `maybeWireCodex` and converted to a skipped leg — the wire layer never throws (AC-15). Contrast: init's claude checkpoints stay loud (SR-07) — codex is always warn-and-skip.
- Trust precondition surfaced as a note (never a silent no-op, NFR-09).
- No secret written: cloud uses the token-free bridge; no token appears in `.codex/config.toml`, hooks, or dry-run output (Q1, security).

## Key test scenarios (hints)

- Foreign `[mcp_servers.*]` tables + comments + non-alpha order → owned table added, foreign byte-preserved (AC-06, R-04).
- Hooks: parse `.codex/hooks.json` → every command targets `hook-client/index.js`, NOT the binary (AC-08); `--provider codex-cli` on all 7 (AC-07); foreign hook entries preserved.
- Missing flag path: force `buildHookClientCommand` without the hint → fail-loud throw asserted (NFR-07, R-05).
- New codex entry without `--harness codex-cli` → `skipped-intent` + help line; with it → `created` (AC-11).
- Idempotence: run twice → TOML `unchanged` + hooks `unchanged`, byte-identical files (AC-04).
- Path with spaces/quotes → TOML `command`/`args` re-parse to intended argv (R-10).
- Malformed config.toml / hooks.json → `skipped-malformed`, preserved, no throw (AC-15).
- Cloud transport → `command="node", args=[bridge, hash]`, no `url=`, no token (Q1).
- Behavioral (verifier): in a trusted `.codex/`, execute each manifest `command` → event reaches the JS hook client stamped `provider=codex-cli` (AC-09); connect via `WireLeg.entry` → `context_*` RETURNS (AC-10). Per-event firing RECORDED for all 7 (Q4).
