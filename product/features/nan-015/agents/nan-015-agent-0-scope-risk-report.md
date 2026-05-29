# Agent Report: nan-015-agent-0-scope-risk

## Task
Scope-level risk assessment for nan-015 (shared model volume for ONNX models).

## Output
- `/workspaces/unimatrix/product/features/nan-015/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
- **High severity**: 3 risks (SR-01, SR-02, SR-06)
- **Medium severity**: 4 risks (SR-03, SR-04, SR-05, SR-07)
- **Low severity**: 1 risk (SR-08)
- **Total**: 8 risks

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-01 (High/Med)**: Cache path precedence chain -- three resolution layers (env var, config field, dirs fallback) must be ordered unambiguously. Existing ADR-004 and ADR-005 document the current chain; the new env var adds a third precedence level.

2. **SR-06 (High/Med)**: Three call sites must all resolve to the shared volume. If any call site bypasses the env var, models silently split across volumes, breaking backup separation.

3. **SR-02 (High/Low)**: Shared writable volume widens the supply-chain attack surface. Lesson #4642 (verify-then-load ordering) becomes more critical when models live on a volume accessible to multiple containers.

## Knowledge Stewardship
- Queried: /uni-knowledge-search for "lesson-learned failures gate rejection" -- found 5 results, none directly applicable to volume/container risks
- Queried: /uni-knowledge-search for "outcome rework" -- found 5 results, none directly applicable
- Queried: /uni-knowledge-search for "risk pattern" -- found 5 results, none directly applicable to this feature
- Queried: /uni-knowledge-search for "Docker container volume" -- found ADR-005 (#4573) and config co-location pattern (#4626), both directly informative
- Queried: /uni-knowledge-search for "ONNX model download cache" -- found ADR-004 (#70) and lesson #4642, both directly informative
- Stored: nothing novel to store -- this is the first volume-separation feature; patterns will emerge if similar work recurs
