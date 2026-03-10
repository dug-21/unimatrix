# col-020b Test Strategy Overview

## Test Approach

All testing is Rust unit tests within existing `#[cfg(test)] mod tests` blocks (ADR-002). No infra-001 integration tests are added in this feature. Tests use realistic MCP-prefixed tool names (`mcp__unimatrix__context_*`) to prevent regression of the original #192 bug.

Three test locations, matching the three source files with logic changes:
1. `session_metrics.rs` -- normalize, classify, counter tests
2. `types.rs` -- serde backward compat tests
3. `knowledge_reuse.rs` -- semantic revision tests

## Risk-to-Test Mapping

| Risk | Priority | Component Test Plan | Test Count |
|------|----------|-------------------|------------|
| R-01 (normalize edge cases) | High | tool-name-normalizer.md | 8 |
| R-02 (serde alias drops fields) | Med | type-renames.md | 6 |
| R-03 (serde default incorrect zero) | Med | type-renames.md | 3 |
| R-04 (delivery_count miscount) | High | knowledge-reuse-semantics.md | 5 |
| R-05 (by_category wrong set) | Med | knowledge-reuse-semantics.md | 4 |
| R-06 (#193 data flow) | Critical | data-flow-debugging.md | code review |
| R-07 (re-export compile) | Low | re-export-update.md | compile gate |
| R-08 (MCP-prefix gap) | Critical | tool-classification.md, knowledge-curated-counter.md | 7 |
| R-09 (curate mapping error) | Low | tool-classification.md | 2 |
| R-10 (inconsistent normalization) | High | knowledge-curated-counter.md | 2 |
| R-11 (curate key breaks consumers) | Low | knowledge-curated-counter.md | 3 |
| R-12 (spawn_blocking swallow) | Med | data-flow-debugging.md | code review |
| R-13 (new field names in output) | Low | type-renames.md | 2 |

## Cross-Component Test Dependencies

- C1 (normalizer) is tested standalone, then indirectly through C2 (classify) and C3 (counters).
- C4 (type renames) must compile before C5 (knowledge reuse semantics) tests can reference `FeatureKnowledgeReuse`.
- C7 (re-export) is validated by compilation of the full workspace.

## Integration Harness Plan

### ADR-002 Decision

Per ADR-002, infra-001 integration tests are deferred to a follow-up. All col-020b testing uses Rust unit tests with synthetic data.

### Existing Suite Coverage

The `context_retrospective` tool is exercised by the `tools` suite (test_retrospective_* tests). These tests validate that the MCP interface returns a well-formed report but do not seed MCP-prefixed tool names into observations, so they cannot verify the normalization fix end-to-end.

### Suite Selection for Stage 3c (Smoke Gate Only)

| Suite | Relevance | Run? |
|-------|-----------|------|
| smoke | Minimum gate for any change | Yes (mandatory) |
| tools | col-020b changes tool computation; smoke subset covers retrospective | Via smoke |
| protocol | No protocol changes | No |
| lifecycle | No storage/lifecycle changes | No |
| volume | No scale changes | No |
| security | No security changes | No |
| confidence | No confidence changes | No |
| contradiction | No contradiction changes | No |
| edge_cases | No edge case boundary changes | No |

### New Integration Tests Needed (Deferred)

A follow-up should add to `suites/test_tools.py`:
- `test_retrospective_mcp_prefixed_knowledge_counters` -- seed observations with `mcp__unimatrix__context_search` tool names, call `context_retrospective`, assert `knowledge_served > 0`.
- `test_retrospective_knowledge_curated_counter` -- seed `mcp__unimatrix__context_correct` observations, assert `knowledge_curated > 0` in session summary.
- `test_retrospective_feature_knowledge_reuse_single_session` -- seed query_log for one session, assert `delivery_count > 0`.

These validate the full stack (Store -> observe -> server -> MCP JSON) which unit tests cannot cover. Filing as a follow-up keeps col-020b scope bounded.
