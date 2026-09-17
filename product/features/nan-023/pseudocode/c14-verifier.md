# Component: C14 Verifier — `test/` (new)

> ADR-006 (assert retrieval-returns / hook-fires from the WIRED command's manifest, local + cloud, pre-tag real-server exercise). This is the load-bearing non-tautology component (R-01, R-02, SR-09). The verifier EXECUTES `manifest[].command` / connects via `manifest[].entry` — it NEVER string-matches or reconstructs paths.

## Purpose

Prove each C14 leg is demonstrably functional, not merely present: (a) a `context_*` retrieval RETURNS from the wired MCP target, and (b) wired hooks FIRE to the JS hook client. Consumes the `WireLeg[]` manifest produced by `wire()` — the single source of what to execute. Includes negative/mutation controls that FAIL when the wired artifact is broken but its manifest string is intact (defeats R-02).

## Core discipline (non-negotiable — SR-09, R-02, #4177)

1. The verifier's ONLY input for WHAT to execute is the manifest returned by `wire()`. It does not re-derive `.codex/config.toml` paths, hook command strings, or slugs.
2. For hooks: `child = spawn the exact WireLeg.command`, feed a synthetic event on stdin, assert INGRESS at the JS hook client (breadcrumb/queue/server record) — NOT that the command string contains `hook-client`.
3. For retrieval: connect the MCP target described by `WireLeg.entry` (spawn `entry.command`+`entry.args`, or the local-binary command), issue a `context_*` call, assert a NON-EMPTY RETURN — NOT that the config file contains an entry.
4. Every behavioral AC (AC-09, AC-10) has at least one mutation/negative control: break the wired artifact, keep the manifest string intact, assert the verifier FAILS. If it still passes, it is a presence proxy — reject.

## Test layout (extend existing `test/` — cumulative, do not scaffold isolated infra)

```
test/
  c14-verifier.js            # the harness: manifest → execute → assert (shared helpers)
  c14-claude.test.js         # claude-code: retrieval-returns + hook-fires (local; cloud return via bridge)
  c14-opencode.test.js       # opencode: retrieval-returns (plugin-level firing, not JS-client)
  c14-codex.test.js          # codex-cli: retrieval-returns + hook-fires (trusted .codex/, per-event evidence)
  c14-negative.test.js       # mutation/negative controls for AC-09/AC-10 (R-02)
  golden/                    # SR-07 pre-refactor claude-code golden files
    mcp.json.golden
    settings.json.golden
  fixtures/                  # reuse/extend test/fixtures — multi-harness repos, TOML edge corpus, trusted .codex/
  helpers/                   # reuse/extend test/helpers — MCP spawn, hook-event feeder, trust seeding
```

Reuse existing helpers (`test/helpers`, `test/run-hook-client.js`, `test/fixtures`) — extend, never duplicate (test infra is cumulative).

## Harness: `runManifest(manifest, deployment)` — the execution spine

```
function verifyLeg(leg, deployment):     # deployment ∈ {"local","cloud"}
  switch leg.surface:
    case "mcp":
    case "retrieval":
      return assertRetrievalReturns(leg, deployment)
    case "hooks":
      return assertHookFires(leg, deployment)
    case "plugin":
      return assertPluginLevel(leg, deployment)   # opencode observation is plugin-level (AC-10 note)
    default: skip (skipped-* legs are asserted separately — see below)
```

### `assertRetrievalReturns(leg, deployment)` — AC-10 (return path is load-bearing, C-06)

```
function assertRetrievalReturns(leg, deployment):
  assert leg.entry is present                       # a created/updated/unchanged mcp leg carries the exact entry
  target = spawnMcpTargetFromEntry(leg.entry, deployment)
    # local  stdio-binary: spawn entry.command (the absolute binary) as an MCP stdio server
    # cloud  stdio-bridge: spawn "node <bridgePath> <projectHash>" (token-free) — the SAME bytes wire wrote
    # claude-code local/cloud: entry is mcpServers.unimatrix (command/args); spawn it
    # opencode: entry is mcp.unimatrix (command array); spawn it
  response = mcpClient(target).call("context_status" or a read-only context_* tool)   # any returning context_* call
  assert response is a NON-EMPTY RETURN            # presence of the config entry does NOT discharge this (SR-09)
  teardown(target)
```

### `assertHookFires(leg, deployment)` — AC-09 (hook actually fires, C-03/C-04)

```
function assertHookFires(leg, deployment):
  assert leg.command is present                     # the EXACT wired command string
  syntheticEvent = buildSyntheticEventFor(leg)      # JSON on stdin matching the event (e.g. PreToolUse context_cycle)
  child = spawnCommandString(leg.command, stdin=syntheticEvent, env=deploymentEnv(deployment))
    # spawnCommandString executes the manifest command VERBATIM (node <clientPath> <EVENT> --provider codex-cli)
  await child exit                                  # exit code always 0 (client contract) — do NOT assert on exit code
  ingress = observeIngress(deployment)              # breadcrumb file / queue frame / server record — the client actually ran
  assert ingress observed                           # the event REACHED the JS hook client
  assert ingress.provider === "codex-cli"           # (codex) attribution correct — assert PROVIDER, never source_domain (R-05)
```

`observeIngress` reads the JS hook client's real effect (state breadcrumb via `state.writeBreadcrumb`, queued frame, or the server's recorded event over a real transport) — NOT a parse of the config. This is what makes firing behavioral (R-01).

## Per-harness coverage (from the feasibility matrix — ACCEPTANCE-MAP Q3)

