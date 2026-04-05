# Agent Report: bugfix-523-agent-0-scope-risk-v2

## Role
Scope-Risk Strategist — MODE: scope-risk

## Output
- `/workspaces/unimatrix/product/features/bugfix-523/SCOPE-RISK-ASSESSMENT.md`

## Risk Summary

| Severity | Count |
|----------|-------|
| High     | 1 (SR-01) |
| Medium   | 4 (SR-02, SR-03, SR-04, SR-05) |
| Low      | 1 (SR-06) |

**Total: 6 risks**

## Top 3 Risks

1. **SR-01** (High) — ADR-001 gate placement. Insertion boundary is load-bearing per ADR #4017. Gate must land after Path C, before `get_provider()`. A shift of even a few lines into Path C territory silently breaks unconditional Informs edge accumulation in production.

2. **SR-03** (Med/High likelihood) — Tracing-level AC testability for Items 1 and 2. Historical gate failures (lesson #3935, crt-036) show this exact class of AC is deferred or rejected. Must be resolved in specification — either full tracing-test harness or explicit behavioral-only AC.

3. **SR-02** (Med) — 19-field edit completeness in ~8000-line config.rs. Mechanical omission risk. Field list and ACs (AC-06 through AC-24) are the primary mitigations; tester must treat as checklist, not sample.

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned failures/gate rejections — found lessons #3935 (tracing AC deferral), #4076 (missing test modules), #2758, #1203, #2577 — directly relevant to Items 1, 2, 3 test gaps
- Queried: `/uni-knowledge-search` for risk patterns — found pattern #3426 (regression risk), #2325 (blast radius), #1494 (snapshot-before-spawn); no direct hardening-batch pattern found
- Queried: ADR #4017 (crt-039 ADR-001) in full — confirmed gate placement invariant; used to assess SR-01
- Stored: entry #4142 "Tracing log-level ACs consistently deferred or fail at gate" via `/uni-store-pattern` — pattern visible across crt-036, bugfix-523, and lesson #3935; novel enough to store
