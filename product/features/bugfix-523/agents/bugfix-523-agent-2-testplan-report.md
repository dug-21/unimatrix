# Agent Report: bugfix-523-agent-2-testplan (Stage 3a — Test Plan Design)

## Task

Design per-component test plans for bugfix-523: NaN guard validation in InferenceConfig,
nli_detection_tick early-exit gate, log level downgrade, and session ID sanitization fixes.

## Outputs

| File | Lines | Purpose |
|------|-------|---------|
| `/workspaces/unimatrix/product/features/bugfix-523/test-plan/OVERVIEW.md` | ~120 | Overall strategy, risk-to-test mapping, integration harness plan |
| `/workspaces/unimatrix/product/features/bugfix-523/test-plan/nli-tick-gate.md` | ~110 | AC-01/AC-02/AC-03 — Items 1+2, tick gate tests |
| `/workspaces/unimatrix/product/features/bugfix-523/test-plan/log-downgrade.md` | ~90 | AC-04/AC-05 — behavioral-only log level tests, ADR-001(c) documentation |
| `/workspaces/unimatrix/product/features/bugfix-523/test-plan/nan-guards.md` | ~160 | AC-06..AC-27 — all 19 NaN tests + 2 Inf tests + regression, Group A/B/C |
| `/workspaces/unimatrix/product/features/bugfix-523/test-plan/session-sanitization.md` | ~110 | AC-28/AC-29 — dispatch arm guard tests |

Total new test functions planned: 30 (4 + 3 + 21 + 2).

## Risk Coverage Summary

| Risk | Priority | Status |
|------|----------|--------|
| R-01 (Path A/C gated) | Critical | Covered: T-02, T-03 (both non-negotiable) |
| R-02 (gate wrong boundary) | Critical | Covered: T-03 (structural proof) + code inspection |
| R-03 (19-field NaN omission) | Critical | Covered: 19 individually-named tests AC-06..AC-24 |
| R-04 (guard after session_id use) | Critical | Covered: T-08 (runtime) + code inspection |
| R-05 (wrong warn site) | High | Covered: T-07 (behavioral) + code inspection checklist |
| R-06 (test module absent) | High | Covered: Gate 3a presence-count = 21 NaN tests |
| R-07 (wrong field name string) | High | Covered: spot-check procedure for AC-17..AC-24 |
| R-08 (valid events rejected) | High | Covered: T-09 (regression guard) |
| R-09 (NLI-enabled regressed) | Med | Covered: T-04 |
| R-10 (boundary tests regress) | Med | Covered: AC-27 cargo test command |
| R-11 (behavioral-only unacknowledged) | Med | Covered: mandatory gate report statement in all component files |
| R-12 (cross-field NaN pass-through) | Low | Covered upstream by AC-07 + AC-08; no additional test needed |

## Integration Suite Plan

No new infra-001 integration tests. Behavior changes are internal to the server crate and
not observable through the MCP JSON-RPC interface. Mandatory smoke gate at Stage 3c.

## Key Design Decisions Applied

- AC-01 test uses non-empty `candidate_pairs` per IMPLEMENTATION-BRIEF.md note (empty pairs
  trigger the pre-existing fast-exit, not the new `nli_enabled` gate).
- Items 1 and 2 share one test module. `nli-tick-gate.md` covers Items 1+2 together.
  `log-downgrade.md` provides supplemental ADR-001(c) documentation.
- All 19 NaN tests listed individually per group (A/B/C) — not as "loop group". Each has
  exact function name matching IMPLEMENTATION-BRIEF.md checklist.
- AC-27 is a cargo command, not a named test function.
- Gate report statement for AC-04/AC-05 is specified verbatim in all relevant plan files.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned #4143 (ADR-001 for this batch,
  full content retrieved via context_get), #4133 (NaN guard pattern), #4142 (log-level AC
  pattern), #3548 (test plan assertion coverage gap lesson), #3766 (InferenceConfig NaN
  from bugfix-444). All applied.
- Queried: `context_search` for bugfix-523 ADR decisions — confirmed entry #4143 retrieved.
- Queried: `context_search` for NaN guard testing — confirmed #4133 and #3548 applied.
- Stored: nothing novel to store — the behavioral-only log test pattern is in #4142, the
  NaN guard test pattern is in #4133. No new cross-feature patterns visible from this batch.

## Open Questions

None. All OQs from source documents were resolved in ADR-001 (entry #4143) and
IMPLEMENTATION-BRIEF.md prior to Stage 3a.
