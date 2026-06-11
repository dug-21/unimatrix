# Gate 3c Report: vnc-031

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-06-11
> Result: PASS

Cross-matcher-group stale Unimatrix-hook prune in `mergeSettings` (Step 3c) + dogfood-switchover prune retire + effect-harness attribution repoint. Install-surface only — pure JS, `node:test`. No Rust / server / MCP.

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Risk mitigation proof (R-01..R-15) | PASS | All 15 risks map to passing tests / satisfied delivery gates; RISK-COVERAGE-REPORT mapping verified against actual test names in source |
| 2. Test coverage completeness | PASS | Every R-to-scenario mapping from RISK-TEST-STRATEGY exercised; discriminating tests confirmed non-vacuous; integration analog (dogfood-effect) ran real, not masked |
| 3. Specification compliance (FR-01..FR-16, AC-01..AC-10) | PASS | All 10 ACs verified by named tests; FRs implemented in merge-settings.js Step 3c |
| 4. Architecture compliance (ADR-001/002/003) | PASS | Object-identity keep-test, registered-events-only prune, script retire all match implementation byte-for-byte |
| 5. Knowledge stewardship | PASS | Tester report carries `## Knowledge Stewardship` with Queried + reasoned "nothing novel" |
| Integration: infra-001 N/A determination | PASS | Confirmed correct — zero Rust/server/transport surface |
| Integration: dogfood-effect harness ran real | PASS | 8/8, 0 skip, `suiteSkipReason` did not fire (real client staged) |
| Integration: GATE C parity (P1–P8) on real input | PASS | T-PARITY calls `mergeSettings` directly on real legacy seed; ordering `7bf45fbe` ancestor of `a4ac286b` verified |
| Integration: GATE B negative control non-vacuous | PASS | T1d reconstructs no-Step-3c state without `mergeSettings`; `assert.throws` on shared helper |
| Integration: no tests deleted; fragment identifiers absent | PASS | Zero deleted `it()`/assertions; grep for PRUNE_FRAGMENT identifiers = no match |
| Integration: full-suite single skip is platform-gated | PASS | `test_root_walk_windows_separators` (Windows-only) masks no vnc-031 coverage |

## Detailed Findings

### Check 1: Risk Mitigation Proof
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md maps each R-01..R-15 to named tests. I verified the load-bearing entries exist in source and are discriminating, not happy-path re-asserts:
- **R-01 (string-compare degradation, Critical)**: `test_cross_group_stale_twin_differing_only_by_shape_pruned` (merge-settings.test.js:830) seeds a stale near-twin `"unimatrix  hook   SessionStart"` (collapsed whitespace) under a non-managed matcher and asserts the survivor is the exact fresh command in the managed group — a naive `command ===` or `includes` keep-rule would fail this. `_survivor_is_exact_fresh_command_not_substring` (:887) and `_pretooluse_star_shares_prefix_with_cycle_survivor` (:859) reinforce. The implementation keeps by `hook !== kept` object identity (merge-settings.js:358), never a command compare. R-01 is closed by construction and guarded by a red-on-regression test.
- **R-02 (zero uni hooks, fail-loud)**: `test_each_event_has_exactly_one_unimatrix_entry_cross_group` (:902) carries the explicit `assert(count !== 0, "managed event " + event + " dropped to zero uni hooks")` self-identifying message (:923). `_only_stale_on_input_managed_entry_created_then_kept` (:931) guards the capture-before-create refactor bug (no managed-group uni entry on input → Step 3 creates, Step 3c must not prune).
- **R-04 (Critical, script-retire parity)**: GATE C below.
- **R-13 (negative control)**: GATE B below.
All other rows (R-03, R-05..R-12, R-14, R-15) map to present, passing tests.

### Check 2: Test Coverage Completeness
**Status**: PASS
**Evidence**: Unit `merge-settings.test.js` 73/73 pass, 0 skip (re-run confirmed). Full `packages/unimatrix` suite 807 tests / 806 pass / 0 fail / 1 skip (re-run confirmed). The RISK-TEST-STRATEGY required scenarios — shape-varying near-twin keep, in+cross-group dedup, both-arm clean migration, idempotency incl. stale-first-run, foreign + near-miss + non-command preservation, emptied-group drop, partition combined-seed, action-string distinctness — all map to named, present tests (`test_partition_combined_subagentstop_optout_and_pretooluse_cross_group`:1183, `_preserves_near_miss_foreign_hook`:1031, `_preserves_non_command_entry`:1049, `_quoted_spaced_path_target_kept`:1276, etc.). No risk from design lacks coverage.

### Check 3: Specification Compliance
**Status**: PASS
**Evidence**: AC-01..AC-10 each verified by the named test in ACCEPTANCE-MAP.md and confirmed present in source. FR-01..FR-16 implemented in merge-settings.js Step 3c (lines 337–374): cross-group prune for managed events (FR-01), keep-by-identity exactly one (FR-02), event-key retention never deleted in 3c (FR-06, line 371 comment + filter), unconditional outside-managed prune (FR-04), action string `Removed stale unimatrix hook: <event> (cross-matcher migration)` distinct from `(opt-out)` (FR-07, line 362), signature unchanged (FR-08). FR-14/15/16 (script retire + harness flip) verified under GATE C / GATE B.

