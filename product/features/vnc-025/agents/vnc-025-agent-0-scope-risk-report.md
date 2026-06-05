# Agent Report: vnc-025-agent-0-scope-risk

## Mode
scope-risk

## Artifact
`product/features/vnc-025/SCOPE-RISK-ASSESSMENT.md` — 9 risks (3 technology, 3 scope-boundary, 3 integration), 4 assumptions, 6 design recommendations. Under 100 lines.

## Risk Summary

| Severity | Count | IDs |
|----------|-------|-----|
| High | 3 | SR-01, SR-02, SR-07 |
| Medium | 5 | SR-03, SR-04, SR-05, SR-08, SR-09 |
| Low | 1 | SR-06 |

## Top 3 for Architect/Spec Attention

1. **SR-01** — `SessionState: Clone` hot-path deep-clone (High/High). Buffer field shape must be the architect's first decision; pattern #4737 already records the trap.
2. **SR-07** — Batch-filter rewiring risks reopening delta-bytes → durable observation row (High/Med). The vnc-024 non-persistence filter is load-bearing; merge must tee before it, not replace it.
3. **SR-02** — Secrets posture is architectural-only, no redactor safety net (High/Med). Buffer type should be content-opaque by construction; AC-12 grep/test gate is a hard criterion.

## Knowledge Stewardship
- Queried: /uni-knowledge-search (4 queries: lessons on gates/session-registry, risk patterns, PreCompact precedent, SessionState clone/mutex) — found 7 directly applicable entries: #4737, #3902, #4134, #735, #3158, #3426, #4721; all cited in the assessment.
- Stored: nothing novel to store — the one cross-feature pattern this assessment leans on (SessionState clone-cost) already exists as entry #4737, tagged vnc-025; remaining risks are feature-specific and live in the assessment document.
