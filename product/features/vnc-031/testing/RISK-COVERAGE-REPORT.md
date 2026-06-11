# Risk Coverage Report: vnc-031

Cross-matcher-group stale Unimatrix-hook prune in `mergeSettings` (Step 3c) + dogfood-switchover prune retire + effect-harness attribution repoint. **Install-surface only — pure JS, `node:test`. No Rust / server / MCP.**

Stage 3c execution, branch `feature/vnc-031`, run 2026-06-11.

## infra-001 Smoke Gate — N/A (not skipped, not applicable)

Per the Stage 3a test plan (`test-plan/OVERVIEW.md` §Integration / Harness Plan), the infra-001 pytest harness is **N/A** for this feature, NOT a skipped gate. Rationale (verbatim intent from the plan): infra-001 exercises the compiled `unimatrix-server` Rust binary over MCP JSON-RPC. vnc-031 touches **zero** Rust / server / MCP surface (NFR-04, Non-Goal 5) — it is a pure in-memory JS transform of `.claude/settings.json` plus the existing single write. No infra-001 suite (`protocol`, `tools`, `lifecycle`, `volume`, `security`, `confidence`, `contradiction`, `edge_cases`) maps to this change, and the smoke gate validates server behavior unrelated to the install surface. The feature's integration surface is entirely the `packages/unimatrix` JS package, validated by that package's own `node:test` runner plus the two script-level harnesses (`merge-settings.test.js`, `dogfood-effect.test.js`). The smoke gate does not apply.

## Test Results

### Unit — `merge-settings.test.js` (Step 3c)
- Command: `node --test packages/unimatrix/test/merge-settings.test.js`
- tests **73** · pass **73** · fail **0** · skipped **0**

### Effect harness — `dogfood-effect.test.js` (GATE B + GATE C parity)
- Command: `node --test packages/unimatrix/test/dogfood-effect.test.js`
- tests **8** · pass **8** · fail **0** · skipped **0**
- `suiteSkipReason` did **NOT** fire — the real client was staged (npm/tar available), so the GATE C real-legacy-input parity proof genuinely ran. No loud SKIP; GATE C is satisfied on real input, not a pre-narrowed seed.