| AC | Harness | Local | Cloud |
|----|---------|-------|-------|
| AC-05/AC-10 | opencode | required | required (token-free bridge) |
| AC-08 | codex | required (string) | required (string; deployment-independent) |
| AC-09 | codex | required (fixture seeds trusted `.codex/`) | conditional-on-trust (Gate-0) |
| AC-10 | claude-code | required | required (proven bridge) |
| AC-10 | codex | required | conditional-on-trust (Gate-0; `command="node", args=[bridge,hash]`, never `url=`) |

- **Gate-0 (Q2):** confirmed BEFORE delivery — can cloud CI seed a trusted `.codex/`? Feasible → codex-cloud AC-09/AC-10 HARD; not feasible → documented conditional (local HARD, C14 proven(local)/partial(cloud)) — NEVER silent-skip. Record the outcome in RISK-COVERAGE-REPORT.
- **opencode firing** is plugin-level (in-process TS plugin), asserted at that level — NOT JS-hook-client ingress (AC-10 note).

## AC-09 per-event codex firing evidence (Q4)

For each of the 7 emitted events, execute its manifest `command` with a matching synthetic event and RECORD `fires` vs `wired-inactive`:

```
for leg in manifest.filter(harness=="codex-cli" && surface=="hooks"):
  event = eventOf(leg.command)
  result = assertHookFires(leg, deployment) ? "fires" : "wired-inactive"
  record(event, deployment, result)     # any "wired-inactive" is a NAMED documented gap, never a silent drop
```

Mirroring Claude's event names does NOT prove Codex fires them — the RECORD is the evidence. Feed into the ACCEPTANCE-MAP per-event table during Stage 3c.

## Mutation / negative controls (AC-09 & AC-10 — R-02, defeats tautology)

```
# Retrieval mutation: point the entry at a broken target, keep the manifest shape.
test "AC-10 fails when wired MCP target is broken":
  brokenLeg = clone(mcpLeg); brokenLeg.entry.command = "/nonexistent/binary"
  assert assertRetrievalReturns(brokenLeg, "local") FAILS       # proves we EXECUTE, not string-match

# Hook mutation: point the command at a non-existent client path.
test "AC-09 fails when wired hook client path is broken":
  brokenLeg = clone(hookLeg); brokenLeg.command = brokenLeg.command.replace(clientPath, "/nonexistent/index.js")
  assert assertHookFires(brokenLeg, "local") FAILS              # ingress never observed → fail

# Negative control: blank the wired entry, re-run verifier, assert FAIL.
test "AC-10 fails when the wired entry is deleted":
  delete the owned entry from the config; re-run wire? NO — re-run the verifier against a manifest whose target is gone.
  assert FAIL (not a presence proxy).
```

If any mutation still PASSES, the assertion is a presence/string proxy — reject and rewrite (SR-09 guard).

## Skipped-leg assertions (R-11, SR-08, AC-13/AC-15)

The manifest carries `skipped-*` legs as first-class outcomes. Assert they are DISTINCT and visible:

```
test "skipped-undetected != skipped-malformed != skipped-intent":
  assert each skip reason surfaces in actions output (visible, not swallowed)
  assert skipped-undetected → no writes, exit success (AC-13)
  assert skipped-malformed → input byte-preserved, no throw, no partial write (AC-15)
  assert skipped-intent → no write + help line naming the exact --harness command (AC-11)
```

## Golden files (SR-07, R-07) — capture BEFORE the wire.js refactor

```
# Wave 0 (before routing claude writers through wire.js):
capture golden/mcp.json.golden and golden/settings.json.golden from a fresh-repo `init` fixture.

test "claude-code common path byte-identical after refactor":
  run init through wire() on the same fixture
  assert .mcp.json  == golden/mcp.json.golden      (byte-for-byte)
  assert settings.json == golden/settings.json.golden (byte-for-byte)
```

## opencode sentinel (SR-06, R-08)

```
test "opencode mcp.unimatrix + Ollama provider preserved + retrieval returns":
  fixture opencode.json with mcp.unimatrix + Ollama provider block + foreign keys
  run wire (--harness opencode)
  assert byte-diff of the provider block + foreign keys == empty
  assert assertRetrievalReturns(opencodeRetrievalLeg, "local") RETURNS   # not presence
```

## Pre-tag real-server exercise (SR-03, R-03, ADR-006 §4)

A test/task that runs codex hook-fire + retrieval-return + opencode/claude return against a REAL local+cloud server BEFORE the release chain — surfaces layered failures off-tag (avoids the nan-019/nan-020 one-tag-round-each tax, #5267). Gate this on a real-server env flag; skip-with-reason (visible) when the real server is unavailable, never silently green.

## Command-injection fixture (R-10, security)

```
test "path with spaces/quotes emits correctly-escaped command":
  project root path contains a space and a quote
  run wire
  assert emitted .codex/config.toml command/args re-parse (TOML parser) to the intended argv
  assert emitted hooks command re-parses to the intended argv (node <quoted path> <EVENT> --provider codex-cli)
```

## Size gate (R-09) — separate check, keep green

`test/check-hook-client-size.js` already exists. After `parseHookArgs` lands, assert stripped ≤ 110 KB (PRIMARY) AND raw ≤ 200 KB (BACKSTOP); meta-assertion moves in lockstep if constants change. Never raise the gate.

## Key test scenarios (hints — consolidated)

- Every behavioral leg (claude retrieval+hooks, opencode retrieval, codex retrieval+hooks) asserted from the manifest, local + cloud per the feasibility matrix.
- Mutation control per behavioral AC fails when the artifact is broken but the manifest string is intact (R-02).
- Per-event codex firing RECORDED for all 7 events (Q4).
- Golden claude-code common path byte-identical (SR-07); opencode sentinel byte-preserved + returns (SR-06).
- Skip reasons distinct + visible (SR-08); dry-run action set == real-path (NFR-10); zero writes to global paths (AC-12).
- No token in any written file or dry-run output on the cloud path (Q1, security).
