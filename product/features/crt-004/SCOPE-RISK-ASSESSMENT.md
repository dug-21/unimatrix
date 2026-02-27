# Scope Risk Assessment: crt-004

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Quadratic pair generation: k results produce k*(k-1)/2 pairs per tool call; unbounded k creates write amplification | Med | High | Architect must enforce a hard cap on result set size for co-access pair generation (proposed 10 = 45 pairs max) |
| SR-02 | Confidence weight redistribution: changing 6 weights that sum to 1.0 affects every existing entry's confidence score; regression risk across all confidence-dependent behavior | High | High | Architect should define exact new weight distribution and verify boundary entries (min/max confidence) do not flip ranking under the new weights |
| SR-03 | Co-access feedback loop: boosted entries get retrieved more, generating more co-access signal, getting boosted further; runaway amplification | High | Med | Architect should design boost cap and diminishing returns curve to prevent unbounded amplification |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Confidence function pointer signature mismatch: `compute_confidence` takes `(&EntryRecord, u64) -> f32` but co-access affinity needs CO_ACCESS table data; extending the signature is a breaking API change | Med | High | Architect must decide how co-access affinity integrates: extend the function signature, pre-lookup co-access data before calling, or apply co-access factor separately at query time |
| SR-05 | Scope of "direct briefing boosting" is ambiguous: SCOPE says very small influence but does not define the mechanism (reorder briefing entries? add co-accessed entries to briefing?) | Med | Med | Spec writer should define exactly what "direct co-access boosting in briefing" means operationally |
| SR-06 | Staleness cleanup in context_status could add latency: scanning all CO_ACCESS pairs for staleness is O(total_pairs) on every status call | Low | Med | Architect should consider lazy staleness (filter on read) vs. eager cleanup (delete on status call) |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | CO_ACCESS table must be opened in Store::open alongside 11 existing tables; table initialization order and error handling must match existing patterns | Low | High | Follow exact same pattern as FEATURE_ENTRIES (12th table becomes 13th) |
| SR-08 | crt-003 quarantined entries may appear in co-access pairs; boosting a quarantined entry's partners could surface results influenced by quarantined knowledge | Med | Med | Architect should filter quarantined entries from co-access lookups, not just from search results |
| SR-09 | Search re-ranking modified: crt-002's `rerank_score` is used in both `context_search` and response formatting; co-access boost must integrate without breaking the existing blend | Med | Med | Architect should keep co-access as a separate post-rerank step, not modify the existing rerank_score function |

## Assumptions

1. **redb supports (u64, u64) composite keys efficiently** (Proposed Approach). This key type is already used by TIME_INDEX: `(u64, u64) -> ()`. Verified.
2. **Co-access pair volume stays manageable** (Scale Considerations). At 10K active pairs with 28 bytes each = 280KB. If pair generation is uncapped, pathological usage could produce millions of pairs. Depends on SR-01 cap enforcement.
3. **Existing confidence tests are comprehensive enough to detect weight regression** (SR-02). The crt-002 test suite includes boundary tests and reference values; weight changes will cause test failures that serve as regression guards.

## Design Recommendations

1. **(SR-02)** Define the exact seven-weight distribution in the architecture document. Run existing confidence boundary tests mentally against the new weights to verify no catastrophic shifts.
2. **(SR-03)** Design co-access boost with a hard cap (e.g., max boost = 0.05) and log-transform on co-access count, matching the anti-gaming pattern from crt-002's usage_score.
3. **(SR-04)** The cleanest integration path: apply co-access factor separately at query time, not inside compute_confidence. This avoids breaking the function pointer signature and keeps co-access as a server-layer concern (relational), not a store-layer concern (per-entry).
4. **(SR-08)** Filter co-access lookups to exclude pairs where either entry is quarantined or deprecated.
