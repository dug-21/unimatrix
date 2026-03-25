# Agent Report: col-027-agent-3-risk

## Output

- Produced: `/workspaces/unimatrix/product/features/col-027/RISK-TEST-STRATEGY.md`
- Mode: architecture-risk
- Artifacts consumed: SCOPE.md, SCOPE-RISK-ASSESSMENT.md, ARCHITECTURE.md, ADR-001 through ADR-005, SPECIFICATION.md

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 2 |
| High | 3 |
| Med | 6 |
| Low | 3 |
| **Total** | **14** |

## Top Risks by Severity

1. **R-01 (Critical)** — `extract_error_field()` absent or miscalled: `PostToolUseFailure` arm calls `extract_response_fields()` instead, silently returning `(None, None)`. Error content lost with no test failure signal. ADR-002 mitigates via separate function; R-01 test scenarios enforce it.

2. **R-02 (Critical)** — Partial two-site differential fix: `friction.rs` and `metrics.rs` updated in separate commits causes metric/rule divergence. ADR-004 mandates same-commit delivery; R-02 requires a cross-site assertion in one test function.

3. **R-03 (High)** — `extract_observation_fields()` wildcard fall-through: `"PostToolUseFailure"` arm missing stores `tool = None`, making all records invisible to per-tool rules. AC-03 must assert `obs.tool.is_some()` explicitly.

4. **R-04 (High)** — `PermissionRetriesRule` still fires for fully-terminal failure sessions: `PostToolUseFailure` not counted in terminal bucket. Core feature goal not achieved. Guarded by AC-05 test with `make_failure` helper.

5. **R-14 (High)** — Settings.json registration uses wrong event key casing — hook binary never invoked by Claude Code on tool failure. Entire feature is a silent no-op at runtime.

## Open Questions

None. All specification open questions were resolved before architecture. All SR items traced.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection observation hook" — found entry #699 (silent data orphaning in hook pipeline)
- Queried: `/uni-knowledge-search` for "risk pattern observation pipeline hook dispatch detection rule" — found entries #763, #2907, #2928
- Queried: `/uni-knowledge-search` for "PermissionRetriesRule differential friction false positive" — found entries #3446, #3419, #3472, #3476, #3477
- Queried: `/uni-knowledge-search` for "SQLite observation event_type wildcard silent data loss" — found entries #2903, #384, #3312
- Queried: `/uni-knowledge-search` for "hook binary exit code defensive parsing" — found entries #247, #3335, #2909
- Stored: nothing novel to store — all identified patterns already captured in entries #3472 (atomic two-site update) and #699/#3474 (wrong-extractor silent loss)
