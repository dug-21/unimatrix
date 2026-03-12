# Agent Report: nan-004-agent-0-scope-risk

## Task
Scope-level risk assessment for nan-004 (Versioning & Packaging).

## Output
- `/workspaces/unimatrix/product/features/nan-004/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- **High severity**: 3 (SR-01, SR-03, SR-08, SR-09)
- **Medium severity**: 4 (SR-02, SR-05, SR-06, SR-10)
- **Low severity**: 1 (SR-07)
- **Total**: 10 risks identified

## Top 3 Risks for Architect Attention

1. **SR-09 (High/High)**: Hook PATH resolution — shell hooks run outside npm context where `node_modules/.bin/` is not on PATH. The resolved approach (bare `unimatrix` name) needs prototype validation before the architecture commits to it. If this assumption is wrong, hook wiring strategy must change.

2. **SR-08 (High/Med)**: settings.json merge complexity — merging nested JSON with arrays (hooks, matchers, permissions) is error-prone. Historical lesson #367 confirms two-phase approaches reduce rework. Specification must define exact merge semantics with edge cases.

3. **SR-01 (High/Med)**: ONNX runtime pinned to RC — shared library bundling per-platform is the primary cross-compilation challenge. Native linux-x64 build defers this, but the architecture must isolate ONNX packaging to avoid blocking future platform expansion.

## Knowledge Stewardship
- Queried: /knowledge-search for "lesson-learned failures gate rejection" -- found outcome entries and gate handling procedures, no directly applicable failure lessons for packaging
- Queried: /knowledge-search for "outcome rework packaging distribution npm" -- found two-phase migration lesson (#367) relevant to merge strategy risk
- Queried: /knowledge-search for "risk pattern cross-compilation native dependencies" -- found cfg-gated patterns (#345) and risk traceability pattern (#174), no packaging-specific patterns
- Queried: /knowledge-search for "schema migration version upgrade" -- found ADR-005 scan-and-rewrite migration decisions (#75, #379), confirms migration maturity but gaps untested
- Stored: nothing novel to store -- first packaging/distribution feature, no cross-feature pattern visible yet
