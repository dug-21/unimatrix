# Agent Report: crt-031-agent-3-risk

## Output

Produced: `product/features/crt-031/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 1 |
| High | 4 |
| Medium | 4 |
| Low | 2 |

Total risks: 11 (R-01 through R-10, plus I-01 through I-03 integration risks, E-01 through E-06 edge cases).

## Critical Risks Requiring Human Attention

**R-01 — validate_config cross-check collision (Critical)**: Both `boosted_categories` and `adaptive_categories` default to `["lesson-learned"]`. Any `validate_config` test that constructs a partial `KnowledgeConfig` with a custom `categories` list will trigger both parallel-list cross-checks simultaneously. Tests designed to exercise `AdaptiveCategoryNotInAllowlist` will fail with `BoostedCategoryNotInAllowlist` (or vice versa) unless both lists are explicitly zeroed. This is a concrete repeat of the trap documented in entry #2312. The architecture mandated a construction pattern (ARCHITECTURE.md §Test Construction Pattern) and the spec added AC-16 — but the implementer must audit every existing `validate_config` test helper for the `adaptive_categories: vec![]` addition. This must be verified before gate 3b sign-off.

**R-02 — StatusService wiring is open (High)**: Architecture OQ-01 was listed as open at spec time. The spec hedges with "if not, this wiring is added in the same PR" (FR-11). If `StatusService` does not currently hold `Arc<CategoryAllowlist>`, threading it through adds call sites to `StatusService::new()` that are not enumerated in the implementation guidance. Recommend the delivery agent verify `StatusService::new()` before starting implementation — not mid-wave.

**R-07 — merge_configs omission silently drops adaptive_categories (High)**: FR-17 requires `merge_configs` to include `adaptive_categories` in the `KnowledgeConfig` merge block. Omitting this produces no compile error and no runtime error — the operator's configured value is silently replaced by the default. This is the highest-impact silent failure mode in the feature. Requires an explicit merge unit test before gate 3b.

**R-10 — Gate 3b test module omission (High)**: The lifecycle guard stub in `background.rs` and the `category_lifecycle` formatter in `mcp/response/status.rs` produce no behavioral side effects in this feature. Historical pattern #3579 documents exactly this failure mode — wave delivers production code but zero tests for the "low-visibility" modules. The risk coverage report must not be accepted if `background` or `status` module tests are absent.

## Scope Risk Traceability

All six SR-XX risks from `SCOPE-RISK-ASSESSMENT.md` are traced. SR-03 (the critical default-collision trap) maps to R-01 as the highest-priority risk in this document.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found entries #3579, #2758 (elevated R-10 to High severity)
- Queried: `/uni-knowledge-search` for risk pattern RwLock background tick — found entries #1560, #1542 (confirmed two-lock approach follows established patterns)
- Queried: `/uni-knowledge-search` for CategoryAllowlist config validation test fixtures — found entries #2312, #3770 (directly informed R-01 as Critical)
- Stored: nothing novel to store — R-01 is a concrete instance of existing entry #2312; no new cross-feature pattern emerges from this assessment alone
