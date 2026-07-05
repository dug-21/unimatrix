# Scope Risk Assessment: crt-058

Feature makes `context_deprecate` eagerly `DELETE` agent-authored (`source='agent'`) graph edges touching the deprecated entry, both directions, synchronously and non-fatally, pulling the EveryTick compaction's blanket delete forward for one entry. Load-bearing invariant: **eager ⊆ tick**.

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | The eager op is an **irreversible DELETE of hand-declared graph relationships at the source event** — no soft-delete, no undo, no successor repoint (unlike `context_correct`). An over-broad or mis-keyed predicate destroys agent-authored edges permanently and earlier than the tick would. | High | Low-Med | Architect: predicate MUST be the exact `(source_id=? OR target_id=?) AND source='agent'` and can never exceed the tick's set. Consider auditing deleted edge tuples (not just count) for reconstructability. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-02 | **eager ⊆ tick is asserted only in prose.** If the eager predicate ever widens beyond the tick's status predicate — or the tick later narrows (gains a source filter / repoints agent edges) — the subset breaks → ghost records + divergence. Direct precedent: bugfix-458 (#3910), bugfix-879 (#5417). | High | Med | Spec: make eager ⊆ tick a **testable** invariant. Seed successor-bearing entries the tick would repoint and prove the eager path never touches them. Re-verify if the tick predicate ever changes. |
| SR-03 | "Agent-authored set = exactly `source='agent'`" is a point-in-time enumeration. #4167: **inclusive single-source filters silently undercount when new `EDGE_SOURCE_*` constants ship.** A future human/other agent-authored provenance would be missed by the eager delete. | Low-Med | Low | Spec: document that eager completeness is provenance-enumeration-bound. Inclusive `='agent'` is acceptable ONLY because the miss is subset-safe (tick backstop catches it). Add a test: one edge per source value, assert which are eagerly removed. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | **Response-surface change threads `edges_removed` through `format_status_change` across 3 formats (Summary/Markdown/Json) + audit** — a multi-site lockstep. #5427: string/call-count tests are blind to argument threading; one format can drop the count and ship green. | Med | Med | Spec: require a **behavioral per-format matrix** asserting the count is surfaced (and omitted at zero) in each of the 3 formats, plus audit-record content — not a call-count assertion. |
| SR-05 | **Correctness depends on the EveryTick compaction remaining the backstop.** A swallowed non-fatal eager-delete failure only stays safe because the tick still sweeps. If compaction is later changed/removed, that failure becomes silent permanent dangling-edge retention. | Med | Low | Spec: record compaction-as-backstop as a standing dependency invariant; flag it for re-check on any future compaction change. Ensure the failure `warn`-log is not classified as an expected-suppressed error (#3448 fire-and-forget log discipline). |
| SR-06 | **Ordering / placement.** Delete keys on the (now non-Active) entry id, so it must run after the status flip AND past the step-5 idempotency early-return; step-7 `confidence.recompute` also fires. Wrong order causes redundant re-deprecation deletes or a no-match delete. | Med | Low | Architect: pin insertion site (after flip, after step-5 guard); confirm no interaction with step-7 recompute ordering (AC-07, AC-09). |

## Assumptions

- **Provenance enumeration (SCOPE Background, Goal 1):** `graph_edges.source` takes exactly the enumerated values and every agent/human-directed write binds `EDGE_SOURCE_AGENT`. If false → over- or under-deletion (SR-03).
- **Tick predicate stability (SCOPE Constraints, "eager ⊆ tick"):** assumes the tick stays status-only and blanket over all sources. If the tick gains a source filter or keeps agent edges, the subset can invert (SR-02).
- **Chokepoint exclusivity (SCOPE Constraints, "Chokepoint-only"):** assumes `context_correct` never reaches `deprecate_with_audit` without a successor. If any correction path performs a bare flip, inbound edges that should be repointed are instead deleted (widens SR-01 blast radius).

## Design Recommendations

1. Treat **eager ⊆ tick** (SR-02) as the primary architectural constraint — every design choice must preserve the eager set as a strict subset of the tick set. Cite #3910 and #5417.
2. Prefer auditing removed-edge identity, not just a count (SR-01), so a wrongful eager delete is diagnosable/reconstructable.
3. Spec must root SR-02, SR-03, SR-04 tests in behavior/state, not call- or string-counts (SR-04 / #5427).
4. Carry SR-05 forward as a standing coupling note so a future compaction change re-checks the backstop guarantee.

## Knowledge Stewardship
- Queried: context_search for eager/tick divergence, non-fatal failure, edge-delete provenance, and risk patterns — found #3910, #5417, #4167, #5427, #5431, #3448 directly on point.
- Stored: nothing novel — the recurring risk (multi-pass same-table filter divergence) is already captured as pattern #3910 and lesson #5417; no cross-feature pattern beyond those to add.
