# Agent Report: crt-031-agent-3b-risk

## Summary

Architecture-risk assessment for crt-031 (Category Lifecycle Policy + boosted_categories de-hardcoding). Produced RISK-TEST-STRATEGY.md at feature root.

## Artifacts

- `/workspaces/unimatrix/product/features/crt-031/RISK-TEST-STRATEGY.md` — overwritten with full assessment

## Risk Summary

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 3 | R-01, R-02, R-11 |
| High | 4 | R-04, R-07, R-10, I-04 |
| Medium | 3 | R-03, R-05, R-08 |
| Low | 2 | R-06, R-09 |

## Critical Risks Requiring Human Attention

### R-02 (upgraded from High to Critical) — StatusService::new() has three bypassed construction sites

The architecture document specifies adding `Arc<CategoryAllowlist>` to `StatusService` and wiring through `ServiceLayer::new()`. Source code confirms three additional direct construction sites that the architecture does not enumerate:

1. `background.rs::run_single_tick` line ~446 — constructs `StatusService::new()` directly, bypassing `ServiceLayer` entirely
2. `services/status.rs` test helper ~line 1886 — direct construction
3. `services/status.rs` test helper ~line 2038 — direct construction

Historical entry #3216 documents this exact bypass pattern from dsn-001 where a parameter was silently dropped at `run_single_tick`. The test helpers will produce compile errors (good — caught early), but `run_single_tick` could compile using `CategoryAllowlist::new()` as a default while silently ignoring operator config. The implementation brief must enumerate all four construction sites and require the startup `Arc<CategoryAllowlist>` to be threaded through `background_tick_loop` → `run_single_tick`.

### R-01 (Critical) — validate_config parallel-list default collision

Both `boosted_categories` and `adaptive_categories` default to `["lesson-learned"]` via their serde default functions. Any test fixture constructing `KnowledgeConfig` with a custom `categories` list that does not zero both parallel fields will trigger the wrong `ConfigError` variant. This doubles the existing #2312 trap. AC-24/AC-25 must pass before gate 3b.

### R-11 (Critical) — KnowledgeConfig::default() change causes silent assertion failures

Changing `Default` from `boosted_categories: vec!["lesson-learned"]` to `vec![]` does not cause a compile error. Pattern #3774 documents this failure mode. FR-19 mandates a pre-implementation grep; the implementation brief must make this a blocking pre-step, not optional cleanup.

## Scope Risk Traceability

All nine SR-XX risks traced. SR-07 and SR-08 are fully resolved at architecture level. SR-09 maps to new R-11.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection` — found #3579, #2758. Applied to R-10.
- Queried: `/uni-knowledge-search` for `risk pattern CategoryAllowlist config validation` — found #3770, #3771, #2312. Applied to R-01.
- Queried: `/uni-knowledge-search` for `serde default impl change test breakage` — found #3774, #885, #3773. Applied to R-11 and E-07.
- Queried: `/uni-knowledge-search` for `background tick parameter threading` — found #3216, #2553. Applied to R-02 severity upgrade (High → Critical) and identification of three bypass sites.
- Stored: nothing novel to store — R-02 is an instance of the existing #3216/#2553 pattern. All findings are instances of documented patterns.
