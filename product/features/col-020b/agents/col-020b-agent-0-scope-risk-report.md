# Agent Report: col-020b-agent-0-scope-risk

## Task
Scope-level risk assessment for col-020b (Retrospective Knowledge Metric Fixes).

## Output
- `/workspaces/unimatrix/product/features/col-020b/SCOPE-RISK-ASSESSMENT.md` (37 lines)

## Risk Summary
- **High severity**: 2 (SR-01: serde compat, SR-03: unbounded #193 root cause)
- **Medium severity**: 4 (SR-02, SR-04, SR-07, SR-08)
- **Low severity**: 1 (SR-06)
- **Total**: 8 risks across 3 categories + 4 assumptions

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-03 (High/Med)**: #193 root cause is unresolved and may cross into the Store crate, expanding scope from 2 crates to 3. Recommend time-boxing investigation with a fallback to split the Store fix into a separate issue.

2. **SR-01 (High/Med)**: Serde alias backward compat is unidirectional — aliases only work for deserialization. If any consumer reads col-020b serialized output with col-020 types, renamed fields are silently dropped. Direct precedent: Unimatrix #885 (col-020 gate failure from serde test gaps).

3. **SR-04 (Med/Med)**: Integration test scope is ambiguous — Rust unit tests vs infra-001 Python harness are fundamentally different efforts. Recommend Rust-only for col-020b, defer infra-001 to follow-up.

## Historical Evidence Used
- Unimatrix #885: col-020 lesson-learned — serde types need explicit test coverage
- Unimatrix #729: Pattern — intelligence pipeline needs cross-crate integration tests
- Unimatrix #747: Pattern — cross-crate test infrastructure via feature flags

## Pending
- Attempted to store reusable risk pattern (serde field rename compat testing) but agent lacks Write capability. Pattern should be stored by privileged agent.

## Status
COMPLETE
