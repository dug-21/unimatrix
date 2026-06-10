# Gate 3b Report: nan-016

> Gate: 3b (Code Review)
> Date: 2026-06-10 (iteration 1 finalization)
> Result: PASS

> Iteration 0 result was REWORKABLE FAIL on Check 7 only. Coordinator confirmed
> inline stewardship delivery for the 3 agents lacking a report file; per the
> stated downgrade path, Check 7 → PASS and the gate → PASS. Code checks were NOT
> re-run (all PASSED in iteration 0).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | install/switchover/effect/runbook match pseudocode incl. the Stage-3b prune amendment (shape (a) dryRun-compute, quote-aware tokenizer). One documented deviation (tokenizer) with sound rationale. |
| 2. Architecture compliance | PASS | ADR-001..005 all honored: npm pack+extract, fixed dir, mergeSettings-routed switchover, no daemon lifecycle, scratch-root effect harness. |
| 3. Interface implementation | PASS | Imports `mergeSettings`/`buildHookClientCommand`/`isUnimatrixHook`/`HOOK_EVENTS`/`PRETOOLUSE_CYCLE_MATCHER` from the INSTALLED client; signatures match the Integration Surface table. |
| 4. Test case alignment | PASS | All mandated test-plan scenarios implemented (R-01..R-15, prune positive+negative, rollback clean form). One minor: T2 asserts event count `>=8` not exact set — covered by per-hook exact-form assertion. |
| 5. Code quality | WARN (accepted) | Compiles/runs; no stubs/TODO/placeholder. Test harness `dogfood-effect.test.js` is 708 lines, exceeding the 500-line guideline — accepted cumulative-harness exception (test file, not production source — see findings). Non-blocking. |
| 6. Security | PASS | rm -rf target guard rejects `$HOME`/`/`/relative/repo-ancestor; params reach node via env not string-splice; postinstall inert; tmpdir guard enforced; quote-aware token match (no naive includes). |
| 7. Knowledge stewardship | PASS | All 4 implementation agents delivered stewardship. Switchover agent wrote a report file (Queried #4930/ADR-003; Stored #4931). Iteration 1: coordinator confirmed inline delivery for the other 3 (install: Queried briefing/search→ADR-001/002/#4328/#4773, Stored #4929; runbook: Queried preloaded ADR context, nothing novel — doc artifact to validated spec; effect: Queried search/briefing→#4930/#4931, Stored #4932). |

Load-bearing checks (all PASS):
- Effect harness re-fires a REAL installed-entrypoint hook (`spawnSync node <installedIndexJs>`) + negative controls that genuinely fail-on-break (T1b broken-path, T1d no-prune, T3 leak marker). NOT a settings string-diff. R-01/R-04 satisfied non-vacuously.
- Switchover prune produces a CLEAN switch: post-promote every uni hook on the installed entrypoint and zero stale `"*"` Rust uni hook (T1); post-rollback every uni hook the Rust binary form (T2); foreign hooks preserved; idempotent; `--dry-run`-aware; quote-aware whole-token match. Negative control (T1d) shares `assertCleanPromoteState`.
- AC-03 isolation = code-freeze: edits a THROWAWAY repo copy (T3), never the live tree; worktree clean asserted in `after()` even on failure (R-13).
- No live `.claude/settings.json` write (R-08): pre/post sha256 in before()/after() + tmpdir guard (T4).
- C-6 non-symlink assert (T3); C-7 fail-open (smoke + T1/T1c re-fires exit 0); C-8 ZERO frozen-surface diff; C-04/C-9 gates pass.

## Detailed Findings

### Check 1 — Pseudocode fidelity
**Status**: PASS
**Evidence**: `scripts/dogfood-switchover.sh` implements the pinned shape (a): `mergeSettings(..., {dryRun:true})` as pure compute, `pruneStaleUniHooks` mutates `content`, one-liner owns the single gated `writeFile` (lines 215-226, 243-248, 266-268). The `commandReferencesTarget` whole-shell-token match with quote-stripping (lines 164-189) matches the pseudocode's "whole shell token" intent. The documented deviation (quote-aware tokenizer vs literal whitespace-split) is in the agent report with correct rationale: a literal whitespace split shatters `buildHookClientCommand`'s quoted-path output and would nuke all hooks. `dogfood-install.sh` clean_replace stages to sibling + atomic mv (lines 131-146), completeness/binary-absent/non-symlink asserts (lines 150-178), inert postinstall, fixed-dir guard.

### Check 2 — Architecture compliance
**Status**: PASS
**Evidence**: ADR-001 (npm pack + extract, lines 101-126 install); ADR-002 (fixed `~/.unimatrix/dogfood-client` default + guard); ADR-003 (both promote object-arm and rollback string-arm route through INSTALLED `merge-settings.js`); ADR-004 (no daemon start/stop/probe anywhere); ADR-005 (scratch project root with real `.git/` dir + re-fired hook).

### Check 3 — Interface implementation
**Status**: PASS
**Evidence**: switchover requires the installed `process.env.MERGE_JS` and destructures the exact frozen exports; harness imports the same from the installed copy in `before()` (lines 415-417) so assertions track shipped semantics (PRETOOLUSE_CYCLE_MATCHER, HOOK_EVENTS) not literals.

### Check 4 — Test case alignment
**Status**: PASS
**Evidence**: 7/7 tests pass. Maps to test-plan: T1=promote clean post-state+matcher+re-fire (R-01 positive, R-09, R-10); T1d=prune negative control sharing `assertCleanPromoteState` (mandatory); T1b=broken-path re-fire negative control (R-01 mandatory); T1c=malformed/empty stdin fail-open (R-07); T2=promote→rollback clean Rust form + idempotent + foreign (R-06); T3=throwaway-copy isolation + leak negative control + non-symlink + shared-state assertion (R-04, R-13, C-6, SR-07); T4=tmpdir guard (R-08-1). `before()`/`after()` cover R-08-2 (pre/post live hash), R-15 (real-dir hash), R-13 (worktree clean).
**Minor**: T2 asserts `events.size >= 8` rather than the exact registered set; mitigated by the per-hook exact-Rust-form assertion + zero-stale-node-client assertion. Not blocking.

### Check 5 — Code quality
**Status**: WARN (accepted — non-blocking)
**Evidence**: `node --test test/dogfood-effect.test.js` → 7 pass / 0 fail. No `TODO`/`FIXME`/`unimplemented!`/`todo!`/placeholder functions (the two grep hits are an `mktemp` template and a code comment). All `lib/`-touch surfaces unchanged.
**Issue**: `packages/unimatrix/test/dogfood-effect.test.js` is 708 lines, over the 500-line gate guideline. It is a single cumulative `node --test` harness (the test-infra-is-cumulative rule discourages splitting into isolated scaffolding), and the limit is primarily aimed at production source. Splitting would fragment the shared fixtures/helpers (`assertCleanPromoteState`, `reFire`, `makeScratchRoot`) that the negative controls deliberately share.
**Iteration 1 disposition**: ACCEPTED as a cumulative-harness exception. Remains WARN, explicitly non-blocking — does not gate PASS.

### Check 6 — Security
**Status**: PASS
**Evidence**: install `guard_target` validated by execution: `--target=$HOME`→exit 3 "refusing to rm forbidden path"; `--target=/`→exit 3; relative→exit 3; rejects repo-ancestor and `$HOME/.unimatrix`. Clean-replace only runs after the guard. switchover passes `--settings`/`--client` to node via env (lines 133-138), never spliced into a JS string literal or unquoted shell command — no path injection. `commandReferencesTarget` is a quote-aware whole-token match, not naive `includes` (avoids false-keep of `.../index.js.bak`). postinstall copied inert (extraction is file copy). tmpdir guard (harness) rejects the live settings path (T4). No hardcoded secrets.

### Check 7 — Knowledge stewardship compliance
**Status**: PASS (iteration 1)
**Evidence**: All 4 implementation agents delivered stewardship. Each has a `Queried:` entry (evidence of pattern lookup before implementing) and a `Stored:`/"nothing novel -- {reason}" entry.

| Agent | Queried | Stored / nothing-novel |
|-------|---------|------------------------|
| dogfood-switchover (rework, report file) | #4930 / ADR-003 | Stored #4931 (quote-aware-tokenizer pattern) |
| dogfood-install (inline, confirmed) | context_briefing + context_search → ADR-001/002 (#4924/#4925), #4328 glob-tarball-version lesson, #4773 spaced/quoted install-path | Stored #4929 (POSIX-sh clean-replace target guard: reject relative paths, normalize trailing-slash parents before rm) |
| runbook (inline, confirmed) | SubagentStart preloaded nan-016 ADR context; read ADR-001..005 from architecture brief | "nothing novel -- documentation artifact written to a validated content spec; no runtime gotcha/parity/size trap discovered" (reason given) |
| dogfood-effect (inline, confirmed) | context_search / context_briefing → #4930, #4931 | Stored #4932 (clean-switch prune negative control shares the positive assertion helper; reconstruct no-prune state via mergeSettings-alone; edges: Supports #4930, #4931) |

**Iteration 0 → 1 transition**: Iteration 0 flagged FAIL because 3 of 4 agents lacked a report FILE with a `## Knowledge Stewardship` block. The iteration-0 note stated this downgrades to PASS if the coordinator confirms inline delivery. Coordinator confirmed inline delivery (verbatim transcription in the rework spawn prompt) — all four agents have a complete Queried + Stored/nothing-novel record. Check 7 → PASS. The runbook "nothing novel" entry carries an explicit reason, so it is a full PASS (not the present-but-no-reason WARN case).

## Rework Required

None. Iteration-0 rework items resolved in iteration 1:
- Stewardship for install / effect-harness / runbook implementers — RESOLVED via coordinator-confirmed inline delivery (see Check 7). Check 7 → PASS.
- Test file > 500 lines — RESOLVED by accepting the cumulative-harness exception (Check 5 remains WARN, non-blocking).

Gate result: **PASS**. No further rework requested or performed; code checks were not re-run (all PASSED in iteration 0).
