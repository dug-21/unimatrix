# Agent Report — nan-023 Wave C: Wire Orchestrator

**Agent:** nan-023-agent-3-wire-orchestrator (role: uni-js-dev)
**Component:** `lib/wire.js` — `wire()` + `detectHarnesses()`, intent-gate forwarding, WireLeg aggregation; init.js wire seam; claude writers routed through wire.js.
**Feature:** nan-023 (#990) · Stage 3b, Wave C

## Result

`wire(projectRoot, {harness?, clientPath, binaryPath?, mcp?, dryRun}) -> {actions, manifest:WireLeg[]}`
and `detectHarnesses(dir)` implemented in a new `lib/wire.js` (295 lines, < 300 budget). The
orchestrator detects harnesses by project-local markers, selects the target set, dispatches the
per-(harness × surface) writers, and aggregates every `WireLeg` into the manifest the C14 verifier
and the dry-run printer consume. `init()` now routes claude MCP+hooks + opencode + codex through a
single `wire()` call (dropped inline Steps 3/4/5b); the `wire` verb calls `wire()` from `bin`
(cli-routing agent's component — not edited).

## Files created / modified

- `packages/unimatrix/lib/wire.js` (new, 295 lines) — orchestrator.
- `packages/unimatrix/lib/init.js` (modified) — wire seam: `init()` local path delegates
  claude+opencode+codex to `wire()`; added `resolveClientPath()`; exports `resolveClientPath`.
  `initRemote()` intentionally left unchanged (legacy/cloud path; out of the golden's blast radius).
- `packages/unimatrix/test/wire.test.js` (new) — 18 component tests.
- `packages/unimatrix/test/fixtures/wire/golden/mcp.json` (new) — committed backward-compat golden.
- `packages/unimatrix/test/fixtures/wire/golden/settings.json` (new) — committed backward-compat golden.

## Tests

- `wire.test.js`: **18/18 pass** (detection, golden byte-identical, manifest contract, intent gate,
  fail-safe, idempotence, dry-run==real).
- Affected component files run **sequentially** (`--test-concurrency=1`): **275/275 pass**
  (wire, codex-install, opencode-install, merge-settings, init, init-remote, codex-toml-surgical, bin-mcp-bridge).
- Note on the full concurrent suite: init/init-remote/remote-client tests exhibit a **pre-existing
  concurrency race** (they mutate the package's real `skills/` dir and `~/.unimatrix`, and `node --test`
  runs files in parallel). The failing set is RANDOM across runs and reproduces on the clean baseline
  (stash) **without** my code. Each affected file passes in isolation. Not a regression from this component.

## Backward-compat golden (SR-07 / C-10)

- **Captured FIRST** (before routing claude through wire.js) by running the reused writers
  (`writeMcpJson` + `mergeSettings` with the legacy binaryPath string) against a fixture with a FIXED
  machine-independent binaryPath (`/opt/unimatrix/bin/unimatrix`) → committed as
  `test/fixtures/wire/golden/{mcp.json,settings.json}`.
- **Byte-identical assertion PASSES**: `test_wire_claude_mcp_settings_byte_identical_to_golden` asserts
  wire()'s claude common-path output equals the committed golden byte-for-byte (`Buffer.equals`). The
  claude writers are reused UNCHANGED; local hooks pass the legacy binaryPath string form → identical bytes.

## Key design decisions (documented for the verifier / reviewer)

- **claude-code is always ensured, never detection/intent-gated** (ADR-005 §3). Only opencode/codex are
  detection-gated; only their NEW user-owned-config entries are intent-gated.
- **Single intent-gate site**: writers self-gate (`writeOpencodeMcp` `harnessSelected`,
  `writeCodexMcpToml` `harnessSel`); the orchestrator only forwards the `--harness` opt-in. No double-gate.
- **Two error postures**: claude reuses the loud writers (throw on malformed claude config → propagates,
  SR-07); opencode/codex are fail-safe (warn-and-skip → `skipped-malformed` leg, never throw, AC-15).
  Both asserted (`test_wire_claude_malformed_throws_backward_compat`, `test_wire_opencode_malformed_never_throws`).
- **Circular require broken** by lazy-requiring `init.js` inside claude dispatch (init.js `module.exports=`
  replaces the object at end-of-file → a top-level require would capture partial exports).
- **One hooks WireLeg per managed event** (claude + codex) so the manifest carries every exact command
  string for the C14 verifier (R-02); SubagentStop opt-in honored via reused `subagentStopEnabled`.

## Issues / blockers

- **PRE-EXISTING (flag for feature/SM): the 290 KB remote-install footprint HARD GATE is blown
  feature-wide.** `test_remote_install_under_290kb` fails at **360784 bytes** (lib=292364, skills=68420;
  limit 290000). It **already fails on the clean baseline without wire.js** (~348834 bytes) — the whole
  nan-023 feature branch (codex-install, opencode additions, etc.) has exceeded it. My `lib/wire.js`
  adds ~11.9 KB, worsening an already-failing gate. Per policy I did **not** self-raise the gate; this is
  a human decision to record on #990 (raise the gate or trim lib/). This is a separate gate from the
  hook-client size gate (which PASSES with headroom: stripped 105243/110000, raw 191416/200000).
- The hook-client size gate and zero-dep check both PASS; `package.json`/`package-lock.json` unchanged.
- `initRemote()` cloud path was left unchanged (not routed through wire()); OVERVIEW's cloud-wire() call
  is deferred — the local init path + wire verb are the golden-protected surfaces this wave delivers.

## Self-check

- [x] `node --test` green for affected modules in isolation / sequential (275/275); full-suite failures
      are the pre-existing concurrency race, reproduced on clean baseline.
- [x] hook-client size gate PASS (stripped 105243/110000, raw 191416/200000 — wire.js not under this gate).
- [x] `package.json` / `package-lock.json` unchanged — zero new dependencies.
- [x] Every new fs call wrapped (`existsSafe`); wire never throws for opencode/codex; claude loud-throw
      preserved (SR-07). No stdout on failure paths (summary via reused printer only).
- [x] Wire changes additive-only; WireLeg contract honored; no hand-authored wire types; golden captured
      via reused writers, not edited to pass.
- [x] No TODO/FIXME/HACK/placeholder in non-test code.
- [x] Modified files within component scope (wire.js, init.js seam, golden fixtures, wire.test.js).
- [x] CommonJS throughout (matches edited files); no mixed module syntax.
- [x] Did NOT run/modify integration tests as owned changes (Stage 3c owns those); bin/unimatrix.js not edited.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` + `context_search(pattern)` + `context_get(#5774)` —
  surfaced ADR-001/005 decisions and the codex-install Wave B trap set (WireLeg `note`-vs-`reason`
  invariant, self-gating order, `url=` prohibition, per-event hook legs). Applied all.
- Stored: entry #5776 "wire-orchestrator: pattern" via `context_store` (category pattern) — the circular
  lazy-require trap, claude always-ensured / single-gate-site rules, claude-cannot-report-unchanged
  idempotence-test guidance, and the fixed-binaryPath golden-capture technique.
