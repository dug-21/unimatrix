# Agent Report: crt-031-agent-0-scope-risk

## Output

- `product/features/crt-031/SCOPE-RISK-ASSESSMENT.md` — written (34 lines)

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 1 (SR-03) |
| Medium | 3 (SR-01, SR-02, SR-05) |
| Low | 2 (SR-04, SR-06) |

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-03 (High/High)** — `adaptive_categories` and `boosted_categories` share the same default (`["lesson-learned"]`). Existing test fixtures that zero one but not the other will fail `validate_config` with a misleading error. The spec must enumerate all partial `KnowledgeConfig` constructions and require every parallel list to be explicitly nulled in empty-categories test cases. Historical evidence: entry #2312 (dsn-001).

2. **SR-01 (Med/High)** — `categories.rs` is currently 453 lines. Adding the second `RwLock` field, `is_adaptive`, updated constructors, and tests will almost certainly breach the 500-line file-size rule. Module split must be planned before speccing, not as a follow-up.

3. **SR-05 (Med/Med)** — Every `CategoryAllowlist::new()` and `from_categories()` call site (server.rs default field init, both main.rs paths, test helpers) must be traced and updated. A call site left on the legacy constructor will silently carry only the built-in default policy, making the operator's `config.toml` `adaptive_categories` setting invisible at runtime.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection", "outcome rework config validation cortical", "risk pattern", "CategoryAllowlist config validation KnowledgeConfig", "KnowledgeConfig parallel list boosted_categories structural pattern" — found entries #2312, #3770, #3715, #3721 directly relevant; used to elevate SR-03 to High and confirm SR-01 line-count risk.
- Stored: entry #3771 "KnowledgeConfig parallel list defaults collide in validate_config test fixtures — zero ALL parallel lists together" via `/uni-store-pattern` — cross-feature pattern visible across dsn-001 and crt-031; will recur on any future third parallel list.
