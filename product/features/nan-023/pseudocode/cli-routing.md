# Component: CLI Routing — `bin/unimatrix.js` (modify)

> ADR-005 (`wire` verb, `--harness` routing, #960 intent), ADR-004 (`--force` gates `installSkills` only, never forwarded to `wire`). Small argv growth stays in `bin/unimatrix.js`. Ambiguous/unknown invocation → help, never a silent default install (AC-16).

## Purpose

Route the new `wire` verb and the new flags (`--harness`, `--force`) alongside the existing `init` / `mcp-bridge` / binary-passthrough. Thread `--force` into `init` (skills only), route `wire` to the wiring layer with NO skills/DB, and surface help on ambiguity.

## Current routing (unchanged behavior for these)

- `args[0] === "init"` → `init({dryRun, projectDir, remote, token, bundle, slug})`.
- `args[0] === "mcp-bridge"` → JS bridge.
- else → resolve binary + `execFileSync` passthrough.

## New / changed routing

```
function main():
  args = process.argv.slice(2)
  valueAfter(flag) = args[idx+1] if args.indexOf(flag) >= 0 and idx+1 < len else undefined

  # --- init (extend: thread --force) ---
  if args[0] === "init":
    opts = {
      dryRun:     args.includes("--dry-run"),
      force:      args.includes("--force"),          # NEW — skills only (ADR-004); flows to installSkills, NEVER to wire
      projectDir: valueAfter("--project-dir"),
      remote:     valueAfter("--remote"),
      token:      valueAfter("--token"),
      bundle:     valueAfter("--bundle"),
      slug:       valueAfter("--slug"),
    }
    init(opts).then(ok).catch(fail)                  # unchanged promise handling
    return

  # --- wire (NEW verb — ADR-005) ---
  if args[0] === "wire":
    return routeWire(args, valueAfter)

  # --- mcp-bridge / passthrough: unchanged ---
  ...
```

### `routeWire(args, valueAfter)`

```
function routeWire(args, valueAfter):
  dryRun     = args.includes("--dry-run")
  projectDir = valueAfter("--project-dir")
  harness    = valueAfter("--harness")

  # AC-16: validate --harness value up front. Unknown value → help, write nothing.
  if harness !== undefined AND harness NOT in {"claude-code","opencode","codex-cli"}:
    printWireUsage("unknown --harness: " + harness)
    process.exitCode = 2
    return

  # C-12/SR-04: --force is NOT accepted by wire (definitions-only). Passing it is a
  # no-op with a usage note — guaranteeing --force never re-asserts wiring.
  if args.includes("--force"):
    process.stderr.write("note: --force has no effect on `wire` (it is definitions-only; run `init --force` to refresh skills)\n")
    # do NOT forward force into wire()

  # Resolve project root + client path + transport source, then call the wire layer.
  runWire({ dryRun, projectDir, harness })
    .then(() => { process.exitCode = 0 })
    .catch((error) => {
      # wire legs never throw; a throw here is the reused claude loud-checkpoint
      # (malformed .mcp.json/settings.json) or root-resolution failure — surface it.
      process.stderr.write("unimatrix wire failed: " + error.message + "\n")
      process.exitCode = 1
    })
```

### `runWire` — resolve inputs and dispatch (in `init.js` or a thin `wire` entry)

Recommend a small `runWire(opts)` in `init.js` (NOT in `bin`) so `bin` stays a router. It mirrors `init`'s resolution but SKIPS skills + DB (AC-03):

```
async function runWire(opts):
  projectRoot = opts.projectDir ? resolve(opts.projectDir) : detectProjectRoot(process.cwd())
  clientPath  = resolveClientPath()                 # require.resolve("./hook-client/index.js") w/ path fallback (mirror init.js:457)

  # Transport source: local binary if resolvable, else remote/cloud config.
  # `wire` typically runs in an already-inited repo; choose transport the same way init does.
  transportOpts = resolveWireTransport(opts)        # -> { binaryPath } OR { mcp:{url,token} }

  result = wire(projectRoot, Object.assign({ harness: opts.harness, clientPath, dryRun: opts.dryRun }, transportOpts))
  printSummary(result.actions, opts.dryRun)         # reuse init's printer; manifest drives the lines
  # NO installSkills, NO execFileSync DB/validate (ADR-005 §1).
  return result
```

> **Flag for Stage 3b:** `resolveWireTransport` must decide local-vs-cloud for a standalone `wire`. Options: (a) resolve the local binary via `resolveBinary()` and fall back to cloud when unavailable; (b) read the existing `.mcp.json`/credstore to detect the deployment already inited. Recommend (a) for parity with `init`'s local path and defer cloud-`wire` transport to the same resolution `init` uses; document the chosen rule so dry-run and real paths agree (NFR-10).

## `init` internal change — call `wire()` + pass `force` to `installSkills` only

Inside `init()` (local path) and `initRemote()`:

```
# Step 5 (skills): install-if-absent + optional force (ADR-004).
actions.push(...installSkills(projectRoot, { force: opts.force || false, dryRun }))

# Step 5b (wiring): call the wire layer. opencode/codex legs run here now (not inline).
# NOTE: maybeProvisionOpenCode is subsumed by wire()'s opencode dispatch — either call
# wire() which internally invokes maybeProvisionOpenCode + writeOpencodeMcp, OR keep the
# existing maybeProvisionOpenCode call AND add wire() for the new surfaces. RECOMMEND:
# route ALL wiring (claude reuse + opencode + codex) through wire() so init and the wire
# verb share one path (ADR-005 §1 "init and wire share one wiring layer").
wireResult = await wire(projectRoot, { clientPath, binaryPath, dryRun })   # local
actions.push(...wireResult.actions)
# --force is NEVER passed to wire (C-12).
```

> **Flag / refactor caution (SR-07, R-07):** claude-code `.mcp.json` + `.claude/settings.json` are currently written by `init` directly (Steps 3–4). When routing through `wire()`, the claude writers (`writeMcpJson`/`mergeSettings`) must be REUSED unchanged and produce byte-identical output. Capture the golden files BEFORE this refactor (see c14-verifier.md §Golden). Do not let `init` write claude config twice (once inline, once via wire) — move Steps 3–4 into `wire()`'s claude-code dispatch OR keep them in `init` and have `wire()` skip claude when called from `init`. RECOMMEND: `wire()` owns claude-code dispatch; `init` drops its inline Steps 3–4 and delegates. Assert byte-identical common-path output.

## Help / usage (AC-16, SR-05)

```
function printWireUsage(msg?):
  if msg: stderr(msg)
  stdout:
    "usage: unimatrix wire [--harness <claude-code|opencode|codex-cli>] [--dry-run]"
    "  Ensures MCP + hooks + retrieval for detected harnesses. Writes no definition files."
    "  --harness <name>   target one harness; also the opt-in to write a NEW entry into a user-owned config"
    "  --dry-run          print intended actions, write nothing"
    "  (definition scope is skills only; protocols/agents are not installed — run `init --force` to refresh skills)"
```

Emit help (not a write) for: unknown `--harness` value, conflicting flags, or any ambiguous invocation (AC-16). Never fall through to a default install.

## Error handling

- Unknown `--harness` → help + exit 2 (usage error), no writes.
- `--force` on `wire` → usage note, ignored (never forwarded — C-12, SR-04).
- Root-resolution failure (`detectProjectRoot` throws "not a git repo") → caught, stderr, exit 1 (same as init today).
- Wire legs never throw; the only throws in the `wire` path are the reused claude loud-checkpoints (malformed claude config) → caught, exit 1 (parity with `init`, SR-07).

## Key test scenarios (hints)

- `wire` (multi-harness fixture) → wiring surfaces change, zero skill/protocol/agent writes, no DB exec (AC-03).
- `wire --harness codex-cli` → only codex targeted; opt-in enables the new `[mcp_servers.unimatrix]` entry (AC-11).
- `wire --harness foo` → help, exit 2, nothing written (AC-16).
- `wire --force` → usage note, `--force` ignored, wiring unchanged from a plain `wire` (C-12, SR-04).
- `init --force` → `installSkills` overwrites skills; `wire()` runs always-additive; zero wiring changes attributable to `--force` (AC-02).
- `wire --dry-run` / `init --dry-run` → `[dry-run]` lines, zero filesystem changes, action set == real path (AC-14).
- Byte-identical claude-code `.mcp.json` + `.claude/settings.json` after routing through `wire()` (golden, SR-07/R-07).
