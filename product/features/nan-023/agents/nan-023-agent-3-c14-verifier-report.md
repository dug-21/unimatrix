# Agent Report — nan-023 Wave D: C14 Verifier

Agent: nan-023-agent-3-c14-verifier (role: uni-js-dev)
Component: C14 verifier (`packages/unimatrix/test/`), ADR-006

## Summary

Built the manifest-executing C14 verifier: it EXECUTES the `WireLeg` manifest's
exact `command`/`entry` (retrieval-returns + hook-fires), for claude-code,
opencode, and codex-cli, across local (UDS) and cloud (HTTP/bridge) deployments,
with mutation/negative controls per behavioral AC. Zero new dependencies; no lib
bytes added; hook-client size gate untouched.

## Files Created

- `packages/unimatrix/test/c14-verifier.js` — shared harness (execution spine):
  quote-aware `splitCommand`/`eventOf`, `mcpCallFromEntry` (newline-delimited
  JSON-RPC stdio client using the production framing), `assertRetrievalReturns`,
  `buildSyntheticEventFor`, `runHookCommand`, `observeHookFire`, and the two real
  ingress observers `makeHttpIngress` (cloud) / `makeUdsIngress` (local).
- `packages/unimatrix/test/fixtures/mcp/stdio-mcp-fixture.js` — a real spawnable
  stdio MCP server (production `StdioFramer`, newline-delimited JSON-RPC) that
  answers `context_*` with a NON-EMPTY return. Execution target only; it never
  learns the manifest (non-tautology preserved). Serves both the local
  `command=<binary>` and cloud `command="node", args=[bridge,hash]` shapes.
- `packages/unimatrix/test/c14-claude.test.js` — AC-10 retrieval-returns local +
  cloud (token-free bridge); claude JS-client hook-fires baseline.
- `packages/unimatrix/test/c14-opencode.test.js` — AC-05 retrieval-returns local
  + cloud; vnc-049 sentinel byte-preservation; plugin-level firing note.
- `packages/unimatrix/test/c14-codex.test.js` — AC-10 local + cloud (asserts NO
  url=/token in the cloud entry, Q1); AC-09 hook-fires + provider=codex-cli;
  per-event (7) firing RECORD; documented-conditional self-firing record; R-06
  trust precondition note.
- `packages/unimatrix/test/c14-negative.test.js` — mutation/negative controls for
  AC-10 (claude/codex/opencode broken + blanked entry) and AC-09 (broken client
  path), plus a tautology guard asserting presence passes where execution fails.

No other component's files touched. Golden byte-identical (SR-07) already lives
in `wire.test.js` (Wave C) — not duplicated.

## Tests (verifier self-test)

`node --test` on the four suites: **21/21 pass** (2 consecutive runs, no flake).
- Size gate: `node test/check-hook-client-size.js` → OK. stripped 105243/110000
  (headroom ~4.7 KB), raw 191416/200000 (headroom ~8.6 KB). Unchanged by this
  wave (zero lib bytes added).
- `node test/check-zero-deps.js` → OK. `package.json` / lockfile unchanged.

## Non-Tautology Confirmation

- The harness spawns `manifest[].entry.command`+args VERBATIM and connects a real
  JSON-RPC stdio client; RETURN = non-empty tool content, never config presence.
- Hook-fires execute `manifest[].command` VERBATIM (node mapped to the runner's
  execPath for CI portability, argv otherwise unchanged) with synthetic stdin;
  ingress = a real frame reaching the transport stub (HTTP request body / UDS
  framed body), carrying `provider`. Asserts `provider`, never `source_domain`.
- Mutation controls (all present and PASSING as failures): broken `entry.command`
  (nonexistent path / blanked) → `assertRetrievalReturns` throws, per behavioral
  leg (claude, codex, opencode); broken hook client path → `observeHookFire`
  returns `fired:false`. Each keeps the manifest string INTACT so a presence
  proxy would pass — the tautology guard test makes that gap explicit.

## Gate-0 handling (codex, per test-plan/OVERVIEW)

Codex hook-fire discharge EXECUTES the exact wired command with synthetic stdin,
bypassing codex: command-level firing, local firing, and both retrieval-return
arms are HARD (all pass). Codex SELF-firing the 7 events in a real trusted
`.codex/` is recorded per-event as `wired-inactive (untestable-in-CI: codex not
installed / trust unconfirmed)` via an explicit assertion — a named documented
gap, never a silent skip.

## Per-event firing evidence (observed, cloud/local command-level)

All 7 (SessionStart, UserPromptSubmit, PreToolUse, PostToolUse, PreCompact,
SubagentStart, Stop) → **fires** (ingress observed). `provider=codex-cli` stamped
on the RecordEvent-family events (UserPromptSubmit, PreToolUse-cycle, PostToolUse,
SubagentStart); session-lifecycle/sync frames (SessionRegister/SessionClose/
CompactPayload) fire but carry no provider field — asserted only where present.

## Issues / Blockers

- None blocking. Stage 3c executes against real local+cloud servers; the harness
  drives a real `mcp-bridge.js` unchanged (identical newline-delimited framing).
- Pre-existing (NOT touched): `test_remote_install_under_290kb` footprint gate is
  RED on main (312005 > 290000). No heavy deps added; verifier is lean.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_search (pattern) — no directly-relevant C14
  verifier pattern existed; (decision, topic nan-023) — surfaced ADR-006 (#5768),
  ADR-001 (#5763), ADR-002 (#5764), which I applied.
- Stored: entry #5777 "C14 verifier: claude LOCAL hook leg is the legacy
  Rust-binary command, not the JS client — fire-tests must use the cloud/bridge
  manifest" via context_store (pattern) — captures the local-vs-cloud hook
  command-shape trap, which events carry `provider`, the ingress-observation
  mechanics, and the non-tautology fixture approach.
