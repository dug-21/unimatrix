# Gate 3a Report: vnc-031

> Gate: 3a (Component Design Review)
> Date: 2026-06-10
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment (ADR-001/002/003) | PASS | Identity keep-rule by `Object.is`/`!==` reference (not command compare); managed/opt-out partition explicit; script-retire gated on P1–P8 parity. Pseudocode mirrors ARCHITECTURE Step 3c verbatim and is grounded in real source lines. |
| Specification coverage (FR-01..FR-16) | PASS | All 16 FRs traced to pseudocode + test-plan; no scope additions; NFR-01 surface containment respected. |
| Risk coverage (R-01..R-15) | PASS | Both Critical risks (R-01 identity, R-04 parity) have discriminating/real-input scenarios; AC-02 fail-loud and GATE B negative-control covered. |
| Interface consistency | PASS | `mergeSettings` signature unchanged; `isUnimatrixHook` unchanged; action-string substring-disjoint from `(opt-out)`; OVERVIEW shared types match per-component usage. |
| Knowledge stewardship compliance | PASS | SPECIFICATION + RISK-TEST-STRATEGY both carry `## Knowledge Stewardship` with `Queried:` and a reasoned `nothing novel to store -- ...` (>=2-feature rule). |

## Detailed Findings

### 1. Architecture alignment

**Status**: PASS

**ADR-001 (keep-target by object identity, NOT command-string compare).**
The pseudocode (`merge-settings-step3c.md`) implements the keep test as
`hook => NOT (isUnimatrixHook(hook) AND hook !== kept)` — strict reference
inequality against `keptEntryByEvent[event]`, the object Step 3 placed. It adds an
explicit ANTI-PATTERN block forbidding any `hook.command === ...`, `.includes`, or
tokenizing form. The capture line `keptEntryByEvent[event] = newHookEntry` is
placed at the end of the per-event body so it follows the repoint/push in all
three Step-3 branches. Verified against source: `newHookEntry` (merge-settings.js:262)
is the same object assigned at :302 (repoint), pushed at :305 (append), and wrapped
at :316 (new group) — so a single branch-independent capture is correct.

**ADR-002 (Step 3c managed-events-only with explicit managed/opt-out partition).**
OVERVIEW §Event Partition states Step 3c visits exactly `events` (managed) and
Step 3b visits exactly `HOOK_EVENTS \ events`; union = all, intersection = ∅;
ordering 3 → 3c → 3b is called load-bearing with the reorder failure mode named
(R-09). Matches source: Step 3 (:258-322), Step 3b (:324-333). The per-component
table contrasts 3c vs `pruneUnimatrixEvent` correctly — notably "Event-key delete:
NO" for 3c vs "yes" for 3b, matching the source `delete content.hooks[event]` at
:185-187 that 3c deliberately omits.

**ADR-003 (script-retire gated on parity).**
`dogfood-switchover-retire.md` carries a binding ⛔ GATE C: the `PRUNE_FRAGMENT`
deletion commit may not precede a GREEN P1–P8 proof on REAL legacy input, and a
green `merge-settings.test.js` explicitly does not satisfy it. The post-retire
`run_promote`/`run_rollback` collapse to `mergeSettings(..., {dryRun})` owning the
write, matching `initRemote`. The P5 rollback dirname special-case and P6 quoted
spaced-path tokenizer are correctly retired in favor of identity keep.

### 2. Specification coverage (FR-01..FR-16)

**Status**: PASS

Every FR maps to pseudocode + a test:
- FR-01/04 cross-group unconditional prune outside managed group — Step 3c filter.
- FR-02/03 identity keep, exactly-one, never-zero — keep test + AC-02 `assert(count !== 0)`.
- FR-05/13 foreign + near-miss + non-command preserved — AC-03 trio of tests.
- FR-06 emptied-group drop / event-key retain — `eventArray.filter(...)` without `delete`.
- FR-07/action contract — substring-disjoint phrase + `[dry-run]` prefix (AC-08).
- FR-08/15 signature unchanged — no `targetToken`; both arm shapes call current signature.
- FR-09 both arms — AC-06 string + object arm tests.
- FR-10/11 opt-out + vnc-027 adjacency unchanged — existing tests run unmodified.
- FR-12 idempotency incl. stale-`"*"`-on-run-1 — AC-05 + 3-run stability.
- FR-14/15 script retire gated on parity — GATE C.
- FR-16 harness assertion flip with negative-control preserved — GATE B.

No scope additions. NFR-01 surface containment (four file areas) is respected;
no `lib/init.js` change. The OVERVIEW correctly notes the nan-016 RUNBOOK carries
no surviving-`"*"` assertion (grep-confirmed in the harness plan), bounding FR-16.

### 3. Risk coverage — both Critical risks

**Status**: PASS

