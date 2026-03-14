# Agent Report: crt-019-agent-3-risk

## Output

Produced: `product/features/crt-019/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical (P1) | 1 |
| High (P2) | 9 |
| Medium (P3) | 5 |
| Low (P4) | 2 |
| **Total** | **17** |

## Top Risks for Human Attention

1. **R-01 (Critical/High)**: `compute_confidence` bare function pointer must become a capturing closure. This is the highest architectural risk — if `record_usage_with_confidence` is not updated to accept `Box<dyn Fn(...) + Send>`, the empirical Bayesian prior never flows through to stored confidence values. The feature ships, compiles, and passes all unit tests, yet the core behavioral change is inactive. Requires an integration test that verifies the empirical prior is used in stored confidence, not just that the function is callable.

2. **R-05 (High/High)**: Threshold discrepancy between SPEC (≥5) and ARCHITECTURE (≥10) for cold-start prior activation. This is an explicit unresolved contradiction. The architecture doc raised the threshold with documented stability rationale; the spec was not updated to match. The implementation team must resolve this before writing code — the risk document designates ADR-002's ≥10 as authoritative, but the spec writer should confirm.

3. **R-11 (High/High)**: The store's `record_usage_with_confidence` may internally deduplicate entry IDs before issuing `UPDATE` statements. ADR-004 explicitly flagged this as unverified. The doubled-access mechanism (flat_map repeat approach) silently fails if deduplication happens at the store layer. A store-layer unit test with duplicate IDs in the input list must be written and pass before the implementation is merged.

4. **R-04 (High/High)**: T-REG-02 must be updated before weight constants are changed, not after. With 7 coordinated changes, this ordering is easy to violate under time pressure. Specification C-02 mandates this; the tester should verify this constraint is met in the PR diff order.

5. **R-02 (High/Med)**: All 4 `rerank_score` call sites in search.rs must pass `confidence_weight`. Removal of `SEARCH_SIMILARITY_WEIGHT` as a compiled constant forces a compile error at any forgotten site — but if the constant is accidentally left in as a local binding or dead code, one site can silently use 0.85 while others use the adaptive weight.

## Integration Test Scope Recommendation

Five integration tests should be treated as non-negotiable for this feature:

1. Empirical prior flows through `UsageService` closure to stored confidence (R-01)
2. Store layer allows duplicate IDs in access list without deduplication (R-11)
3. `context_lookup` dedup-before-multiply: second call by same agent produces 0 not 2 (R-07)
4. Search result ordering shifts when `confidence_weight` changes from 0.15 to 0.25 (R-02)
5. Prior-activation boundary: 9 voted entries → cold-start; 10 voted entries → empirical (R-05)

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned/failure/gate-rejection patterns — MCP tools unavailable in this agent context; historical intelligence sourced directly from SCOPE-RISK-ASSESSMENT.md Unimatrix entry references (#199, #202, #705, #706, #735, #771) and ARCHITECTURE.md open-question notes.
- Stored: nothing novel to store — the risk patterns identified (closure vs. function pointer, store dedup assumption, spec/arch discrepancy on thresholds) are specific to crt-019's architecture rather than broadly recurring patterns across features. The spawn_blocking pool saturation pattern is already documented in Unimatrix (#771) and referenced in SR-08.
