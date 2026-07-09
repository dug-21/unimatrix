# Scope Risk Assessment: vnc-047

> Mode: scope-risk. Tracks GH #940. Historical evidence cited by Unimatrix entry ID.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | **Double schema-version discipline in one feature**: `CURRENT_SCHEMA_VERSION` 30→31 (new `cycle_tags` table) AND `SUMMARY_SCHEMA_VERSION` 5→6 (new `RetrospectiveReport.tags` field). Each needs three-path update + pinned test. This exact miss is the recurring gate failure in this codebase (#4153, #4373). | High | High | Treat as **two independent version cascades**. Spec must enumerate, per bump, all three schema paths + the pinned-version test as discrete acceptance line-items — not one lumped "bump versions" task. |
| SR-02 | **Parallel-feature version collision**: another in-flight feature merging first can claim v31 or SUMMARY v6, forcing retroactive renumber (#4095). | Med | Med | Re-verify both version numbers against HEAD at implementation start, not just at design; flag if either is taken. |
| SR-03 | **Hook-path persistence is the only route; bare MCP `context_cycle` persists nothing.** Tags must ride `build_cycle_event_or_fallthrough` → `RecordEvent` → `handle_cycle_event` exactly as `goal`. Wrong route = tags silently accepted-then-dropped. | High | Med | Architect must forbid a second persistence route; write `cycle_tags` in the same txn as the `cycle_start` insert. No API for "did my tags persist?" — set-and-forget. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | **Payoff (cross-run A/B) is external and deferred** (Non-Goal #4). Feature ships a substrate with no in-product consumer; its value cannot be demonstrated inside Unimatrix. | Med | Low | Acceptance must validate store + surface ONLY; never require cross-run analysis. Shape the `(tag)` index for the deferred query direction so v2 needs no re-migration. |
| SR-05 | **Set-once + `ON CONFLICT DO NOTHING` silently ignores a changed tag set** on a re-issued start. A user re-running start with corrected tags gets no update and no error. | Low | Med | Spec must state "first write wins, later start-tags silently no-op" as intended behavior, and cover it with a test — so it is a decision, not a surprise. |
| SR-06 | **"One tag model" parity drift**: two junctions (`entry_tags`, `cycle_tags`) + a reserved future mutation home on `context_tag` invite divergent semantics over time. | Low | Med | Spec must port the `add_tag`-style primitive from entry tags (re-keyed to `feature_cycle`), not reinvent; cite vnc-045/nxs-008 as the source. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | **Fire-and-forget hook path has a documented silent-failure history**: absent session / NULL `feature_cycle` drops cycle data with no error (#4140, #981). A `cycle_tags` insert on the same path inherits that failure mode. | Med | Med | Architect must confirm tags persist even when the SM session is absent/evicted (parity with the #4136 pre-register fix); a dropped-session case must not silently lose tags. |
| SR-08 | **Structural-only tests pass while the real path is untested** (#3935): a test that inserts into `cycle_tags` directly proves storage but not that the MCP→hook→listener chain actually carries tags. | Med | Med | Coverage must exercise the assembled `context_cycle`-start → surfaced-in-`cycle_review` path end to end, not just the store getter. |
| SR-09 | **GC-protected-table registration is a load-bearing, easy-to-miss step**: unregistered, `cycle_tags` is purged with the cycle (unlike the protected `cycle_events`/`cycle_review_index`). | Med | Med | AC-04 already requires it — flag it explicitly as a step that fails silently if forgotten; regression test must mirror `test_gc_protected_tables_regression`. |
| SR-10 | **Pre-existing cached reviews will never show tags**: SUMMARY staleness is advisory-only, so v5 reviews don't recompute to v6 (#5022). Only runs started after deployment carry tags. | Low | Med | Document explicitly: no back-fill; historical cycles show no tags by design. Set human expectation before it reads as a bug. |

## Assumptions

- **A1** (Proposed Approach, hook path, SCOPE §"Cycle storage — TWO paths"): every persisted `context_cycle` start currently flows through the hook path. If any caller reaches the MCP handler without the hook (e.g. non-hook client), its tags are silently lost — SR-03/SR-07.
- **A2** (AC-03/AC-05, §Constraints "Schema-version discipline"): v31 and SUMMARY v6 are the next free numbers at merge. Invalidated by SR-02.
- **A3** (Goals #3, §Proposed Approach "Durability"): the retention protected-set is the complete GC gate; adding `cycle_tags` there fully protects it. If GC has other purge paths, tags could still be lost — SR-09.
- **A4** (Non-Goal #4): the external analyst can join `cycle_tags`/`summary_json` labels against `cycle_review` metrics out-of-band. If no such external consumer materializes, the feature's value is unrealized — SR-04.

## Design Recommendations

1. **Architect**: model the two version bumps (SR-01) as separate cascades in the ADR; forbid any second tag-persistence route and confirm the absent-session path (SR-03, SR-07).
2. **Spec writer**: make each of the two three-path bumps + its pinned test a discrete AC line-item (SR-01); state set-once-first-wins-silently as intended (SR-05); require end-to-end assembled-path coverage, not structural-only (SR-08); state no-back-fill for historical reviews (SR-10).
3. **Both**: keep acceptance scoped to store + surface; do not let deferred cross-run A/B leak into scope (SR-04); port entry-tag primitives rather than fork a cycle dialect (SR-06).
