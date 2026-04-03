# Scope Risk Assessment: crt-042

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `get_embedding()` is O(N) per call (entry #3658). Up to 200 expanded entries = 200 × O(N) scans (~1.4M comparisons at 7k corpus). Deferral to measurement is appropriate only if the feature flag enforces no-default-on until latency is measured and a hard ceiling is set. | High | High | Architect must wire wall-clock debug instrumentation in Phase 0 and define a latency ceiling as a post-measurement gate condition before `ppr_expander_enabled` can default to `true`. |
| SR-02 | BFS frontier sorted by node-ID creates a deterministic budget-boundary bias toward lower (older) entry IDs when `max_expansion_candidates` is hit early. Relevant cross-category entries with higher IDs may be silently excluded. | Med | Med | Document the bias explicitly; consider a post-measurement follow-up to sort by edge weight once the expander proves its value. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | S1/S2 (Informs) edges may be single-direction only from crt-041 (source_id < target_id convention). Outgoing-only traversal would see only half the S1/S2 graph, silently halving the expander's effective graph density for those edge types. The scope classifies this as a "delivery prerequisite check" — that classification underestimates the risk: if single-direction, the fix requires a crt-041 write-site change or a back-fill migration, not just a code check. | High | Med | Upgrade to a blocking prerequisite: delivery agent must confirm S1/S2 edge directionality against crt-041 source before writing any Phase 0 code. If single-direction, file a separate issue to back-fill bidirectional edges (same pattern as CoAccess back-fill, entry #3889) before crt-042 ships. |
| SR-04 | Phase 0 + Phase 5 combined ceiling (250 entries beyond k=20) interacts with the existing `ppr_max_expand=50` cap in an unspecified way. Scope says "no conflict" but does not constrain whether Phase 5 still injects up to 50 more on top of 200 Phase 0 entries. Result set size post-expansion could reach 270 before PPR scoring truncates. | Med | Low | Architect must document the combined ceiling explicitly in the implementation and add an AC verifying the maximum post-expansion pool size. |
| SR-05 | Eval gate requires MRR >= 0.2856 AND P@5 > 0.1115. If the expander is correct but the graph is too sparse (crt-041 edges insufficient) or S1/S2 is single-direction (SR-03), the eval gate will fail — leaving the feature shipped but gated-off indefinitely with no actionable owner. | Med | Med | The eval gate failure scenario needs an explicit owner and resolution path in the delivery brief (not just "measure and decide"). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Direction semantics ambiguity (lesson #3754): specs for graph traversal have historically used conceptual "Incoming" language to describe what is implemented as `Direction::Outgoing` in reverse PPR. The scope correctly specifies Outgoing-only, but spec/architecture documents that describe the traversal conceptually may reproduce the same ambiguity, causing reviewer confusion or future direction-flip bugs. | Med | High | Architect and spec writer must express traversal contracts behaviorally ("entry A surfaces when HNSW seed B exists and edge A→B exists") rather than solely by Direction enum value. Cite entry #3754 and #3750 as precedent. |
| SR-07 | Quarantine check for expanded entries is the caller's responsibility in `search.rs`, not inside `graph_expand()`. If future callers of `graph_expand` bypass the quarantine check, quarantined entries silently enter the result pool. | Med | Low | Spec writer should add a contract annotation on `graph_expand` documenting that callers are responsible for quarantine filtering, and add an AC verifying no quarantined entry appears in results even when it is graph-reachable. |

## Assumptions

- **SCOPE.md §Proposed Approach / Phase 0**: Assumes `vector_store.get_embedding()` is the only way to compute cosine similarity for expanded entries, and that None means "skip." If a stored entry has a valid embedding not returned by this path (layer-0 miss bug — entry #1712), it will be silently excluded from expansion. The crt-014 bugfix (entry #1724) addressed this for the tick path; the search path should be verified to use the same corrected iterator.
- **SCOPE.md §Design Decisions Q1**: Assumes crt-041 S8 (CoAccess) edges are already bidirectional per the crt-035 back-fill pattern (entry #3889). This must be confirmed at delivery time, not assumed.
- **SCOPE.md §Constraints 6**: Defers latency measurement to post-ship A/B eval. This is acceptable only because the feature flag defaults to `false`. If the flag is flipped without latency data, the assumption that "bounded well within max=200" is sufficient becomes untested.

## Design Recommendations

- **SR-01**: Wire `debug!` timing instrumentation for Phase 0 in the architecture spec. Gate condition confirmed: P95 latency addition ≤ 50ms over pre-crt-042 baseline (measure baseline in same eval run; gate is a delta, not an absolute P95). Do not default the flag to `true` until measured.
- **SR-03**: Make the S1/S2 directionality check a hard pre-implementation gate in the delivery brief, not a post-hoc check. The fix path (back-fill migration) has significant scope implications.
- **SR-06**: Spec writer must write all traversal ACs in behavioral form (what surfaces, not what Direction enum is used), citing lesson #3754.
