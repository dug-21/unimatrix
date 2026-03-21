# Agent Report: col-023-agent-0-scope-risk

## Output

- Produced: `/workspaces/unimatrix/product/features/col-023/SCOPE-RISK-ASSESSMENT.md`
- Line count: 38 (under 100 limit)
- Risks identified: 8 (SR-01 through SR-08)

## Risk Summary

| Severity | Count |
|----------|-------|
| High | 4 (SR-01, SR-02, SR-04, SR-07) |
| Med | 4 (SR-03, SR-05, SR-06, SR-08) |
| Low | 0 |

## Top 3 Risks for Architect Attention

1. **SR-01 (High/High)** — The `json_pointer` DSL cannot express temporal window rules. The scope commits to both threshold and temporal window rule types but constrains the implementation to a primitive that only handles one of them. The architect must define an evaluator struct or narrow the DSL to threshold-only before design proceeds.

2. **SR-02 (High/Med)** — Two live representations of `MetricVector.universal` (typed `UniversalMetrics` struct + `HashMap<String, f64>`) create a serialization contract that is not explicitly managed. The architect must pick one canonical form before the storage layer is specified.

3. **SR-07 (High/Med)** — Removing the `_ => continue` skip means detection rules will receive events from unregistered domains. Rules that do not guard on `source_domain` will silently produce false findings. The lesson from entry #699 (pipeline integrity breakage via silent pass-through) applies directly — the spec must mandate `source_domain` guards as a rule authoring constraint.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection"` — found entry #699 (silent data orphaning on hook pipeline, directly applicable to SR-07), entry #723 (architecture/spec cross-validation, applicable to SR-05)
- Queried: `/uni-knowledge-search` for `"outcome rework"` — no directly applicable rework outcomes found
- Queried: `/uni-knowledge-search` for `"risk pattern"` — found entry #377 (wave-based refactoring with compilation gates, applicable to SR-04), entry #2843 (HookType blast radius, directly applicable to SR-04)
- Queried: `/uni-knowledge-search` for observation pipeline and generalization patterns — found entry #882 (best-effort optional computation) and entry #383 (ObservationSource trait independence)
- Stored: entry #2902 "Generalization features consistently underestimate DSL expressiveness gap between declared rule types and chosen implementation primitive" via `/uni-store-pattern` — novel pattern visible in col-023 scope, predictable recurrence in any future feature with no-new-dependencies + expressiveness requirements
