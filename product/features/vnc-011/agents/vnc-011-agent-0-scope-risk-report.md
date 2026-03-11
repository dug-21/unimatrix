# Agent Report: vnc-011-agent-0-scope-risk

## Task
Scope-level risk assessment for vnc-011 (Retrospective ReportFormatter).

## Output
- `/workspaces/unimatrix/product/features/vnc-011/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- High severity: 1 (SR-03)
- Medium severity: 3 (SR-01, SR-05, SR-07)
- Low severity: 4 (SR-02, SR-04, SR-06, SR-08)
- Total: 8 risks

## Top 3 Risks for Architect/Spec Writer
1. **SR-03** (High/Med): `evidence_limit` default change from 3 to 0 is a global behavioral change affecting JSON consumers. May need format-dependent defaults.
2. **SR-01** (Med/High): RetrospectiveReport has 15+ fields from 6 features, many Optional. Formatter must handle all None-path combinations gracefully.
3. **SR-07** (Med/Med): col-020b dependency is in-progress. `FeatureKnowledgeReuse` type shape may shift before vnc-011 ships.

## Notes
- Unimatrix was unavailable (embedding model initialization error). Historical intelligence queries skipped.
- SCOPE is well-constrained. Formatter-only approach is sound. Primary risk is the evidence_limit default change being a silent breaking change.
