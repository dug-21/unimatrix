# Component: hook-client `--provider` hint — `lib/hook-client/index.js` (`parseHookArgs`) + `lib/merge-settings.js` (`buildHookClientCommand` 3rd arg)

> ADR-003 §1 (teach the JS hook client a `--provider <name>` argv hint). Q5 (DECIDED): the codex-cli provider arm ALREADY exists in both `normalize.js:23` (`KNOWN_PROVIDERS`) and `hook.rs:35` — NO normalizer arm to add. The ONLY gap is that `index.js` does not parse `--provider` argv, so it silently infers `claude-code`. **SIZE-GATED: never raise the hook-client size gate (110 KB stripped PRIMARY / 200 KB raw backstop; ~10 KB raw headroom). #5372, lesson #4780.**

## Purpose

Two small, coupled changes:
1. `buildHookClientCommand` gains an optional 3rd `providerHint` arg that appends `" --provider <hint>"` — so the codex hook writer emits `node <clientPath> <EVENT> --provider codex-cli`.
2. `index.js` gains `parseHookArgs(argv)` that extracts the event AND the provider hint from argv, and routes a present hint into the EXISTING `normalize.js` hint path (as opencode already does) instead of the inference path. Absent hint → behavior byte-identical to today (claude-code inference — backward compat, SR-07).

## Change 1 — `buildHookClientCommand(clientPath, event, providerHint?)` in `merge-settings.js` (~+3 lines)

Current (do not break the 2-arg call sites — claude local/remote hooks):

```
function buildHookClientCommand(clientPath, event):
  quoted = /\s/.test(clientPath) ? '"'+clientPath+'"' : clientPath
  return "node " + quoted + " " + event
```

New (additive 3rd arg; 2-arg calls unchanged → byte-identical claude output, SR-07):

```
function buildHookClientCommand(clientPath, event, providerHint):
  quoted = /\s/.test(clientPath) ? '"'+clientPath+'"' : clientPath
  base = "node " + quoted + " " + event
  if providerHint is a non-empty string:
    return base + " --provider " + providerHint
  return base
```

- Only the codex hook writer passes the 3rd arg (`"codex-cli"`). Every existing 2-arg call (claude-code) returns the identical string as today — this is the backward-compat guarantee (NFR-06).
- `providerHint` is emitted verbatim; the codex writer only ever passes the literal `"codex-cli"` (a `KNOWN_PROVIDERS` member), so no escaping is needed here. (Ownership regex pattern 5 already matches this command form → idempotent re-runs.)

## Change 2 — `parseHookArgs(argv)` in `hook-client/index.js` (lean — size-gated)

Signature (Integration Surface — exact):

```
parseHookArgs(argv: string[]) -> { event: string, providerHint: (string | null) }
```

Algorithm (minimal; `argv` is `process.argv`, so real args start at index 2):

```
function parseHookArgs(argv):
  event = argv[2] || ""
  providerHint = null
  for i = 3; i < argv.length; i++:
    a = argv[i]
    if a === "--provider":
      if i+1 < argv.length: providerHint = argv[i+1]
      i++                                   # consume the value
    else if a startsWith "--provider=":
      providerHint = a.slice("--provider=".length)
  return { event, providerHint }
```

Keep it terse (no helper sprawl, no new module) to protect the size gate.

## Wiring `parseHookArgs` into the existing hint path (`main()`)

Today `main()` does (index.js:343-352):

```
rawEvent = process.argv[2] || ""
normalized = normalize.normalizeEventName(rawEvent)      # inference path — always infers claude-code for shared names
input.provider = normalized[1]                           # provider from inference
effectiveEvent = normalized[0] === UNKNOWN ? rawEvent : normalized[0]
```

Change to route a present, KNOWN hint through the hint path; ABSENT or UNKNOWN hint → today's inference (byte-identical, SR-07):

