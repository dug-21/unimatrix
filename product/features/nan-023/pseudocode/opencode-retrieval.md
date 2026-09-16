# Component: opencode Retrieval Writer — `lib/opencode-install.js` (`writeOpencodeMcp`, new fn, ~+70 lines)

> ADR-001 (per-harness writer returns a WireLeg), vnc-049 sentinel (C-05, SR-06, R-08). Additive `mcp.unimatrix` in `opencode.json`; preserves the Ollama `provider` block + all foreign keys byte-for-byte. Reuses the module's existing helpers — do NOT reinvent `readJsonSafe`/`detectIndent`/`isWithinProject`/`writeJson`.

## Purpose

Close the opencode retrieval gap: today `opencode-install.js` provisions only the plugin (`.opencode/`) and the `plugin[]` array. This adds the `mcp.unimatrix` retrieval entry to `opencode.json` so a `context_*` call resolves against the opencode slug (AC-05/AC-10). Strictly additive: it touches ONLY `config.mcp.unimatrix` and never reads/rewrites the Ollama `provider` block, existing `mcp.*` foreign servers, permissions, or any non-Unimatrix key.

## Public signature (Integration Surface — exact)

```
writeOpencodeMcp(dir, { binaryPath?, url? }, dryRun) -> WireLeg
```

> **Transport reconciliation (see OVERVIEW open-question).** Per Q1/ADR-006 the cloud path must be token-free. Prefer accepting a resolved `transport` descriptor from the orchestrator: `writeOpencodeMcp(dir, { transport }, dryRun)` where `transport` is `{kind:"stdio-binary", binaryPath}` (local) or `{kind:"stdio-bridge", bridgePath, projectHash}` (cloud). The bare `url?` field is retained for signature-compat but the cloud entry is emitted as the token-free stdio-bridge command, NOT `url=` carrying a token. Pick one and keep the emitted bytes = the Q1-decided bytes.

## opencode.json `mcp.unimatrix` entry shape

opencode's MCP schema (local stdio server) — mirror the `.mcp.json` shape opencode expects. The entry OBJECT written (also returned as `WireLeg.entry`):

```
# LOCAL (stdio-binary):
mcp.unimatrix = {
  type: "local",
  command: [ "<absolute binaryPath>" ],     # opencode uses an argv array for local MCP
  enabled: true
}

# CLOUD (stdio-bridge, token-free — Q1):
mcp.unimatrix = {
  type: "local",
  command: [ "node", "<absolute mcp-bridge.js>", "<projectHash>" ],
  enabled: true
}
```

> **Flag for Stage 3b:** confirm opencode's exact `mcp` server schema (`type`/`command` array vs `command`+`args`) against the opencode config the repo already ships (`packages/unimatrix` may carry a reference `opencode.json`, or vnc-049 artifacts). Emit exactly what opencode reads so retrieval RETURNS (AC-10) — do not invent keys. The verifier connects using `WireLeg.entry`, so the entry must be spawnable as written.

## Algorithm

```
function writeOpencodeMcp(dir, opts, dryRun):
  cfgPath = dir/"opencode.json"
  surface = "retrieval"; harness = "opencode"

  # Containment guard (AC-12) — never follow a path escaping root.
  if not isWithinProject(dir, cfgPath):
    return leg("skipped", cfgPath, reason="path escapes project root")

  read = readJsonSafe(cfgPath)                 # never throws; distinguishes absent/malformed
  if read.malformed:
    return leg("skipped-malformed", cfgPath, reason="opencode.json is not valid JSON — preserved unchanged")

  config = read.parsed || {}
  desiredEntry = buildOpencodeEntry(opts)      # the exact object above

  # Ensure the mcp container without clobbering foreign servers.
  if config.mcp is not a plain object:
    if config.mcp !== undefined:               # present but wrong type → do not clobber a user value
      return leg("skipped-malformed", cfgPath, reason="`mcp` key is not an object — preserved unchanged")
    config.mcp = {}

  existing = config.mcp.unimatrix
  if deepEqual(existing, desiredEntry):
    return leg("unchanged", cfgPath, entry=desiredEntry)   # idempotent — no write (AC-04)

  action = existing === undefined ? "created" : "updated"
  config.mcp.unimatrix = desiredEntry          # touch ONLY this key; every foreign key/block untouched in the object graph

  if dryRun:
    return leg(action, cfgPath, entry=desiredEntry)        # WireLeg carries action; no write
  else:
    indent = read.present ? detectIndent(read.raw) : 2     # preserve the file's own indentation (SR-06)
    writeJson(cfgPath, config, indent)                     # re-serialize; only mcp.unimatrix differs
    return leg(action, cfgPath, entry=desiredEntry)

function leg(action, path, {entry?, reason?}):
  return { harness:"opencode", surface:"retrieval", action, path, entry, reason }
```

