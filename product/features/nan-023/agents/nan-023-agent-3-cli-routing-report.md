# Agent Report — nan-023 Wave C · cli-routing

- Agent ID: nan-023-agent-3-cli-routing
- Role: uni-js-dev
- Component: CLI routing (`packages/unimatrix/bin/unimatrix.js`)
- Issue: #990 · ADR-005 (#960 intent), ADR-004 (`--force` scope)

## Summary

Routed the new `wire` verb, `--harness <name>`, and `--dry-run` in `bin/unimatrix.js`,
and threaded `--force` into the `init(...)` options object for the skills-installer
handoff. The wire path calls the shared `wire(projectRoot, opts)` layer only — it never
touches `installSkills` or the DB/validate steps (AC-03). `--harness` is validated up
front and carried into `wire()` as the new-entry opt-in signal; the orchestrator enforces
the #960 intent gate (no double-implementation here). `--force` is a no-op usage note on
`wire` and is NEVER forwarded (C-12/SR-04).

Coordinated with the parallel wire-orchestrator agent purely via the pseudocode
`wire(projectRoot, { harness?, clientPath, binaryPath?, mcp?, dryRun }) -> { actions, manifest }`
signature (synchronous, per the Integration Surface). Did NOT edit `lib/wire.js` or
`lib/init.js`.

## Report

### Files modified
- `packages/unimatrix/bin/unimatrix.js` (modified)
  - `init` branch: added `force: args.includes("--force")` to the options object.
  - New `wire` branch + helpers `routeWire`, `resolveWireTransport`, `printWireUsage`,
    `printWireSummary`, and `VALID_HARNESSES`.
- `packages/unimatrix/test/cli-routing.test.js` (new) — 17 unit tests driving the real
  shim entry point with `lib/wire.js` mocked.

### Routing behavior implemented
- `wire` → `wire(root, {harness?, clientPath, dryRun, ...transport})`; prints a
  wire-specific summary from `result.actions`; exit 0 on success.
- Transport (matches init, so dry-run/real agree — NFR-10): prefer local binary via
  `resolveBinary()`; else read the out-of-tree cloud credential keyed by
  `computeProjectHash(root)`; else empty descriptor (wire reports skipped MCP legs, never
  throws).
- Unknown `--harness` value (or a dangling `--harness` that swallows the next flag) →
  `printWireUsage` (usage to stdout, error to stderr), exit 2, `wire()` never called (AC-16).
- Help text states the skills-only definition boundary (C-13).
- `--force` on `wire` → stderr usage note, not forwarded (C-12). `--force` never reaches
  the wire path's opts at all (verified in test).
- `wire()` throw (reused claude loud-checkpoint on malformed `.mcp.json`/`settings.json`) →
  `unimatrix wire failed: <msg>` on stderr, exit 1 — parity with `init` (SR-07).

### Tests: 17 pass / 0 fail (`node --test test/cli-routing.test.js`)
Covers: verb routing + no-init (AC-03), `--harness` forwarding (AC-11), unknown/ambiguous
harness → help/exit 2 (AC-16), skills-only boundary in help (C-13), `--force` not
forwarded + wire still additive (C-12/SR-04/AC-02), `--dry-run` opts parity (AC-14),
local+cloud+empty transport resolution, wire-throw error posture (SR-07), and the
`init --force` → `force:true` handoff.

Regression: existing `test/shim.test.js` (13) + `test/bin-mcp-bridge.test.js` (7) = 20/20
pass; `test/init.test.js` 23/23 pass.

### Gates
- Size gate (`node test/check-hook-client-size.js`): PASS — stripped 105243/110000,
  raw 191416/200000 (bin/ is not under the hook-client gate; unchanged by this work).
- Zero deps (`node test/check-zero-deps.js`): PASS. `package.json` / lockfile unchanged.
- Fail-open: every fs/net-adjacent call in the wire path is wrapped; unknown harness and
  transport-resolution failures degrade to help/skip, never a throw to the host; the only
  surfaced throw is the reused claude loud-checkpoint (caught, exit 1, parity with init).

### Scope adherence
Modified only `bin/unimatrix.js` + added `test/cli-routing.test.js`. Did NOT edit
`lib/wire.js` or `lib/init.js`. Did NOT run or modify integration tests (Stage 3c).
`test/fixtures/wire/` left untouched (parallel agent's).

### Blockers
None. `lib/wire.js` did not exist on disk at implementation time (built by the parallel
Wave C wire-orchestrator agent); tests mock it against the agreed signature. When both
land, the wire path is live end-to-end. Coordination assumption: `wire()` is synchronous
(per the Integration Surface); if the orchestrator makes it async, the `try/catch` around
the call would need to become promise handling — flag for Gate 3b if it diverges.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced ADR-005 (#5767), ADR-001 (#5763),
  and the JS-init/bridge-routing decisions (#1200, #5116); confirmed the wire-verb + intent
  model already read from the ADR files. No new gotchas beyond what the pseudocode carried.
- Stored: entry #5775 "Testing bin/unimatrix.js CLI routing: capture stderr on exit 0"
  via context_store (pattern, topic hook-client) — the execFileSync-drops-stderr trap that
  breaks `--force` no-op-note assertions, and the mock-wire.js routing-test recipe.