**R-01 (Critical — identity degrading to command-string compare).**
The discriminating tests exist and would fail a naive `command ===` reimplementation:
- `test_cross_group_stale_twin_differing_only_by_shape_pruned` — stale near-twin
  differing only by collapsible whitespace/arg spacing in a non-managed group;
  asserts the survivor is the placed object by matcher + **exact** fresh command.
  A `command === fresh` keep-rule keeps the wrong object or both → red.
- `test_cross_group_survivor_is_exact_fresh_command_not_substring` — exact-equality
  (not `includes`) structural proxy for identity.
- `test_cross_group_quoted_spaced_path_target_kept` — #4931 quoted-spaced-path kept
  by identity (unit-level).
The test plan states the coverage requirement that R-01 is mitigated only when a
string/`includes` reimplementation turns >=1 test red.

**R-04 (Critical — script-retire parity on REAL legacy input before deletion).**
GATE C is binding and ordered: P1–P8 proven GREEN on a seed carrying a genuine
`"*"` Rust `PreToolUse` hook plus `.bak`/old-client-dir hooks (not a pre-narrowed
seed, #4938), demonstrated inside the real `dogfood-effect.test.js` harness, BEFORE
the fragment-deletion commit. The coverage report is required to record both commit
SHAs and confirm parity-proof <= deletion. Each P-row maps to a named harness
assertion. P6 is justifiably split to unit-level (spaced install dir not realizable
in `os.tmpdir()`), with the rationale recorded as an open question for the implementer.

**AC-02 fail-loud non-zero invariant.** Covered: extended
`test_each_event_has_exactly_one_unimatrix_entry` asserts `count === 1` per event
plus an explicit `assert(count !== 0, "managed event " + event + " dropped to zero
uni hooks")`, and the adversarial `test_cross_group_only_stale_on_input_managed_entry_created_then_kept`
exercises the no-pre-existing-managed-entry seed (guards a capture-before-create bug).

**GATE B negative-control preservation.** Covered and correctly diagnosed: the plan
identifies that once Step 3c ships, the old `noPrunePromoteContent` (which relied on
`mergeSettings` NOT pruning) goes vacuous, and repoints the reconstruction to a
no-Step-3c path (managed-group repoint only, stale `"*"` re-injected) fed to the
SHARED `assertCleanPromoteState` helper, preserving `assert.throws` (#4932). A
fail-on-break drill is specified as a delivery-time check.

### 4. Interface consistency

**Status**: PASS

- `mergeSettings(filePath, commandSource, options)` signature unchanged across all
  files; no `targetToken`/prune-hint (FR-08/R-15). Confirmed against source :203.
- `isUnimatrixHook` unchanged, sole ownership signal; its `null`/non-string-command
  guard (:122) is relied on for the malformed-entry edge case.
- Shared types in OVERVIEW (`keptEntryByEvent`, `newHookEntry`, matcher group,
  HookEntry) match per-component usage; `keptEntryByEvent` is function-local.
- Action-string contract substring-disjoint: new `Removed stale unimatrix hook:
  <event> (cross-matcher migration)` vs opt-out `Removed unimatrix hook: <event>
  (opt-out)` — "stale" insertion means the opt-out prefix is not a substring of the
  new phrase, and `(opt-out)` is not a substring of `(cross-matcher migration)`.
  AC-08/AC-07 assert disjointness explicitly. No contradictions across component files.

### 5. Knowledge stewardship compliance

**Status**: PASS

Design-phase source documents carry the required block:
- SPECIFICATION.md §Knowledge Stewardship — `Queried:` context_briefing (read-only tier, no storage).
- RISK-TEST-STRATEGY.md §Knowledge Stewardship — `Queried:` context_search (found #4938/#4932/#4827/#4263/#4826) and a reasoned `Stored: nothing novel to store -- ...` invoking the >=2-feature rule for the candidate object-identity pattern.
Both reasons are present and substantive (not a bare "nothing novel"). No WARN.

## Rework Required

None.

## Notes for downstream gates (3b/3c, not blocking)

- **GATE A (R-14)** delivery-base dependency check (crt-052 #706, vnc-027 #4811 on
  the actual base branch via `git merge-base`) is a manual pre-prune gate the
  coverage report must record. Not a 3a artifact concern.
- **GATE C ordering** — Gate 3b/3c must verify the parity-proof commit precedes the
  `PRUNE_FRAGMENT` deletion commit (two SHAs in RISK-COVERAGE-REPORT).
- **Harness SKIP** — `dogfood-effect.test.js` self-skips loudly if npm/tar cannot
  stage the temp install; the plan correctly states a SKIP is not a PASS and must be
  recorded as a gap, and GATE C's real-input proof is unsatisfied under a skip.
- **Open questions** carried forward to 3b: `seedWithCrossGroupStale` per-arm fresh
  command derivation; P6 unit-vs-harness split acceptance. Both are implementer
  confirmations, not design gaps.
