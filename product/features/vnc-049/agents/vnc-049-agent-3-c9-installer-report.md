# Agent Report — C9 Installer OpenCode Branch (vnc-049)

Agent: vnc-049-agent-3-c9-installer (role: uni-js-dev) · Wave 4 · Commit faedeb99

## Files modified
- `packages/unimatrix/lib/opencode-install.js` (NEW) — non-clobbering additive OpenCode provisioning.
- `packages/unimatrix/lib/init.js` (MODIFY) — thin delegating call (import + one call after copySkills + export).
- `packages/unimatrix/test/opencode-install.test.js` (NEW) — 24 component tests.

## What was built
- `detectsOpenCode(dir)`: presence of `opencode.json` or `.opencode/` (fail-safe).
- `maybeProvisionOpenCode(dir, {dryRun})`: detection-gated entry point wired into the local `init()` flow (Step 5b). Strictly additive; no-op when not detected; never throws.
- Mode A (primary, ADR-006 §2): drops a self-contained ESM re-export shim into `.opencode/plugins/unimatrix.js` + non-clobber dep add to `.opencode/package.json`. Touches ONLY `.opencode/` — never the retrieval sentinel file.
- Mode B (alternate): additive append of the plugin package to `opencode.json` `plugin[]`, mutating only that key at the file's own indentation.
- Idempotence: `includes()`-dedupe (array) + `hasOwnProperty` guard (dep) + identical shim bytes on re-run (nan-004 #1195 principle).
- Fail-safe: malformed config → warn + skip that leg, file preserved verbatim, never throws (opposite of init's loud-checkpoint posture — modeled on cleanStaleRemoteSubtree).
- R-15: all write targets derived from FIXED constants + containment-checked (`isWithinProject`); config content treated as JSON data only, injected paths never followed/executed.

## Tests
- 24 new C9 tests + existing installer regression suites (init.test.js, merge-settings.test.js) run together: **112 pass / 0 fail**.
- Covers: detection (3), provisioning (5), byte-for-byte preservation of mcp.unimatrix / STDIO command / Ollama provider / non-Unimatrix keys (4), idempotence (3), R-15 untrusted input (4), malformed/edge fail-safe (4), pure merge helpers (3).
- AC-05c (live `context_*` retrieval still returns) is deliberately NOT exercised here — it is the Stage 3c integration/infra-001 leg. Did not run or modify integration tests.

## Gates
- Zero new deps: PASS (`check-zero-deps.js` — package.json/lockfile unchanged).
- hook-client size gate: PASS (stripped 103423/110000, raw 185363/200000) — new module is in `lib/`, not `lib/hook-client/`, so it does not consume that budget.
- `opencode-install.js` ≈ 340 lines, under the 500 code-line cap.

## Coordination flag (C1 dependency)
C1 (`packages/unimatrix/opencode-plugin/`) is NOT yet landed in this tree. C9 provisions the artifact by a shared identity constant: `PLUGIN_PACKAGE = "@dug-21/unimatrix-opencode-plugin"`, exported symbol `UnimatrixObservePlugin`, shim file `unimatrix.js`. C1 pseudocode explicitly requires C1 and C9 to agree on the artifact name — **C1 must publish under this package name / export, or the constant in `opencode-install.js` must be reconciled at C1 landing.** The dep version is read from the host `@dug-21/unimatrix` package.json (`^<version>`), fallback `*`.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search — surfaced ADR-006 (#5743), nan-004 prefix-match pattern (#1195, applied), ADR-004/ADR-001 context. Applied the nan-004 dedupe/non-clobber principle to the plugin-array + package-dep surface.
- Stored: FAILED — `context_store` returned "pool timed out while waiting for an open connection" on two attempts (server unavailable). Pattern to store when the server recovers: "OpenCode installer branch is fail-SAFE, unlike the rest of init.js which throws" (topic `opencode-install`) — covers the posture-inversion trap, what byte-for-byte preservation actually asserts (serialized subtrees, not whole-file), and the R-15 config-injected-path vector.