## Byte-for-byte preservation reasoning (SR-06, R-08, NFR-02)

- The Ollama `provider` block, foreign `mcp.*` servers, permissions, and all non-Unimatrix keys are preserved because the function mutates ONLY `config.mcp.unimatrix` in the parsed object and re-serializes with the file's detected indent. `JSON.parse`→mutate-one-key→`JSON.stringify` preserves all other key/values; ordering of object keys is insertion order (existing keys keep their position; `unimatrix` appends within `mcp`). 
- **Caveat vs. the codex surgical writer:** JSON re-serialization does NOT preserve comments or exotic whitespace — but `opencode.json` is JSON (no comments), and the vnc-049 sentinel is a *value* sentinel (the provider block + mcp entry survive as parsed values), asserted byte-for-byte on the foreign regions via the test. This is the same posture `provisionPluginArray` already uses successfully (`writeJson(cfgPath, config, detectIndent(read.raw))`). Reuse that exact idiom.
- Idempotence: `deepEqual(existing, desiredEntry)` short-circuits with `unchanged` and no write, guaranteeing byte-identical run-2 output (AC-04).

## Intent gate interaction (AC-11)

The orchestrator gates this writer: a NEW `mcp.unimatrix` (absent today) requires `--harness opencode`; an existing one is additive and proceeds. `writeOpencodeMcp` itself is intent-agnostic — it is only CALLED when the gate permits (the gate uses `readJsonSafe` + `hasKeyPath(parsed, ["mcp","unimatrix"])`). Keep the gate in the orchestrator (single site) so this writer stays a pure additive upsert. (If the implementer prefers self-gating, pass `harnessSel` and mirror the codex approach — but do it in exactly one place.)

## Error handling

- Malformed JSON → `skipped-malformed`, file preserved, never throws (AC-15, NFR-03).
- `mcp` present but not an object, or `mcp.unimatrix` present but not an object → treat as a user value; skip rather than clobber (fail-safe).
- fs write error → caught by the orchestrator's per-harness try/catch → converted to a skipped leg (message carries paths only, no token; NFR — no secret written/logged).

## Key test scenarios (hints)

- Fixture `opencode.json` with `mcp.unimatrix` + Ollama `provider` block + foreign keys → run → byte-diff of the provider block + foreign keys = empty; `mcp.unimatrix` present (AC-05, R-08).
- After wiring, `context_*` retrieval against the opencode slug RETURNS (executed from `WireLeg.entry`, not presence — AC-10).
- Fresh `opencode.json` without `mcp.unimatrix` (+ `--harness opencode`) → additive `created`; indent preserved via `detectIndent` (R-08 fresh case).
- Existing `mcp.unimatrix` with a stale command → `updated`, surrounding keys byte-identical.
- Idempotence: run twice → `unchanged` second run, byte-identical file (AC-04).
- Malformed `opencode.json` → `skipped-malformed`, file byte-preserved, no throw (AC-15).
- Cloud transport → entry is the token-free bridge command array, no token in the file or dry-run output (Q1, security).
