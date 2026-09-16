# Test Plan — Wire Orchestrator (`lib/wire.js`)

> Component: `wire(projectRoot, {harness?, clientPath, binaryPath?, mcp?, dryRun}) -> {actions, manifest}` + `detectHarnesses(dir)` (ADR-001/005)
> Runner: `node --test` · Test file: new `test/wire.test.js`, temp `.git` fixtures (`makeTempProject` idiom).
> Primary risks: **R-11** (skip masks unwired leg), **R-01** (manifest is verifier's input contract). Secondary: R-12 (intent gate), R-14 (containment), R-13.
> ACs: AC-03, AC-04, AC-11, AC-13, AC-15. The manifest (`WireLeg[]`) is the verifier's sole contract — malformed/absent WireLeg breaks R-01/R-02 downstream.

The orchestrator itself writes nothing; it detects, applies the intent gate, dispatches per-harness
writers, and aggregates `WireLeg[]`. Tests use stubbed/real writers against fixtures and assert the
**manifest contract** — every dispatch path yields a well-formed `WireLeg`, including all `skipped-*`.

## Detection (AC-13, FR-15)

- `test_detectHarnesses_all_markers` — fixture with `.mcp.json`/`.claude/`, `opencode.json`/`.opencode/`, `.codex/` → `{"claude-code":true,"opencode":true,"codex-cli":true}`.
- `test_detectHarnesses_none` — bare repo → all false.
- `test_detectHarnesses_partial` — only `.codex/` present → codex true, others false.
- `test_detectHarnesses_ignores_gemini` — `.gemini/` present → not in the returned set (out of scope).

## Manifest contract (R-11, R-01 — every leg is a well-formed WireLeg)

- `test_wire_undetected_harness_yields_skipped_undetected_leg` — no `.codex/` marker → a `WireLeg{action:"skipped-undetected"}` is present in the manifest (a **reported** outcome), no write, exit success (AC-13).
- `test_wire_skip_reasons_are_distinct` — `skipped-undetected` vs `skipped-malformed` vs `skipped-intent` are DISTINCT, visible manifest actions — a malformed skip is never presented as success (R-11, #4473, AC-15).
- `test_wire_every_dispatch_returns_wellformed_leg` — for each surface, the aggregated leg has required fields (`harness, surface, action, path`), and `command`/`entry` present for hooks/mcp respectively (verifier input contract, integration risk).
- `test_wire_actions_and_manifest_consistent` — `actions[]` (human summary) is derived from the same manifest the verifier consumes (dry-run printer + verifier + summary share one source, ADR-001 §4).

## Intent gate dispatch (AC-11, R-12)

- `test_wire_no_harness_gates_new_entry` — detected opencode/codex lacking the entry, no `--harness` → orchestrator emits `skipped-intent` legs (not writes).
- `test_wire_harness_selects_single` — `--harness codex-cli` → only codex writers dispatched; undetected named harness → `skipped-undetected` leg, not an error (ADR-005 §2).
- `test_wire_claude_common_path_not_gated` — claude-code MCP dispatched without opt-in (backward compat).

## Fail-safe posture (AC-15)

- `test_wire_leg_never_throws_on_malformed` — a writer handed malformed input returns `skipped-malformed`; `wire()` **never throws** and does no partial write (contrast: init's loud claude checkpoint is preserved — asserted in `cli-routing.md`).

## Idempotence + dry-run + containment

- `test_wire_run_twice_manifest_actions_unchanged` — second run → legs report `unchanged`; surfaces byte-identical (AC-04).
- `test_wire_dryRun_manifest_equals_real` — dry-run manifest action set == real-path action set; zero writes (AC-14).
- `test_wire_containment_guarded` — every dispatched writer is `isWithinProject`-guarded; a path escaping root → `skipped` (AC-12).

## Coverage note

This plan proves the **manifest contract** that R-01/R-02's verifier depends on. Behavioral return/fire assertions are NOT here — they belong to `c14-verifier.md`, which executes this manifest.
