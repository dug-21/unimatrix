# Agent Report — nan-023 Wave A · hook-client-provider

**Agent:** nan-023-agent-3-hook-client-provider (role: uni-js-dev)
**Component:** hook-client `--provider` argv hint (ADR-003 §1/§2, Q5)
**Status:** COMPLETE

## Report

### Files modified
- `packages/unimatrix/lib/hook-client/index.js` — added `parseHookArgs(argv) -> {event, providerHint}`; wired it into `main()` so a present, KNOWN hint takes the hint path (`normalize.mapToCanonical` + stamp `input.provider = hint`); absent/unknown hint keeps today's `normalizeEventName` inference (byte-identical, SR-07). Exported `parseHookArgs`.
- `packages/unimatrix/lib/merge-settings.js` — `buildHookClientCommand(clientPath, event, providerHint?)` gained the optional 3rd arg appending `" --provider <hint>"`. 2-arg call sites unchanged.
- `packages/unimatrix/test/hook-client/index.test.js` — new `parseHookArgs` unit block (6 tests) + behavioral spawn block (3 tests: codex-cli hint stamps `provider=codex-cli`, no hint infers `claude-code`, unknown hint falls back to inference exit-0). Extended `runEntry` with optional `options.args`.
- `packages/unimatrix/test/merge-settings.test.js` — new `buildHookClientCommand` provider-hint block (7 tests: backward-compat, appends flag, empty-hint omits, targets JS client not binary, matches ownership regex, spaced-path quoting, every-event-carries-flag).

### Design adherence
- No normalizer arm added; `normalize.js` and `hook.rs` untouched (Q5 — the `codex-cli` arm already exists in both). Reused `KNOWN_PROVIDERS`, `mapToCanonical`. Parity corpus #4751 unaffected; `parity-opencode-arm` and `normalize` tests still green.
- Fail-open preserved: `parseHookArgs` is a pure argv scan that never throws; unknown/malformed hint → inference; no new stdout writes; exit-0 contract intact.
- C-04 fail-loud: `buildHookClientCommand` with `"codex-cli"` always emits the flag; test `test_every_hook_event_carries_provider_flag` asserts every event carries it.
- `source_domain` excluded from all assertions (forced `claude-code` at ingress, #5748) — tests assert `provider` only (R-05).
- Zero new dependencies (`check-zero-deps` green); `package.json`/lockfile unchanged.

### Tests
- Component suites: `index.test.js` 63 pass / 0 fail (was 54; +9), `merge-settings.test.js` 81 pass / 0 fail (was 74; +7).
- Supporting: `size-gate.test.js` 21 pass, `normalize.test.js` 22 pass, `parity-opencode-arm.test.js` 36 pass. No new failures anywhere.

### hook-client size gate — PASS
- Stripped: **105,243 / 110,000** (PRIMARY) — 4,757 B headroom.
- Raw: **191,416 / 200,000** (BACKSTOP) — 8,584 B headroom.
- Gate NOT raised (R-09, #5372/#4780).

### Issues / ambiguity
- `merge-settings.js` is listed under the brief as a shared file (~+3 lines). My component pseudocode explicitly owns the `buildHookClientCommand` 3rd-arg change; the codex writer (which *calls* it with `"codex-cli"`) is owned by the codex-install component. No overlap in edits.
- Shared worktree note: `lib/init.js` and `test/init.test.js` show uncommitted changes from a sibling Wave-A agent (skills-installer). I staged and committed ONLY my four files; those sibling files were left untouched for that agent to commit.
- Behavioral `provider=codex-cli` firing/return is fully proven in Stage 3c (c14-verifier); I added a local stub-server spawn assertion as component-level confirmation — no integration tests run or modified.

### Commit
- `83267dc1 impl(hook-client-provider): parse --provider argv hint into existing hint path (#990)`

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` (pattern + decision) — surfaced size-gate patterns (#5377, #5378, #5372), the observation-harness split-brain pattern (#5737), and nan-023 ADRs (#5765 ADR-003). Applied: budget against the stripped total, keep JS/Rust twins in lockstep, no normalizer arm.
- Stored: entry #5770 "Routing a --provider hint in hook-client index.js must use mapToCanonical, never normalizeEventName" via `/uni-store-pattern` — the non-obvious trap that `normalizeEventName` re-infers `claude-code` for shared event names and would silently clobber the hint.