### Full package suite (AC-10) — `packages/unimatrix`
- Command: `cd packages/unimatrix && node --test`  (canonical auto-discovery, == `npm test`)
- tests **807** · pass **806** · fail **0** · skipped **1**
- The 1 skip is `test_root_walk_windows_separators` — a Windows path-separator test that platform-skips on Linux. Pre-existing, environment-conditional, unrelated to vnc-031; masks no vnc-031 coverage.
- **Invocation note:** `node --test packages/unimatrix/test/` (trailing-slash dir form) fails on Node v24 with `MODULE_NOT_FOUND` — Node 24 resolves a trailing-slash path as a module to load, not a test directory to glob. This is a runner-invocation artifact, NOT a feature regression. The canonical form (`node --test` from the package dir, the package's own `test` script) discovers `test/` correctly and is GREEN.

### Live dry-run smoke — `scripts/dogfood-switchover.sh`
Scratch settings seeded with a legacy `"*"` PreToolUse Rust uni hook + a foreign `Bash` hook; staged client; both arms `--dry-run`:
- `promote --dry-run`: exit **0**; emits `[dry-run] Removed stale unimatrix hook: PreToolUse (cross-matcher migration)`
- `rollback --dry-run`: exit **0**; emits the same cross-matcher action
- settings.json **byte-unchanged** (sha256 identical before/after) — dry-run writes nothing.

## Delivery Gates

### GATE A (R-14) — delivery-base dependency — PASS
- crt-052 **#706** (transcript-fed cycle-review distillation): present on `main` — commit `ae9dbb53`.
- vnc-027 matcher-narrowing (ADR-004 #4811 is a Unimatrix ADR entry ID, not a commit ref): vnc-027 fully merged on `main` (#680, e.g. `0d9bc9a9` design, `62c44258` size-gate, retro artifacts). `EVENT_MATCHERS`/`PRETOOLUSE_CYCLE_MATCHER` source present.
- `git merge-base HEAD main` = `379d6ec3`. Both replacement-observation sources reachable on the delivery base before the prune landed.

### GATE B (R-13) — harness negative control repointed, not deleted — PASS
- `dogfood-effect.test.js` T1d preserved and repointed: it reconstructs the **no-Step-3c** post-state via `unprunedPromoteContent` (NOT routed through current `mergeSettings`, which would now return a clean state and make the control vacuous), sanity-asserts the stale hook survives (`staleNoPrune.length >= 1`), then asserts the SHARED `assertCleanPromoteState` helper **`assert.throws`** on that unpruned state.
- Non-vacuous: a regression to a no-op Step 3c reproduces exactly this state and turns the positive T1 red. Negative control fires (`assert.throws` at line 606). GREEN, not skipped.

### GATE C (R-04, binding) — P1–P8 parity on REAL legacy input before fragment deletion — PASS
- Ordering: parity-proof commit `7bf45fbe` (dogfood-effect GATE C) **precedes** fragment-deletion commit `a4ac286b` (dogfood-switchover retire). `git merge-base --is-ancestor 7bf45fbe a4ac286b` = true → parity-proof ≤ deletion. Ordering gate held.
- AC-09a static absence: `grep -E 'PRUNE_FRAGMENT|pruneStaleUniHooks|commandReferencesTarget|shellTokens|emitAndWrite' scripts/dogfood-switchover.sh` → **no match** (PASS). Both arms (`run_promote` object / `run_rollback` string) call `mergeSettings(..., {dryRun})` owning the write; no bespoke prune remains.
- Parity ran on real legacy-shaped input in the harness (T-PARITY + T1 + T2), `suiteSkipReason` did not fire.

#### P1–P8 parity row mapping
| # | Legacy input | Outcome | Arm | Asserting test | Result |
|---|---|---|---|---|---|
| P1 | `"*"` Rust PreToolUse uni hook | pruned; fresh under `PRETOOLUSE_CYCLE_MATCHER` | promote | T1 `assertCleanPromoteState` + T-PARITY | PASS |
| P2 | stale node-client uni hook | pruned | rollback | T2 (node-client count 0) | PASS |
| P3 | `.../index.js.bak` uni hook | pruned (unconditional) | both | T1 + T2 (no `.bak` survivor) | PASS |
| P4 | old-client-dir uni hook (`dogfood-client-OLD/...`) | pruned | promote | T1 + T-PARITY | PASS |
| P5 | genuine `LD_LIBRARY_PATH=<dir> <rust> hook <e>` | **kept** by identity | rollback | T2 (exact Rust form) | PASS |
| P6 | quoted spaced-path target (#4931) | **kept**; quoting irrelevant | both | unit `test_cross_group_quoted_spaced_path_target_kept` (load-bearing identity proof; spaced install dir not realizable in `os.tmpdir()` harness — OVERVIEW OQ-2) | PASS |
| P7 | foreign hook alongside stale uni hooks | preserved byte-for-byte | both | T1 + T2 `foreignPresent` | PASS |
| P8 | group emptied solely of uni hooks | dropped; event key kept | both | T1 + T2 (no empty group, key present) | PASS |

All 8 rows GREEN for the correct arm on real legacy input.

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Identity keep-rule degrades to string/command compare | `test_cross_group_stale_twin_differing_only_by_shape_pruned`, `_pretooluse_star_shares_prefix_with_cycle_survivor`, `_survivor_is_exact_fresh_command_not_substring`, `_quoted_spaced_path_target_kept` | PASS | Full |
| R-02 | Managed event ends with zero uni hooks (fail-loud) | `test_each_event_has_exactly_one_unimatrix_entry(_cross_group)`, `_only_stale_on_input_managed_entry_created_then_kept` | PASS | Full |
| R-03 | Wrong-scope prune in/out managed group | `test_cross_group_in_group_dup_plus_cross_group_stale`, `_multiple_stale_groups_all_pruned` | PASS | Full |
| R-04 | Script-retire parity not proven on real legacy input | GATE C: T-PARITY + T1 + T2 (P1–P8) on real input; AC-09a grep; ordering gate `7bf45fbe ≤ a4ac286b` | PASS | Full |
| R-05 | Both call arms diverge | `test_cross_group_migration_string_arm`, `_object_arm`; T1 (object/promote) + T2 (string/rollback) on real input | PASS | Full |
| R-06 | Idempotency incl. stale-`"*"`-on-run-1 | `test_cross_group_migration_idempotent`, `_three_run_stability`; T2 second-rollback byte-identical | PASS | Full |
| R-07 | Foreign + near-miss + non-command collateral damage | `test_cross_group_preserves_foreign_star_hook`, `_preserves_near_miss_foreign_hook`, `_preserves_non_command_entry` | PASS | Full |
| R-08 | Emptied-group drop / event-key retention | `test_cross_group_drops_emptied_group_keeps_event`, `_foreign_retaining_group_not_dropped` | PASS | Full |
| R-09 | Partition seam (3c managed vs 3b opt-out) | `test_partition_combined_subagentstop_optout_and_pretooluse_cross_group`; opt-out tests unmodified | PASS | Full |
| R-10 | vnc-027 adjacency | `test_pretooluse_matcher_exactly_cycle_tools`, `_all_other_matchers_unchanged`, `test_cross_group_pretooluse_survivor_under_cycle_matcher`, `_subagentstop_optin_composes` | PASS | Full |
| R-11 | Action-string contract drift | `test_cross_group_emits_action_and_dry_run_prefix` (phrase + `[dry-run]` prefix + substring-disjoint) | PASS | Full |
| R-12 | Consumer init-test sensitivity / rubber-stamping | Full package suite 806/807 pass; no fixture edited to a new shape (see AC-10 note) | PASS | Full |
| R-13 | Harness negative control kept (GATE B) | T1d `assert.throws` on repointed no-Step-3c post-state; non-vacuous | PASS | Full |
| R-14 | Delivery-base dependency present (GATE A) | #706 on `main` (`ae9dbb53`); vnc-027 merged (#680) | PASS | Full |
| R-15 | Signature creep | `test_cross_group_migration_string_arm` + `_object_arm` both call current signature; no new param | PASS | Full |

No risk is uncovered.

## Gaps

None. Every risk R-01..R-15 maps to at least one passing test or a satisfied delivery gate. No loud SKIP masked any coverage: the effect harness ran with the real client staged (no `suiteSkipReason`), so GATE C real-input parity is genuinely proven. The single full-suite skip (`test_root_walk_windows_separators`) is a Linux-platform Windows-path test unrelated to this feature.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_legacy_star_pretooluse_migrates_clean` |
| AC-02 | PASS | `test_each_event_has_exactly_one_unimatrix_entry_cross_group` incl. `assert(count !== 0)`; `_only_stale_on_input_managed_entry_created_then_kept` |
| AC-03 | PASS | `test_cross_group_preserves_foreign_star_hook` + `_preserves_near_miss_foreign_hook` + `_preserves_non_command_entry` |
| AC-04 | PASS | `test_cross_group_drops_emptied_group_keeps_event` |
| AC-05 | PASS | `test_cross_group_migration_idempotent`; `test_merge_idempotent_round_trip` + `test_subagentstop_optout_idempotent` unmodified |
| AC-06 | PASS | `test_cross_group_migration_string_arm` + `_object_arm` (both arms, same legacy seed) |
| AC-07 | PASS | `test_subagentstop_pruned_on_opt_out`, `_optout_preserves_foreign_hook`, `_optout_idempotent` unmodified; cross-matcher phrase substring-disjoint from `(opt-out)` |
| AC-08 | PASS | `test_cross_group_emits_action_and_dry_run_prefix`; live dry-run smoke emits `[dry-run] Removed stale unimatrix hook: PreToolUse (cross-matcher migration)`, no file written |
| AC-09 | PASS | (a) grep fragment-absent; (b) T1 clean-state + T1d negative control (`assert.throws`); (c) GATE C P1–P8 parity GREEN on real input before deletion (`7bf45fbe ≤ a4ac286b`) |
| AC-10 | PASS | Full `packages/unimatrix` suite 806 pass / 0 fail / 1 unrelated Windows-skip. No init/init-remote consumer fixture changed to a new event/group shape — install-surface event count stable at 8 (live dry-run confirms 8 managed events). No rubber-stamped shape change. |

### AC-10 init/init-remote consumer note
The init / init-remote consumer tests asserting event/group shape pass unmodified — `mergeSettings` Step 3c is additive (prunes stale cross-group uni hooks for the SAME registered event set; managed-group output shape unchanged). The live dry-run shows the stable 8-event managed install surface. No fixture was changed to a new shape, so there is **no** install-surface shape change to flag as intended.

## GH Issues Filed

None. No integration test failed; no pre-existing failure required an `xfail` or a GH Issue. The only non-pass results are (1) the unrelated Linux/Windows platform skip and (2) the Node-24 trailing-slash runner-invocation artifact — neither is a feature failure.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` — surfaced vnc-031 ADR-001/003 (#4939, #4941) and parity/negative-control lessons (#4938, #4932 referenced in the Risk Strategy); confirmed approach, no new test procedure surfaced.
- Stored: nothing novel to store — the load-bearing test patterns (parity-on-real-legacy-input #4938, negative-control-reconstruction #4932, install-surface event-count sensitivity #4826) already exist and were applied directly. The Node-24 trailing-slash `node --test` invocation artifact is a generic runner gotcha, not a vnc-031-specific reusable procedure; defer storing unless it recurs across features (≥2-feature rule).
