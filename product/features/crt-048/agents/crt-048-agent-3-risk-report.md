# Agent Report: crt-048-agent-3-risk

**Role**: Architecture Risk Strategist
**Mode**: architecture-risk
**Feature**: crt-048 — Drop Lambda freshness dimension

## Deliverable

`product/features/crt-048/RISK-TEST-STRATEGY.md` — written.

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 4 |
| High | 3 |
| Medium | 3 |
| Low | 1 |
| **Total** | **10** |

## Top Risks Requiring Test Coverage

**R-01 / R-06 (Critical)** — `compute_lambda()` positional argument mis-ordering at either of the two call sites in `services/status.rs`. All remaining parameters are `f64` — the compiler cannot detect transposition. Mitigation: tests with three distinct dimension values where any transposition produces a detectably different result. Grep to confirm both call sites updated.

**R-02 (Critical)** — Partial `StatusReport` struct field removal in `mcp/response/mod.rs`. Eight fixture sites, 16 field references. The `make_coherence_status_report()` helper at line 1434 carries non-default values (0.8200 / 15) and will be missed by any search-and-replace targeting default values. Mitigation: build gate plus post-delivery grep for removed field names.

**R-03 (Critical)** — `DEFAULT_STALENESS_THRESHOLD_SECS` deleted by an implementer following Goal 7 literally. ADR-002 and FR-10/AC-11 encode the retention constraint, but this risk is only mitigated if the implementer reads the implementation notes rather than just the goals list. Mitigation: AC-11 grep check as a delivery gate step.

## Key Design Decisions Reflected

- Architecture correctly declined the named-struct refactor for `compute_lambda()` (SR-02). Risk mitigated through tests, not API redesign.
- `DEFAULT_STALENESS_THRESHOLD_SECS` retention encodes the highest scope risk (SR-03) into both ADR-002 and AC-11 — the risk is now a named, testable acceptance criterion.
- All 8 `mcp/response/mod.rs` fixture sites enumerated exactly (SR-06 resolved in architecture), removing the estimation uncertainty from the scope.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for risk patterns — entry #2398 (API Extension Gap) and #3829 (weight delta pattern) found and applied to risk severity ratings
- Queried: `/uni-knowledge-search` for StatusReport serialization history — entry #325 confirmed known breaking-change surface
- Stored: nothing novel to store — all relevant patterns already exist in Unimatrix; this feature's risks are feature-specific
