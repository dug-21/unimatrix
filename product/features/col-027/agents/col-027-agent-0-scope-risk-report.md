# Agent Report: col-027-agent-0-scope-risk

## Output
- Produced: `product/features/col-027/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary
| Severity | Count |
|----------|-------|
| High     | 2 (SR-01, SR-07) |
| Medium   | 4 (SR-02, SR-04, SR-06, SR-08) |
| Low      | 2 (SR-03, SR-05) |

## Top 3 Risks for Architect/Spec Writer Attention

1. **SR-01 (High/High)** — `extract_response_fields()` was written for `tool_response` object shape; the failure payload carries a plain `error` string instead. Silent None or incorrect extraction will cause AC-03 to fail. Needs an explicit named-field extraction path.

2. **SR-07 (High/Med)** — If the new `PostToolUseFailure` event falls through to the wildcard arm in `extract_observation_fields()`, the stored record will have no `tool_name`. Must have an explicit arm before the wildcard; test must assert `tool_name` is populated.

3. **SR-08 (Med/Med)** — `metrics.rs compute_universal()` and `friction.rs PermissionRetriesRule` both independently implement the Pre-Post differential. A partial fix (one file without the other) causes metric/rule divergence. The spec should couple the relevant ACs.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for risk patterns — found entry #2843 (blast-radius pattern from col-023 hooktype coupling), #3446 (lesson-learned: PermissionRetriesRule misattribution), #1268 (lesson-learned: test payloads must match real producer serialization), #3419 (pattern: permission_friction_events is a tool-cancellation proxy), #3471 (pattern: adding a new hook event type pipeline)
- Stored: entry #3472 "Duplicated Pre-Post differential in metrics.rs and friction.rs must be updated atomically" via context_store (pattern) — novel finding visible across col-026 and col-027
