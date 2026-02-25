# Scope Risk Assessment: crt-003

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Exhaustive match breakage: adding `Quarantined = 3` to `Status` enum will cause compile errors in all match arms across 5 crates | Med | High | Architect should catalog every match site before designing; treat this as a cross-crate refactor, not a local change |
| SR-02 | Conflict heuristic false positive rate unknown: no NLI model means rule-based detection, which may flag complementary entries as contradictions | Med | Med | Architect should design the heuristic output as a scored signal (not binary), enabling threshold tuning without code changes |
| SR-03 | HNSW does not expose stored embeddings: retrieving an entry's embedding for re-search or consistency check may require reading raw data points from hnsw_rs internals or maintaining a separate embedding store | High | Med | Architect must verify hnsw_rs API can return embeddings by data_id; if not, consider storing embeddings in a new redb table |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope creep into automated quarantine: the temptation to auto-quarantine flagged contradictions is strong but creates a DoS vector | High | Med | Spec writer should make the manual-quarantine boundary explicit in acceptance criteria and add a non-goal test scenario |
| SR-05 | `context_status` latency growth: contradiction scanning defaults to ON, adding O(n log n) HNSW queries to every status call | Med | Med | Architect should define latency budget and design early-exit when embed service is not ready |
| SR-06 | Quarantine vs. Deprecated semantics overlap: both exclude entries from retrieval, risking user confusion about which to use when | Low | Med | Spec writer should document clear semantic distinction in tool descriptions and response messages |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | crt-002 confidence formula: `base_score()` has exhaustive match on Status; adding Quarantined requires a design decision about quarantined entries' base score | Med | High | Architect should decide quarantined base_score (likely lower than deprecated 0.2, or same) |
| SR-08 | QueryFilter default status: currently defaults to Active when no status provided; must ensure Quarantined entries are excluded without breaking callers who expect current behavior | Med | High | Architect should verify all QueryFilter usage sites and ensure backward compatibility |
| SR-09 | VECTOR_MAP and HNSW contain quarantined entries: quarantined entries remain in the vector index, so search returns them before status filtering removes them; wasted search budget | Low | High | Accept this: removing from HNSW would require rebuild. Status filter post-search is the correct approach |

## Assumptions

1. **hnsw_rs exposes stored embeddings** (referenced in Proposed Approach, embedding consistency checks). If hnsw_rs does not provide an API to retrieve a data point's embedding by data_id, the embedding consistency check requires a separate storage mechanism. This is the highest-risk assumption (SR-03).

2. **Status enum u8 serialization is backward-compatible** (referenced in Constraints). Adding variant 3 does not affect deserialization of existing entries with values 0-2. This should be verified with a targeted test.

3. **Conflict heuristic is sufficient without NLI** (referenced in Non-Goals). The heuristic will miss subtle contradictions. The tunable threshold mitigates false positives but does not improve recall. Accepted trade-off: zero additional model dependencies.

## Design Recommendations

1. **(SR-01, SR-07, SR-08)** Map every exhaustive match on `Status` across all crates before implementation. The architect should produce a list in the architecture doc.
2. **(SR-03)** Investigate hnsw_rs embedding retrieval API early. If unavailable, the architecture must include an alternative (e.g., store embeddings in a new EMBEDDINGS table in redb, or re-embed from text for every check).
3. **(SR-02)** Design the conflict heuristic as a pluggable component with a scored output, not a hardcoded boolean. This allows future replacement with an NLI model without architecture changes.
