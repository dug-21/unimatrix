# Agent Report: crt-047-agent-3-risk

## Role
Architecture Risk Strategist (MODE: architecture-risk)

## Output
`/workspaces/unimatrix/product/features/crt-047/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 2 |
| High | 5 |
| Medium | 5 |
| Low | 2 |
| **Total** | **14** |

## Top Risks Requiring Human Attention

**R-01 (Critical)**: ADR-003 and SPECIFICATION directly contradict each other on the orphan attribution query. ADR-003 final resolution is ENTRIES-only (no AUDIT_LOG join). The SPECIFICATION (FR-05, FR-06, AC-04, NFR-03, Domain Models) specifies an AUDIT_LOG join throughout. The implementor cannot satisfy both. This must be resolved in the IMPLEMENTATION-BRIEF before pseudocode begins — it affects the SQL design for both `orphan_deprecations` and `deprecations_total`.

**R-02 (Critical)**: ADR-001 introduces a `first_computed_at` column and requires `ORDER BY first_computed_at DESC WHERE first_computed_at > 0` as the baseline ordering key. SPECIFICATION FR-10 still says `ORDER BY feature_cycle DESC LIMIT N`. The spec was not updated to absorb ADR-001. This is a direct contradiction that will cause the implementor to produce either the wrong ordering (following spec) or an undocumented extra column (following ADR-001). Column count in SPECIFICATION FR-08 (five columns) and ADR-004 (seven columns — five snapshots + `corrections_system` + `first_computed_at`) also diverges.

**R-04 (High)**: `corrections_total` definition conflict between SPEC (includes `corrections_system`) and ADR-002 (excludes `corrections_system`, total = intentional curation only). Affects stored column design and σ baseline signal quality.

## Open Questions Inherited from Specification

- **OQ-SPEC-01**: Should AUDIT_LOG orphan query filter `outcome = Success`? (Depends on R-01 resolution — if ENTRIES-only path wins, this is vacuous.)
- **OQ-SPEC-02**: `corrections_system` stored column disposition — ADR-002 says include it, FR-08 omits it.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for risk patterns — entries #4092, #3894 (migration pattern), #4076 (gate-3b test omission) informed R-03, R-07, R-10.
- Stored: nothing novel to store — risks are feature-specific. Cross-feature pattern candidate (spec not absorbing ADR decisions before implementation) deferred until delivery confirms the recurrence.
