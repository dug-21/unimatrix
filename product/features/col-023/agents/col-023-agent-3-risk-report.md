# Agent Report: col-023-agent-3-risk

## Output

- Produced: `product/features/col-023/RISK-TEST-STRATEGY.md`
- Mode: architecture-risk

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 4 |
| High     | 5 |
| Medium   | 4 |
| Low      | 1 |
| **Total**| **14** |

## Critical Risks Flagged for Human Attention

**R-04 (Spec/Architecture Conflict — gate blocker)**: SPECIFICATION.md FR-06 retains Admin runtime domain pack override as a functional requirement (including AC-08 and Workflow 3). ADR-002 explicitly removes it from W1-5 scope. OQ-01 in the spec is still open. This is a hard conflict that must be resolved before implementation begins. Either FR-06/AC-08 are removed from the spec, or a target MCP tool is named and its schema delta is defined.

**R-01 (Cross-domain false findings)**: All 21 rewritten detection rules must include the `source_domain` guard preamble as the first operation in `detect()`. This is the highest-likelihood critical risk — the guard is easy to omit and the failure is silent (phantom findings in production reports). ADR-005 mandates it; the gate-3a checklist must enforce it. Informed by lesson #699 (silent data orphaning in hook pipeline).

**R-02 (Backward compatibility regression)**: A snapshot test capturing `RetrospectiveReport` output for a fixed Claude Code session fixture before and after the refactor is required. Without it, string-comparison errors in any of the 21 rewritten rules are invisible until production.

**R-03 (Test fixture gap post-Wave 4)**: Integration test fixtures that constructed `ObservationRecord` with `hook: HookType` must supply both `event_type` AND a non-empty `source_domain` after the Wave 4 update. A fixture supplying `source_domain: ""` compiles and runs, but all rules silently produce no findings — a false green. Static grep verification is required as part of the Wave 4 gate.

## Open Questions Affecting Risk Assessment

- OQ-01: Which existing MCP tool is extended for FR-06? Unresolved at architecture time. Determines whether R-04 is a test risk or a removal action.
- FM-05: Does the SQLite version in use support `ALTER TABLE ADD COLUMN IF NOT EXISTS`? If not, the migration idempotency (R-05/FM-05) requires an explicit column-existence check before applying the migration.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found #699 (silent data orphaning via hardcoded None in hook pipeline; directly elevated R-01 to Critical/High), #2758 (gate-3c non-negotiable test validation; shaped coverage summary), #1203 (cascading rework from incomplete gate validation; elevated R-03 severity)
- Queried: `/uni-knowledge-search` for rework outcome detection rules retrospective — found outcomes #218, #752, #654 (no rework patterns specific to detection rule rewrites; no severity adjustments)
- Queried: `/uni-knowledge-search` for risk pattern — found #261 (AuditSource-driven behavior differentiation; confirms source_domain guard as security pattern), #377/#363 (wave-based refactoring; confirms R-03 fixture gap is well-precedented)
- Queried: `/uni-knowledge-search` for SQLite migration — found #370/#681 (create-new-then-swap pattern; informed FM-05 migration idempotency failure mode)
- Stored: nothing novel to store — the cross-domain false finding contamination risk (R-01) is specific to this feature; would warrant a pattern entry if it recurs across 2+ features.
