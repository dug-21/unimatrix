# Risk-Based Test Strategy: vnc-031

Cross-matcher-group stale Unimatrix-hook prune in `mergeSettings` (root-cause fix for #728; install-surface only).

Sources: SCOPE.md, SCOPE-RISK-ASSESSMENT.md (SR-01..SR-07), ARCHITECTURE.md (Step 3c, keep-by-identity), ADR-001/002/003, SPECIFICATION.md (FR-01..FR-16, AC-01..AC-10), `lib/merge-settings.js`, `test/merge-settings.test.js`, `scripts/dogfood-switchover.sh`.
Historical evidence: #4938 (parity on real legacy input, never a pre-narrowed seed), #4932 (negative control must share the positive helper + reconstruct no-prune state via mergeSettings-alone), #4827 (retiring a parity case needs arm-key reconciliation), #4263 (fixture fields silently diverge when not derived from the same input), #4826 (install-surface event-count test sensitivity).

This design has an unusual property: ADR-001's object-identity keep-rule makes the highest-severity *correctness* risk (SR-01 zeroing a managed event) **unrepresentable by construction**. The real residual risk has therefore shifted to two places: (a) the **identity mechanism itself silently degrading to a string/command compare** during implementation — which reintroduces SR-01 invisibly — and (b) the **script-retire parity gate (R-04)**, where a passing `merge-settings.test.js` does not prove the script's legacy cases are subsumed. Test design must attack those two, not just re-assert the happy path.

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Identity keep-rule silently implemented (or later refactored) as a command/string compare, reintroducing SR-01: a fresh entry differing only by whitespace / `LD_LIBRARY_PATH` prefix / arg-order / quoting from a stale entry causes the *wrong* object to be kept or the keep-target to be pruned. | High | Med | Critical |
| R-02 | A managed event ends with **zero** uni hooks (keep-target pruned, or Step 3 appended a new group but `keptEntryByEvent` captured the wrong reference). Fail-loud invariant (FR-03) only protects if a test actually asserts `count !== 0`. | High | Low | High |
| R-03 | Step 3c prunes a uni hook in the **managed** group that is not the kept object (e.g. a leftover dup that Step 3's in-group dedup did not remove, or a second uni entry placed before capture), or fails to prune a uni hook in a non-managed group. | Med | Med | High |
| R-04 | Script-retire (ADR-003) regresses promote/rollback: the source prune does not subsume every case `commandReferencesTarget` handled — and `merge-settings.test.js` passing does NOT prove it, because the proof must run on real legacy-shaped input (#4938), not a pre-narrowed seed. | High | Med | Critical |
| R-05 | Both call arms diverge (SR-05): string arm (`LD_LIBRARY_PATH=<dir> <bin> hook <event>`) and object arm (`node <client> <event>`) do not both produce a clean migration, or only one is exercised on real legacy input. | High | Med | High |
| R-06 | Idempotency breaks on the stale-`"*"`-on-first-run path (SR-07): run 1 (stale present) and run 2 (clean) take different code paths and diverge under `deepStrictEqual`. | Med | Med | High |
| R-07 | Foreign-hook collateral damage (SR-02): a foreign `"*"` PreToolUse hook, a non-`command`-type entry (`type:"url"`), or a uni-**looking**-but-unclassified near-miss is pruned because the prune is keyed on something broader than `isUnimatrixHook`, or a foreign-only group is dropped. | High | Low | High |
| R-08 | Event-key / emptied-group cleanup wrong: a group emptied solely by stale-uni removal is NOT dropped, OR the event key is deleted even though the managed group holds the keep-target (FR-06), OR a foreign-retaining group is dropped. | Med | Med | High |
| R-09 | Partition seam (SR-03): an event is visited by both Step 3c and Step 3b, or by neither — double-pruning a fresh entry or leaving a stale one. SubagentStop opt-out (Step 3b) regresses or its `(opt-out)` action string is perturbed. | Med | Low | Med |
| R-10 | vnc-027 adjacency (SR-06): an over-broad Step 3c prunes the retained cycle-event entry under `PRETOOLUSE_CYCLE_MATCHER`, or perturbs the SubagentStop opt-in matrix. | High | Low | High |
| R-11 | Action-string contract drift (FR-07): the cross-matcher phrase collides with `(opt-out)` / `Updated hook` / `Removed duplicate`, or the `[dry-run]` prefix is lost, breaking init/switchover summaries that grep these. | Low | Med | Med |
| R-12 | Consumer init tests that iterate `HOOK_EVENTS` or assert event/group counts false-fail or, worse, are rubber-stamped to codify an unintended new install shape (#4826). | Med | High | High |
| R-13 | nan-016 harness/runbook assertion flip (AC-09 / #4932) loses its negative control: the clean-state assertion becomes vacuous if the no-prune reconstruction path is removed instead of repointed to mergeSettings-alone. | Med | Med | Med |
| R-14 | Delivery-base dependency gap (OQ-B): crt-052 #706 / vnc-027 #4811 not actually present on the base branch → prune removes telemetry with no live replacement source. | High | Low | Med |
| R-15 | Signature creep (FR-08): the implementation adds a `targetToken`/prune-hint parameter, breaking the OQ-1 no-signature-change guarantee and the back-compat call shapes. | Low | Low | Low |

Priority = Severity × Likelihood (Critical = High×Med with construction-blast-radius or a binding gate; High = High×Low or Med×Med-plus).

---

## Risk-to-Scenario Mapping

### R-01: Identity keep-rule degrades to string/command compare
**Severity**: High **Likelihood**: Med **Impact**: SR-01 reappears invisibly. A future "simplification" to `entry.command === fresh.command`, or an in-loop bug that compares strings, would still pass the happy-path tests but prune the survivor (or keep a stale near-twin) whenever the command shape varies. The dogfood script already hit this exact class (#4931 spaced-path).

**Test Scenarios** (the discriminating tests — these are the ones that would catch a string-compare regression that the AC-tests alone would not):
1. **Stale-twin-differing-only-by-shape**: seed a managed event (e.g. `SessionStart`) with a stale uni hook whose command is byte-identical to the *fresh* command except for collapsible whitespace or arg spacing (`unimatrix  hook   SessionStart` vs the fresh `LD_LIBRARY_PATH=... unimatrix hook SessionStart`), placed in a **non-managed** matcher group. Assert: exactly one uni hook survives, it is the freshly-written managed entry (matcher === `EVENT_MATCHERS[event]`, command === `expectedLocalCommand(event)`), and the near-twin is gone. A string-compare keyed on `command` could keep both or keep the wrong one; identity keeps only the placed object.
2. **PreToolUse `"*"` stale Rust hook + fresh cycle entry whose commands share a long common prefix** — assert the `"*"` uni hook is gone and the survivor is under `PRETOOLUSE_CYCLE_MATCHER` (AC-01).
3. **Mechanism assertion**: a white-box test that the kept survivor is the *same object reference* Step 3 placed is not directly observable from outside; instead assert the structural proxy — after merge, for each managed event there is exactly one uni entry AND its `command === source.commandForEvent(event)` exactly (not `includes`). Pair with scenario 1 so command-shape variance is in play.

**Coverage Requirement**: At least one managed event must be tested with a stale uni hook whose command would *defeat a naive string keep-rule* (whitespace/arg variance) and the survivor must be asserted by both location (managed matcher) and exact fresh command. Mitigated only when a string-compare reimplementation would make a test red.

---

### R-02: Managed event ends with zero uni hooks (fail-loud)
**Severity**: High **Likelihood**: Low **Impact**: hooks stop firing for that event — a silent loss of the whole feature for that event. ADR-001 claims this is impossible by construction; FR-03 demands a test that *fails loud* rather than passing silently.

**Test Scenarios**:
1. Extend `test_each_event_has_exactly_one_unimatrix_entry` to a cross-group seed: every managed event seeded with an extra stale uni hook under a non-managed matcher. Assert `count === 1` **and** add an explicit `assert(count !== 0, "managed event " + event + " dropped to zero uni hooks")` with a distinct failure message (AC-02).
2. Adversarial seed where the *only* uni hook present pre-merge is the stale one under a non-managed matcher (no uni hook in the managed group on input) — Step 3 must create/populate the managed group and Step 3c must not then prune the just-created entry. Assert count === 1 in the managed group.

**Coverage Requirement**: A zero-uni-hook post-state for any managed event is a hard test failure with a self-identifying message, exercised on a seed that has no pre-existing managed-group uni entry.

---

### R-03: Wrong-scope prune inside / outside the managed group
**Severity**: Med **Likelihood**: Med **Impact**: a stale dup survives in the managed group (under-prune) or the kept entry is removed (over-prune). Interacts with Step 3's existing in-group dedup.

**Test Scenarios**:
1. Seed the managed group itself with TWO uni entries (a dup that pre-rename dedup handles) PLUS a stale uni hook in a non-managed group. Assert exactly one uni hook total, in the managed group, with the fresh command — proving Step 3 dedup and Step 3c cross-group prune compose correctly (extends `test_dedup_removes_extra_unimatrix_hooks` to the cross-group case).
2. Seed three matcher groups for one event each holding a uni hook (managed + two stale foreign-matcher groups) — assert both stale groups lose their uni hook and only the managed survivor remains.

**Coverage Requirement**: A seed combining in-managed-group duplication AND cross-group staleness yields exactly one survivor in the managed group.

---

### R-04 (CRITICAL): Script-retire parity not proven on real legacy input
**Severity**: High **Likelihood**: Med **Impact**: retiring `PRUNE_FRAGMENT` regresses promote/rollback even with a green `merge-settings.test.js`. Direct echo of lesson #4938: a tool wrapping a shipped merge primitive must account for state the primitive does not manage, and parity must be proven on **real legacy-shaped input, never a pre-narrowed seed**.

**Enumerated legacy cases the script's `commandReferencesTarget` handled — each MUST be proven subsumed by Step 3c on real input before `PRUNE_FRAGMENT` is deleted (FR-15 / ADR-003 table):**

| # | Legacy-shaped input the script handled | Required Step 3c outcome | Arm |
|---|---|---|---|
| P1 | Stale `"*"` `PreToolUse` Rust uni hook (`LD_LIBRARY_PATH=... unimatrix hook PreToolUse` under `"*"`) | Pruned; fresh entry under `PRETOOLUSE_CYCLE_MATCHER` kept | promote (object) |
| P2 | Stale node-client uni hook from a prior install (`node <other>/hook-client/index.js <event>`) | Pruned | rollback (string) |
| P3 | `.../hook-client/index.js.bak` uni hook (different whole token) | Pruned (uni-owned, not the kept object — unconditional) | both |
| P4 | Old-client-dir uni hook (`.../dogfood-client-OLD/lib/hook-client/index.js <event>`) | Pruned | promote |
| P5 | Rollback dirname-level `LD_LIBRARY_PATH=<dir>` keep — the genuine just-written legacy command | **Kept** (it IS the keep-target by identity; no dirname heuristic) | rollback |
| P6 | Quoted spaced-path target kept (the #4931 bug the tokenizer existed for) | **Kept** by object identity; quoting irrelevant | both |
| P7 | Foreign hook present alongside stale uni hooks | Preserved byte-for-byte | both |
| P8 | A matcher group emptied solely of uni hooks | Group dropped, event key retained | both |

**Test Scenarios**:
1. A `dogfood-switchover.sh`-level (or harness-level) test that runs `promote` on a settings file carrying P1+P3+P4+P7 simultaneously (a realistic legacy live-settings shape), then asserts: zero stale uni hooks off the entrypoint, the fresh node-client entry present, foreign preserved. **On real legacy input, not a seed pre-narrowed to the cycle matcher.**
2. The mirror `rollback` test carrying P2+P3+P7 plus the genuine legacy command (P5/P6) — assert every uni hook is the exact `LD_LIBRARY_PATH=<binDir> <binary> hook <event>` form, node-client uni count == 0, the genuine command kept.
3. Static assertion (AC-09a): `dogfood-switchover.sh` contains no `PRUNE_FRAGMENT` / `pruneStaleUniHooks` / `commandReferencesTarget` / `shellTokens` / `emitAndWrite`, and both arms call `mergeSettings(..., {dryRun})` owning the write.
4. **Ordering gate**: P1–P8 parity demonstrated GREEN before the fragment-deletion commit. Deletion must not precede the parity proof.

**Coverage Requirement**: Every row P1–P8 has a passing assertion on real legacy-shaped input for the correct arm, the bespoke prune is statically gone, and the parity proof precedes deletion. This is the binding retire gate — do not retire on tests that only exercise pre-narrowed seeds.

---

### R-05: Both call arms must produce identical clean migration
**Severity**: High **Likelihood**: Med **Impact**: a clean migration for `init` but not `init --remote` (or vice versa) ships a half-fixed install surface.

**Test Scenarios**:
1. `test_cross_group_migration_string_arm` and `test_cross_group_migration_object_arm`: same legacy `"*"` seed, run each `commandSource` shape (string `BINARY`; object via `buildHookClientCommand`), assert AC-01/AC-02 post-state for each (AC-06).
2. Cross-reference with R-04 P1 (object/promote) and P2 (string/rollback) so each arm is also exercised on the multi-case legacy input, not just a single stale `"*"`.

**Coverage Requirement**: Both arms assert clean single-survivor-per-event on the legacy seed; no per-consumer branch exists in `mergeSettings` (verified by both arms passing the identical assertion).

---

### R-06: Idempotency including stale-`"*"`-on-first-run
**Severity**: Med **Likelihood**: Med **Impact**: a non-idempotent merge churns settings.json on every init, or the first run (stale present) and second (clean) diverge.

**Test Scenarios**:
1. `test_cross_group_migration_idempotent`: seed the legacy `"*"` case, run twice, `deepStrictEqual(first.content, second.content)` (AC-05).
2. Confirm existing `test_merge_idempotent_round_trip` and `test_subagentstop_optout_idempotent` still pass unmodified.
3. Three-run stability on a multi-stale seed (R-04 P-shape): runs 2 and 3 are no-ops (extends `test_three_consecutive_merges_no_growth` to cross-group).

**Coverage Requirement**: `deepStrictEqual` holds across runs for an input that carried stale cross-group uni hooks on run 1; run 2+ emit no cross-matcher removal actions.

---

### R-07: Foreign-hook preservation incl. near-miss and non-command entries
**Severity**: High **Likelihood**: Low **Impact**: a user's hook is deleted — the worst-class regression for an install tool; blast radius now spans all groups (SR-02), not one. Correctness is fully bounded by `isUnimatrixHook` precision (FR-13).

**Test Scenarios**:
1. `test_cross_group_preserves_foreign_star_hook`: seed both a stale uni `"*"` PreToolUse hook AND a foreign `"*"` PreToolUse hook (`my-tool pre-check`); assert the foreign hook survives byte-for-byte in its group, the stale uni hook is gone (AC-03).
2. `test_cross_group_preserves_near_miss_foreign_hook`: a uni-**looking** but unclassified command (e.g. `my-unimatrix-wrapper run`, or a command that contains `unimatrix` but does not match `UNIMATRIX_PATTERNS` prefix anchors) under a non-managed matcher — assert it survives (SR-02, FR-13). This is the discriminating test that a too-loose ownership check would fail.
3. Non-`command` foreign entry (`type:"url"`) in a non-managed group of a managed event — assert preserved (extends `test_hook_entry_without_type_command_is_preserved` to cross-group).
4. A non-managed group holding only foreign hooks after pruning is NOT dropped (ties to R-08).

**Coverage Requirement**: A near-miss uni-looking-but-unclassified hook and a non-command foreign hook both survive in a non-managed group; no foreign hook is mutated or removed under any seed.

---

### R-08: Emptied-group drop / event-key retention
**Severity**: Med **Likelihood**: Med **Impact**: stale empty `{matcher:"*", hooks:[]}` groups accumulate (under-clean) or the event key is wrongly deleted (the managed survivor disappears with it).

**Test Scenarios**:
1. `test_cross_group_drops_emptied_group_keeps_event`: seed a `"*"` group whose only entry is a stale uni hook; assert the `"*"` group is absent post-merge, `content.hooks.PreToolUse` still exists and contains the `PRETOOLUSE_CYCLE_MATCHER` group (AC-04).
2. Foreign-survives-so-group-stays: a `"*"` group with a stale uni hook + a foreign hook — assert the group remains with only the foreign hook (cross-check R-07.4).
3. Reuse the existing `pruneUnimatrixEvent` cleanup shape (NFR-03) — assert no parallel cleanup machinery introduced (a code-shape check: the `eventArray.filter(g => g.hooks.length > 0)` idiom is shared).

**Coverage Requirement**: Emptied-by-uni-removal groups are dropped; event key retained iff the managed survivor exists; foreign-retaining groups never dropped.

---

### R-09: Partition seam (Step 3c managed vs Step 3b opt-out)
**Severity**: Med **Likelihood**: Low **Impact**: double-prune or gap at the `events` / `HOOK_EVENTS \ events` boundary.

**Test Scenarios**:
1. Existing `test_subagentstop_pruned_on_opt_out`, `test_subagentstop_optout_preserves_foreign_hook`, `test_subagentstop_optout_idempotent`, `test_subagentstop_optin_then_optout_round_trip` all pass unmodified (AC-07).
2. Action-phrase disjointness: the new cross-matcher phrase is distinct from `(opt-out)` (ties R-11); a SubagentStop opt-out run emits `(opt-out)` and never the cross-matcher phrase, and a managed-event cross-group run emits the cross-matcher phrase and never `(opt-out)`.
3. Combined seed: SubagentStop stale uni hook (non-registered → Step 3b) AND a stale `"*"` PreToolUse uni hook (registered → Step 3c) in one file — assert both removed, each via its own path's action string, no double emission.

**Coverage Requirement**: Union covers all `HOOK_EVENTS`, intersection empty; opt-out tests unchanged; combined seed exercises both paths in one merge with distinct actions.

---

### R-10: vnc-027 adjacency preserved
**Severity**: High **Likelihood**: Low **Impact**: the retained cycle frame or the SubagentStop matrix breaks — re-opening a shipped decision (#4811).

**Test Scenarios**:
1. Existing `test_pretooluse_matcher_exactly_cycle_tools`, `test_all_other_matchers_unchanged`, and the SubagentStop opt-in matrix tests pass unmodified.
2. After a cross-group migration from a stale `"*"` seed, assert the surviving managed PreToolUse entry is under `PRETOOLUSE_CYCLE_MATCHER` (the retained cycle frame is the keep-target, never pruned) — FR-11.
3. Opt-in path: with `writeOptIn(fp, true)` and a stale cross-group SubagentStop uni hook seeded, assert the SubagentStop survivor is the fresh `"*"` managed entry (opt-in registration + cross-group prune compose).

**Coverage Requirement**: `EVENT_MATCHERS` untouched; cycle frame survives as keep-target; SubagentStop opt-in/opt-out byte-unchanged.

---

### R-11: Action-string contract
**Severity**: Low **Likelihood**: Med **Impact**: init/switchover summaries and any grep on actions misclassify removals.

**Test Scenarios**:
1. `test_cross_group_emits_action_and_dry_run_prefix`: seed legacy `"*"`; non-dry-run asserts an action matching the cross-matcher phrase; dry-run asserts the same action with `[dry-run]` prefix and no file written (AC-08).
2. Assert the cross-matcher phrase is distinct from `(opt-out)`, `Updated hook`, `Added hook`, `Removed duplicate` (substring-disjoint).

**Coverage Requirement**: One action per group that lost ≥1 uni entry; distinct phrase; `[dry-run]` prefix preserved.

---

### R-12: Consumer init-test sensitivity / rubber-stamping
**Severity**: Med **Likelihood**: High **Impact**: install-surface group-set change ripples into `init`/`initRemote` tests that count events/groups (#4826); a false-fail gets silenced by editing the fixture to the new shape without confirming intent.

**Test Scenarios**:
1. Re-run all `init` / `init --remote` consumer tests that iterate `HOOK_EVENTS` or assert event/group counts (AC-10).
2. For any fixture that changes, require an explicit justification that the new group set is an *intended* clean-migration shape (review obligation, not a test) — e.g. a managed event losing a stale `"*"` group is intended; a managed event losing its managed group is a bug.
3. The whole `packages/unimatrix` suite green under `node --test`.

**Coverage Requirement**: No fixture edited to a new shape without a stated intent; full suite green.

---

### R-13: nan-016 harness assertion flip keeps its negative control
**Severity**: Med **Likelihood**: Med **Impact**: the clean-state assertion in `dogfood-effect.test.js` becomes vacuous (always passes), so a future prune regression goes undetected. Direct constraint from #4932.

**Test Scenarios**:
1. The positive clean-state assertion (zero stale uni hooks off the entrypoint; no uni hook under `"*"`) passes with the simplified script (AC-09b).
2. **Negative control preserved**: reconstruct the no-prune post-state by calling the installed `mergeSettings(..., {dryRun:true})` exactly as `run_promote` does and reading `result.content` WITHOUT any prune — that content still carries the stale `"*"` uni hook, and feeding it to the SHARED clean-state assertion helper must `assert.throws` (#4932). Now that mergeSettings itself prunes cross-group, the no-prune reconstruction must call a path that does NOT include Step 3c — the harness change must repoint this control, not delete it.
3. Prove fail-on-break: a temporary neuter of the source prune makes the positive assertion fail while the negative control still passes; restore after.

**Coverage Requirement**: Clean-state assertion non-vacuous — it must fail on an unpruned post-state; the negative control is repointed (not removed) when the script prune is retired.

---

### R-14: Delivery-base dependency present
**Severity**: High **Likelihood**: Low **Impact**: pruning broad-hook telemetry with no live replacement source on the actual base branch (OQ-B).

**Test Scenarios**:
1. Pre-delivery verification (architect/SM gate, not a unit test): confirm crt-052 #706 and vnc-027 #4811 are reachable on the base branch this delivers from (`git merge-base` / log check), not merely "on main as of writing."

**Coverage Requirement**: Documented confirmation both dependencies are on the delivery base before the prune lands.

---

### R-15: Signature stability
**Severity**: Low **Likelihood**: Low **Impact**: a `targetToken`/prune-hint param breaks OQ-1 back-compat.

**Test Scenarios**:
1. Both legacy call shapes — `mergeSettings(fp, binaryString, {dryRun})` and `mergeSettings(fp, {events, commandForEvent}, {dryRun})` — work unchanged (existing arm tests cover this; add no new parameter).

**Coverage Requirement**: No new required parameter; both arms call with the current signature.

---

## Integration Risks

- **Step 3 ↔ Step 3c coupling (R-01, R-03):** Step 3c is correct only because it holds the live `keptEntryByEvent` object reference from the same in-memory pass (ADR-001 "Harder" consequence — cannot be a detached post-processor). The integration risk: a refactor that moves the prune to operate on re-read content, or that captures the reference at the wrong point (before the repoint/push completes), breaks identity. Tested via R-02 scenario 2 (no pre-existing managed entry → capture must follow the create) and R-03 scenario 1.
- **Step 3c ↔ Step 3b ordering (R-09):** 3c runs after 3, before 3b. If reordered, an event mid-rename could be visited by 3b (opt-out, removing ALL uni hooks including the fresh one) instead of 3c. The combined-seed scenario (R-09.3) is the boundary probe.
- **`mergeSettings` ↔ `dogfood-switchover.sh` (R-04, R-13):** the script collapses to a plain `mergeSettings(..., {dryRun})` owning its write. The integration surface is the write-ownership handoff (script no longer post-processes content) and the harness `pruneCount` reporting that disappears. #4827 warns: retiring a parity case requires reconciling the consumer's arm-keys, not just deleting the fragment — the harness's clean-state attribution must move from script-prune to mergeSettings.
- **`mergeSettings` ↔ `init`/`initRemote` (R-12):** no init.js code change, but the install-surface shape consumers assert against shifts (a managed event's group set may shrink). Boundary: consumer tests that count groups.

## Edge Cases

- A managed event whose ONLY uni hook on input is the stale cross-group one (no managed-group uni entry pre-merge) — Step 3 creates the managed entry, Step 3c must not prune it (R-02.2).
- A coincidentally-identical command: a uni hook in a foreign group whose command byte-equals the fresh command — FR-04 prunes it anyway (different object). Assert no "two identical commands under two matchers" end-state.
- Stale uni hook with a `command` that is a non-string or entry that is `null` inside a group's `hooks` array — `isUnimatrixHook` already guards (returns false); assert no throw and the malformed entry is treated as foreign (left untouched).
- Empty `hooks: []` array on a group on input; a group missing the `hooks` key entirely — Step 3c's `Array.isArray(group.hooks)` guard skips it (mirrors `pruneUnimatrixEvent`).
- A managed event with multiple non-managed groups each holding a stale uni hook (R-03.2) — all pruned, one action per group.
- Quoted spaced-path command (`node "/a b/hook-client/index.js" PreToolUse`) as the keep-target — kept by identity, quoting irrelevant (R-04 P6; the #4931 failure mode is gone by construction).

## Security Risks

This is install-surface code that rewrites a user's `.claude/settings.json`. The untrusted input is the **existing settings.json content** (possibly hand-edited or carried from another tool) and the **stale hook commands** within it.

- **Untrusted input:** arbitrary JSON in settings.json; arbitrary command strings in hook entries. `mergeSettings` already throws on malformed JSON and non-object `hooks` (Step 1/2, unchanged) — Step 3c runs only on validated content and is pure in-memory mutation (no new throw path, ARCHITECTURE Error Boundaries).
- **Blast radius of the prune:** the one genuine danger is **deleting a hook the user wanted**. The prune is bounded entirely by `isUnimatrixHook` precision (FR-13). Widening the blast radius from one group to all groups (SR-02) means an over-broad ownership check now deletes user hooks everywhere, not just in the managed group. R-07's near-miss test is the security-relevant assertion: a uni-looking-but-unclassified command must survive. No change to `UNIMATRIX_PATTERNS` (Non-Goal 2) keeps the attack/false-positive surface fixed.
- **No new injection surface:** Step 3c composes no commands and reads no external files; it filters an in-memory array by an existing predicate. The dogfood script's removal of `commandReferencesTarget` (which tokenized command strings) actually *shrinks* the surface — no more shell-token parsing of stale commands. The script continues to pass parameters to node via env only (no string interpolation), unchanged.
- **No path traversal / deserialization risk introduced:** no new file reads, no `eval`, no dynamic require beyond the existing `MERGE_JS` require in the script (unchanged).

## Failure Modes

- **Malformed settings.json:** unchanged — throws with a diagnostic before Step 3c (existing `test_malformed_json_errors_with_diagnostic`).
- **Zero-uni-hook managed event (R-02):** must be a hard, self-identifying test failure (FR-03), never a silent pass. By construction the kept object is never a prune candidate; the test is the regression guard.
- **Idempotency break (R-06):** caught by `deepStrictEqual` across runs; a churning merge is a correctness failure, not a warning.
- **Foreign hook removed (R-07):** must fail a test loudly; there is no acceptable degraded mode — a deleted user hook is a hard regression.
- **Script-retire regression (R-04):** promote/rollback leaving a stale `"*"` group is the exact #4930 symptom that motivated the feature; the harness negative control (R-13) must surface it.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (keep-rule mis-classify → zero uni hooks) | R-01, R-02 | ADR-001 makes string-divergence unrepresentable via object identity; residual risk is implementation degrading to string-compare — R-01 discriminating tests (shape-varying near-twin) + R-02 fail-loud zero assertion guard it. |
| SR-02 (isUnimatrixHook false-positive, blast radius widens to all groups) | R-07 | Prune strictly bounded by unchanged `isUnimatrixHook`; near-miss-foreign-survives test (R-07.2) + non-command foreign preservation (R-07.3) assert the bound. |
| SR-03 (partition seam managed vs opt-out) | R-09 | Step 3c covers `events`, Step 3b covers `HOOK_EVENTS \ events`; combined-seed test exercises both paths with disjoint action strings; ordering (3→3c→3b) probed. |
| SR-04 (script parity not subsumed) | R-04, R-13 | Enumerated P1–P8 case table proven on REAL legacy input (#4938) before fragment deletion; deletion ordering-gated; harness negative control repointed (#4932). |
| SR-05 (cross-consumer blast radius, both arms) | R-05, R-12 | Both string and object arms tested on legacy seed (AC-06); consumer init tests re-run, fixture changes justified not rubber-stamped (#4826). |
| SR-06 (vnc-027 adjacency) | R-10 | `EVENT_MATCHERS` untouched; cycle frame is the keep-target (never pruned); SubagentStop opt-in/opt-out byte-unchanged; existing vnc-027 tests pass unmodified. |
| SR-07 (idempotency, stale-`"*"`-on-run-1) | R-06 | Identity keep-test is consistent across runs; `deepStrictEqual` incl. stale-`"*"`-first-run (AC-05); 3-run stability. |
| — (OQ-B base-branch dependency, SCOPE Assumption 1) | R-14 | Pre-delivery confirmation crt-052 #706 + vnc-027 #4811 on the actual base branch. |

Every SR-01..SR-07 maps to at least one architecture risk with a guarding scenario. R-14 covers the scope Assumption-1 dependency that is not numbered SR-xx.

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-04) | ~7 (shape-varying near-twin keep, PreToolUse stale-`"*"` migration, exact-command survivor; P1–P8 parity on real legacy input across both arms, static fragment-absence, deletion-ordering gate) |
| High | 8 (R-02, R-03, R-05, R-06, R-07, R-08, R-10, R-12) | ~16 (fail-loud zero, in+cross-group dedup, both-arm clean migration, idempotency incl. stale-first-run, foreign + near-miss + non-command preservation, emptied-group drop/event-key retention, cycle-frame + SubagentStop adjacency, consumer-suite re-run) |
| Medium | 4 (R-09, R-11, R-13, R-14) | ~7 (partition combined-seed + opt-out unchanged, action-string distinctness + dry-run prefix, harness negative-control repoint + fail-on-break, base-branch dependency check) |
| Low | 1 (R-15) | 1 (signature stability via existing arm tests) |

---

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_search` for install-surface prune/migration lessons and parity patterns — found #4938 (parity on real legacy input, never a pre-narrowed seed), #4932 (negative control shares positive helper + reconstructs no-prune state via mergeSettings-alone), #4827 (retiring a parity case needs arm-key reconciliation), #4263 (fixture fields diverge when not derived from the same input), #4826 (install-surface event-count test sensitivity). All four directly informed R-04, R-12, R-13.
- Stored: nothing novel to store — the load-bearing patterns (#4938 parity-on-real-input, #4932 negative-control-reconstruction) already exist and are referenced; vnc-031's risks are feature-specific instances of them, not a new cross-feature pattern. A candidate cross-feature pattern ("object-identity keep-rule makes the string-divergence risk unrepresentable but shifts residual risk to mechanism-degradation tests") is observed in only this one feature so far; defer storing until a second feature exhibits it (≥2-feature rule).
