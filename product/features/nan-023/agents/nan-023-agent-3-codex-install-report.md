# Agent Report — nan-023-agent-3-codex-install (Wave B)

Role: uni-js-dev · Component: codex-install (Wave B writers) · Feature: nan-023

## Summary

Implemented the Wave B fs-touching writers on top of the committed Wave A surgical
TOML helpers in `lib/codex-install.js`:

- `maybeWireCodex(dir, {clientPath, binaryPath?, url?, transport?, harnessSel?, dryRun}) -> WireLeg[]`
  — fans out to the MCP TOML writer + hooks writer, fail-safe (never throws out of
  the wire layer, AC-15), and surfaces the codex trust precondition on every leg
  via an auxiliary `note` field (NFR-09).
- `writeCodexMcpToml(dir, opts, dryRun) -> WireLeg` — surgical `[mcp_servers.unimatrix]`
  writer. Self-gates intent (AC-11); checks `readTomlTable(...).malformed` BEFORE the
  intent gate; local shape `command = "<binaryPath>"`, cloud shape
  `command="node", args=["<bridge>","<hash>"]`; NEVER emits `url =` (Q1/Principle 8);
  all values routed through the Wave A `tomlString` escaper (R-10).
- `writeCodexHooks(dir, {clientPath, dryRun}) -> WireLeg[]` — **widened to WireLeg[]**
  per OVERVIEW recommendation (a): one leg per event so the manifest carries all 7
  exact command strings (SR-09/R-02). Targets `node <clientPath> <EVENT> --provider codex-cli`
  via `buildHookClientCommand(..., "codex-cli")` — never the binary (AC-08/C-03);
  `--provider codex-cli` fail-loud via `requireProviderFlag` (NFR-07/C-04);
  7-event set (Q4), foreign hooks preserved via `isUnimatrixHook` scoping.

Signature widening flagged and adopted: `writeCodexHooks -> WireLeg[]` (OVERVIEW option a).
`writeCodexMcpToml` opts carry a `transport` descriptor (OVERVIEW Q1 reconciliation);
`resolveCodexTransport` also accepts bare `binaryPath` / `bridgePath`+`projectHash`
for the stated component signature. A bare `url` is intentionally NOT a transport
source (resolves to `kind:"none"` → skipped-undetected) to honor Principle 8.

## Files modified
- `packages/unimatrix/lib/codex-install.js` (extended; Wave A helpers untouched)
- `packages/unimatrix/test/codex-install.test.js` (new, 25 tests)

## Results
- Tests: 25/25 pass (`node --test test/codex-install.test.js`). Related suites
  (codex-toml-surgical, merge-settings, opencode-install, codex-install) 173/173 pass.
- Zero-deps: OK (package.json / lockfile unchanged).
- hook-client size gate: OK — stripped 105243/110000, raw 191416/200000. codex-install.js
  is NOT under this gate (lives in `lib/`, gate counts `lib/hook-client/` only); hook-client untouched.
- `lib/codex-install.js`: 621 lines — over the <450 target (Wave A already consumed
  ~278), well under the 1000-line hard cap.

## Issues / blockers
- None blocking. Note the 450-line target was a pre-Wave-A whole-file estimate; both
  waves land at 621 (< 1000 hard cap). Did not modify integration tests (Stage 3c owns those).
- WireLeg action-literal mapping for two defensive branches (path-escape, no-transport):
  used `skipped-undetected` (no dedicated literal exists among the six); disambiguated by `reason`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) and context_get #5770 —
  found ADR-001/003/005/006 decisions, the mapToCanonical provider-hint pattern (#5770),
  and the two-level TOML merge pattern; applied the wire-manifest + provider-hint contracts.
- Stored: entry #5774 "codex-install: pattern" via context_store (category pattern) —
  the WireLeg reason-iff-skipped vs. trust-note-on-success trap, intent-gate-after-malformed
  ordering, never-emit-url= transport rule, WireLeg[]-per-event, and fail-loud provider flag.
