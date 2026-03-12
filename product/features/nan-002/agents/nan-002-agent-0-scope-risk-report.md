# Agent Report: nan-002-agent-0-scope-risk

## Task
Scope-level risk assessment for nan-002 (Knowledge Import).

## Output
- `/workspaces/unimatrix/product/features/nan-002/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- **High severity**: 3 (SR-01, SR-04, SR-08)
- **Medium severity**: 5 (SR-02, SR-03, SR-05, SR-07, SR-09)
- **Low severity**: 1 (SR-06)
- **Total**: 9 risks identified

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-08** (High/Med): Export/import format contract is implicit code, not a shared definition. Format drift between nan-001 and nan-002 will cause silent failures. Recommend shared format types.
2. **SR-01** (High/Med): ONNX model network dependency makes import fail in air-gapped environments. Recommend pre-flight model availability check and clear error messaging.
3. **SR-04** (High/Med): `--force` is a destructive one-shot with no undo. Recommend double-opt-in or confirmation for non-empty databases.

## Knowledge Stewardship
- Queried: /knowledge-search for lesson-learned failures -- found #885 (serde test coverage) directly applicable to SR-09
- Queried: /knowledge-search for risk patterns -- no patterns directly applicable to import/restore scenarios
- Queried: /knowledge-search for direct SQL bypass -- found #336 (ADR-004) and #344 (pattern) directly informing SR-02
- Queried: /knowledge-search for ONNX model dependency -- found #82 (lazy init ADR) and #69 (hf-hub ADR) informing SR-01
- Stored: nothing novel to store -- risks are feature-specific, no cross-feature pattern visible yet (nan-002 is the first import feature)
