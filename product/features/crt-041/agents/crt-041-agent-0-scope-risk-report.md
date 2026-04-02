# Agent Report: crt-041-agent-0-scope-risk

## Output
- Produced: `product/features/crt-041/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
| Severity | Count |
|----------|-------|
| High     | 3 (SR-01, SR-04, SR-07) |
| Medium   | 3 (SR-02, SR-05, SR-06) |
| Low      | 3 (SR-03, SR-08, SR-09) |

## Top 3 Risks for Architect/Spec Writer Attention
1. **SR-07 (High/High)** — InferenceConfig dual-maintenance trap. crt-040 and crt-041 both add fields. If either feature leaves `impl Default` out of sync with serde backing fns, defaults silently diverge. Must be caught by spec (entries #3817, #4014).
2. **SR-01 (High/Med)** — S2 vocabulary terms used in dynamically constructed SQL. If any term contains `'`, `%`, or `\`, the query is either broken or injectable. Space-padded instr() pattern is safe only when the term is a sqlx bound parameter, not string-interpolated.
3. **SR-04 (High/Med)** — crt-040 prerequisite `write_graph_edge(source: &str, ...)` may not exist at crt-041 delivery start. No spec-level gate currently enforces verification. Undetected absence widens scope mid-sprint.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found entries #3579, #2758, #1203 (gate process failures, not directly applicable to crt-041 scope risks)
- Queried: `/uni-knowledge-search` for "outcome rework background tick implementation" — found entries #1542, #3822, #1616, #3723, #3827 (tick patterns, informed SR-02 and SR-08)
- Queried: `/uni-knowledge-search` for "risk pattern SQL injection dynamic query" — no directly applicable pattern found
- Queried: `/uni-knowledge-search` for "watermark counter crash safety write ordering" — found entry #4026 (S8 watermark pattern, confirmed SR-03 is low severity — idempotency covers it)
- Queried: `/uni-knowledge-search` for "InferenceConfig impl Default serde default mismatch trap" — found entries #3817, #4013, #4014 (directly informed SR-07)
- Retrieved full content: entries #4026, #3826, #3675, #4011, #4014, #3817, #3981
- Stored: nothing novel to store — the S2 dynamic-SQL injection risk is a first occurrence on this feature; one data point does not establish a cross-feature pattern. Will revisit after delivery retrospective if the pattern recurs.
