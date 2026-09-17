# Agent Report — nan-023 Wave B: opencode-retrieval

**Agent:** nan-023-agent-3-opencode-retrieval (role: uni-js-dev)
**Component:** opencode retrieval writer — `writeOpencodeMcp` in `lib/opencode-install.js`
**Commit:** 74adcba4 `impl(opencode-retrieval): additive mcp.unimatrix WireLeg writer (#990)`

## Summary

Added `writeOpencodeMcp(dir, opts, dryRun) -> WireLeg` — an additive, non-clobbering
writer for the opencode `mcp.unimatrix` retrieval entry. Reuses the module's existing
`readJsonSafe`, `detectIndent`, `isWithinProject`, `writeJson`. Touches ONLY
`config.mcp.unimatrix`; the Ollama `provider` block, foreign `mcp.*` servers,
`permission`, and all non-Unimatrix keys are preserved byte-for-byte (C-05 / vnc-049).

## Files modified

- `/workspaces/unimatrix/packages/unimatrix/lib/opencode-install.js` (+246): `writeOpencodeMcp`,
  `buildOpencodeEntry`, `resolveOpencodeTransport`, `opencodeLeg`, `isPlainObject`, `deepEqual`;
  exports `writeOpencodeMcp`, `buildOpencodeEntry`.
- `/workspaces/unimatrix/packages/unimatrix/test/opencode-install.test.js` (+246): 13 new tests
  across sentinel preservation, fresh additive write, local/cloud entry shape, idempotence,
  malformed fail-safe, intent gate, dry-run, containment.

## AC-10 schema resolution (pseudocode open question 6)

**Resolved: `{type:"local", command:[<absBinary>], environment:{LD_LIBRARY_PATH:dirname(binary)}, enabled:true}`.**

Evidence — three concurring sources:
1. **Ground truth**: the repo's hand-authored `/workspaces/unimatrix/opencode.json` `mcp.unimatrix`
   uses `type:"local"`, a `command` ARRAY (single absolute binary path, no subcommand),
   `environment.LD_LIBRARY_PATH` = the binary's dir, and `enabled:true`.
2. The existing test `sentinelConfig()` mirrors the same `type/command-array/environment` shape.
3. `init.js` `writeMcpJson` sets `env.LD_LIBRARY_PATH = path.dirname(binaryPath)` for the native
   binary — the same requirement, under opencode's `environment` key name.

Key divergences from claude-code `.mcp.json` (deliberately honored): opencode uses a `command`
argv ARRAY (not `command`-string + `args[]`), and the env key is `environment` (not `env`).
`LD_LIBRARY_PATH` is load-bearing: without it the native binary cannot resolve shared libs, so the
entry would be present-but-not-spawnable and retrieval would not RETURN. Stored as pattern #5773.

The pseudocode's simplified entry omitted `environment`; I included it as required-for-spawnable,
which the pseudocode explicitly licensed ("Emit exactly what opencode reads so retrieval RETURNS...
confirm against the opencode config the repo already ships"). Not an invented key — it is in the
shipped reference.

## Constraint compliance

- **C-05 sentinel**: writer mutates only `mcp.unimatrix`; provider/permission/foreign keys
  byte-for-byte preserved (tests assert JSON-region equality). Note: `mcp.unimatrix` itself is
  Unimatrix-owned, so a *stale* entry is updated to canonical shape; an already-correct entry is
  `unchanged` with no write (idempotent steady state → byte-stable).
- **AC-10 return path**: emitted entry is spawnable (see above).
- **Q1 token-free cloud**: cloud emits `command:["node", bridgePath, projectHash]`, never `url=`.
  A bare `url` with no bridge descriptor yields `skipped` (never a token-bearing entry); test asserts
  no secret/url leaks into the leg.
- **AC-04 idempotence / AC-14 dry-run / AC-12 containment / AC-15 malformed**: covered, never throws.
- **AC-11 intent gate (writer-visible)**: a NEW entry requires `harnessSelected:true`
  (`--harness opencode`); merging into an existing entry is additive. Single decision site in the writer.

## Notes / flags for downstream (Wave C orchestrator + verifier)

- **Signature reconciliation (OVERVIEW open question)**: `writeOpencodeMcp` accepts
  `{binaryPath?, url?, transport?, bridgePath?, projectHash?, harnessSelected?}`. Preferred cloud
  input is a resolved `transport` descriptor (`{kind:"stdio-bridge", bridgePath, projectHash}`) or
  the explicit `bridgePath`+`projectHash` pair. `url` is accepted for signature-compat but never
  emitted (Q1). The orchestrator should resolve the `Transport` and pass it through.
- **`skipped` action for containment / no-transport**: per this component's pseudocode
  (`leg("skipped", ...)`) and its test plan, containment-escape and unresolvable-transport use a bare
  `action:"skipped"` (with `reason`) rather than a `skipped-*` literal from the OVERVIEW enum. The
  orchestrator's manifest printer/verifier should treat any `skipped`/`skipped-*` leg as a visible
  skip. Flagging so the enum handling stays lenient.
- `harnessSelected` is the flag the orchestrator forwards from `--harness opencode` for the
  new-entry intent gate.

## Test + gate results

- `node --test test/opencode-install.test.js`: **40 pass / 0 fail** (13 new + 27 pre-existing).
- `node test/check-zero-deps.js`: OK — no runtime deps; `package.json`/lockfile unchanged.
- `node test/check-hook-client-size.js`: OK — stripped 105243/110000, raw 191416/200000
  (my file is not under the gate; run for completeness).
- Did NOT run/modify integration tests (Stage 3c). `lib/codex-install.js` shows as modified in the
  shared worktree — that is the parallel Wave B codex agent, not mine; my commit contains only my two files.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search (pattern + decision) — surfaced ADR-001
  (#5763, WireLeg contract), ADR-002/005; no prior opencode mcp-schema pattern existed. Confirmed the
  spawnable schema from the shipped reference `opencode.json` + `writeMcpJson`.
- Stored: entry #5773 "opencode mcp.unimatrix must emit type:local + command ARRAY +
  environment.LD_LIBRARY_PATH to be spawnable" via /uni-store-pattern.
