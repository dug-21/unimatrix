# Agent Report: nxs-012-agent-0b-scope-risk

## Task
Re-assess scope-level risks for nxs-012 after the --skip-quarantined flag moved from import to export.

## Output
- SCOPE-RISK-ASSESSMENT.md replaced at `product/features/nxs-012/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- **High severity**: 3 (SR-02, SR-06, SR-07, SR-08 — SR-06 and SR-08 also high likelihood)
- **Medium severity**: 5 (SR-01, SR-03, SR-04, SR-05, SR-09)
- **Low severity**: 0

## Top 3 Risks for Architect Attention
1. **SR-06** (High/High) — ADR-007 (#4614) is stale. It describes import-side HashSet filtering but scope now requires export-side filtering. Must be corrected before architecture.
2. **SR-08** (High/Med) — Export-side filtering touches 5 table exporters; a missed skip-set check in any one silently produces orphaned rows.
3. **SR-02** (High/Med) — The skip-set query must share the same BEGIN DEFERRED snapshot transaction as all table exports to avoid TOCTOU races.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for lesson-learned failures, risk patterns, outcome rework -- found #4614 (ADR-007 skip-quarantined cascade, now stale), #1166 (nan-002 retro), #4531 (gate 3b file size lesson)
- Stored: nothing novel to store -- the stale ADR finding is feature-specific, not a cross-feature pattern