```
{ event: rawEvent, providerHint } = parseHookArgs(process.argv)

if providerHint is non-null AND normalize.KNOWN_PROVIDERS.includes(providerHint):
  # HINT PATH — mirror the opencode hint contract (ADR-003 §1; normalize.js already
  # supports it via normalizeOpencode-style stamping). Canonicalize the event name
  # WITHOUT letting inference overwrite the provider: use the canonical map, keep the hint.
  canonical = normalize.mapToCanonical(rawEvent)          # name canonicalization only (no provider inference)
  input.provider = providerHint                           # STAMP the hint (e.g. "codex-cli"), not the inference
  effectiveEvent = (canonical === normalize.UNKNOWN_EVENT) ? rawEvent : canonical
else:
  # INFERENCE PATH — unchanged from today (backward compat, SR-07). Unknown hint is
  # IGNORED and falls back to inference (fail-open; the client's exit-0 contract).
  normalized = normalize.normalizeEventName(rawEvent)
  input.provider = normalized[1]
  effectiveEvent = (normalized[0] === normalize.UNKNOWN_EVENT) ? rawEvent : normalized[0]
```

Notes:
- `mapToCanonical` already exists in `normalize.js` (exported) and does name-only canonicalization — reuse it; do NOT add a new normalizer function (Q5: no normalizer arm to add; keeps the JS/Rust twins in lockstep via parity corpus #4751).
- An UNKNOWN hint (not in `KNOWN_PROVIDERS`) is ignored → inference. Never throw (exit-0 contract, ADR-003 §1).
- This is the ONLY behavior change in `index.js`. Every existing test that spawns without `--provider` must produce byte-identical behavior (the inference branch is untouched logic).

## Split-brain / parity (R-05, #5737, #4751)

- `KNOWN_PROVIDERS` already contains `codex-cli` in `normalize.js:23` and its Rust twin `hook.rs:35`. This change does NOT edit `normalize.js` or `hook.rs`, so there is no new arm to keep in step — it only wires argv parsing into the EXISTING hint contract. Assert via the parity corpus (#4751) that the existing codex arm round-trips the hint.
- `source_domain` is forced to `claude-code` at ingress (#5737/#5748) — OUT OF SCOPE. AC-09 asserts the ingested `provider` value (`codex-cli`), NOT `source_domain`. A test asserting `source_domain=codex` will false-fail — assert `provider` only.

## Size gate (R-09, C-Modularity) — HARD CONSTRAINT

- Current `hook-client/index.js`: 464 lines / raw ~189,946 B vs 200,000 B backstop → ~10 KB raw headroom; PRIMARY limit is 110 KB stripped.
- `parseHookArgs` + the `main()` branch must be LEAN: no new module, minimal comment prose (trim, don't add paragraphs), reuse `mapToCanonical`/`KNOWN_PROVIDERS`.
- After the change, `test/check-hook-client-size.js` MUST pass: stripped ≤ 110 KB (budget against the STRIPPED total, not just raw headroom) AND raw ≤ 200 KB. If cap constants ever change, the `size-gate` meta-assertion moves in lockstep (#5378). NEVER minify, NEVER raise the gate (#4780).

## Error handling

- `parseHookArgs` never throws (pure argv scan; missing value → hint stays null).
- Unknown/malformed hint → inference fallback (fail-open, exit 0).
- No stdout writes added (only `transform.js` writes stdout — C-05 preserved).

## Key test scenarios (hints)

- `parseHookArgs(["node","idx.js","PreToolUse","--provider","codex-cli"])` → `{event:"PreToolUse", providerHint:"codex-cli"}`; `--provider=codex-cli` form → same.
- No `--provider` → `providerHint:null`; `main()` behavior byte-identical to today (backward compat, SR-07).
- Spawn with `PreToolUse --provider codex-cli` + synthetic stdin → ingested event carries `provider="codex-cli"` (NOT `claude-code`); assert on `provider`, never `source_domain` (R-05).
- Unknown hint `--provider bogus` → ignored, inference stamps `claude-code`, exit 0 (fail-open).
- `buildHookClientCommand(p, "Stop", "codex-cli")` → `node <p> Stop --provider codex-cli`; `buildHookClientCommand(p, "Stop")` → `node <p> Stop` (2-arg unchanged).
- `check-hook-client-size.js` passes both limits after the change (R-09).