### Check 4: Architecture Compliance
**Status**: PASS
**Evidence**: Implementation matches ADRs byte-for-byte:
- **ADR-001 (keep-by-identity)**: `keptEntryByEvent[event] = newHookEntry` captured once per event after merge resolves (merge-settings.js:334, branch-independent); Step 3c keep test is `hook !== kept` (:358), never a string compare. Matches ADR-001 §Decision exactly.
- **ADR-002 (cross-group generalization, registered-events-only)**: Step 3c loops `for (const event of events)` (:345) — registered events only; Step 3b loops `HOOK_EVENTS \ events` (:381). Partition union=all, intersection=empty, as specified. Reuses the `pruneUnimatrixEvent` filter idiom (:371) without deleting the event key.
- **ADR-003 (script retire)**: `dogfood-switchover.sh` `run_promote` calls `mergeSettings(..., {events, commandForEvent}, {dryRun})` (object arm), `run_rollback` calls `mergeSettings(settingsPath, rustBinary, {dryRun})` (string arm); both own their write via Step 4. No bespoke prune. Matches ADR-003 §Retire mechanics.

### Check 5: Knowledge Stewardship
**Status**: PASS
**Evidence**: RISK-COVERAGE-REPORT.md §Knowledge Stewardship carries a `Queried:` entry (`context_briefing`, surfaced #4939/#4941 ADRs and #4938/#4932 lessons) and a reasoned `Stored: nothing novel` (load-bearing patterns #4938/#4932/#4826 already exist; Node-24 trailing-slash artifact is a generic gotcha deferred under the ≥2-feature rule). Reasoned, not bare.

## Integration Test Validation

### infra-001 N/A determination — CONFIRMED CORRECT
The feature touches `packages/unimatrix/lib/merge-settings.js`, its test, `scripts/dogfood-switchover.sh`, and `packages/unimatrix/test/dogfood-effect.test.js` only (diff-stat verified). Zero Rust / server / transport / MCP surface (NFR-04, Non-Goal 5). infra-001 pytest smoke exercises the compiled `unimatrix-server` binary over MCP JSON-RPC — no suite maps to this change. N/A is the correct determination, not a masked skip. The integration analog is the dogfood-effect harness (real install → real script → real settings), which ran for real.

### dogfood-effect harness ran real (not masked SKIP) — CONFIRMED
Re-run: 8 tests / 8 pass / 0 skip. `suiteSkipReason` did not fire (`skipped 0`), so the real client was staged (npm/tar available) and the parity proof ran on real input, not a guarded `t.skip`.

### GATE C parity proof (P1–P8) on REAL input — CONFIRMED
T-PARITY (dogfood-effect.test.js:695) calls `installed.merge.mergeSettings(...)` DIRECTLY — not `promote()`/`rollback()` which run the script — so a GREEN result is attributable to Step 3c alone, not a residual fragment. Seed `buildSeedSettings()` (:163) carries genuine legacy shapes: stale `"*"` Rust hook (P1), `.bak` (P3), `dogfood-client-OLD` (P4), foreign (P7), sole-stale-in-non-managed-group (P8). GATE C preconditions assert the seed genuinely carries `"*"`/`.bak`/old-dir hooks (:711–723) before merging — a pre-narrowed seed would fail these. Both arms exercised: promote (object, P1/P3/P4/P7/P8) and rollback (string, P2/P3/P5/P7/P8). P6 (quoted spaced-path keep) proven at unit level (`test_cross_group_quoted_spaced_path_target_kept`:1276) — not realizable in an `os.tmpdir()` install dir, correctly documented.
**Ordering gate**: `git merge-base --is-ancestor 7bf45fbe a4ac286b` = true. Parity-proof commit `7bf45fbe` IS an ancestor of fragment-deletion commit `a4ac286b`. Deletion did not precede proof.

### GATE B negative control preserved and non-vacuous — CONFIRMED
T1d (:582) reconstructs the no-Step-3c post-state via `unprunedPromoteContent` (:434), which deliberately does NOT route through `mergeSettings` (which now bakes in Step 3c and would return a clean state, making the control vacuous — #4932). It applies Step-3-only managed-matcher repoint, leaving stale cross-group uni hooks intact. The test first sanity-asserts `staleNoPrune.length >= 1` (:601, non-vacuous guard), then `assert.throws` on the SHARED `assertCleanPromoteState` helper (:606). A regression to a no-op Step 3c reproduces exactly this state and turns the positive T1 red.

### No tests deleted; PRUNE_FRAGMENT identifiers absent — CONFIRMED
`grep -E 'PRUNE_FRAGMENT|pruneStaleUniHooks|commandReferencesTarget|shellTokens|emitAndWrite' scripts/dogfood-switchover.sh` → no match (exit 1). Diff vs base `379d6ec3`: zero deleted `it()` blocks or `assert.` lines in either test file (merge-settings.test.js = 641 pure insertions; dogfood-effect.test.js additions only). The 147 deletions are concentrated in `dogfood-switchover.sh` (135 lines — the retired fragment) and merge-settings.js (no test loss).

### RISK-COVERAGE-REPORT includes harness/integration counts + infra-001 N/A rationale — CONFIRMED
Report §infra-001 carries the verbatim N/A rationale; §Test Results carries merge-settings (73/73), dogfood-effect (8/8, `suiteSkipReason` did not fire), full suite (806/807), live dry-run smoke, and the P1–P8 parity row table.

### Full-suite single skip is platform-gated — CONFIRMED
The 1 skip is `test_root_walk_windows_separators` (Windows path-separator test, platform-skips on Linux). Pre-existing, environment-conditional, unrelated to vnc-031 — masks no vnc-031 coverage.

## Rework Required

None.

## Scope Concerns

None.
