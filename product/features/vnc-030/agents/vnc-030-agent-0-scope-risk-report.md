# Agent Report: vnc-030-agent-0-scope-risk

Mode: scope-risk · Date: 2026-06-08

## Deliverable

`product/features/vnc-030/SCOPE-RISK-ASSESSMENT.md` (48 lines) — 12 risks (SR-01..SR-12), 6 assumptions, 6 design recommendations.

## Risk Summary

| Severity | Count | IDs |
|----------|-------|-----|
| High | 3 | SR-01, SR-02, SR-06 (+ SR-09 high-severity integration) |
| Medium | 6 | SR-03, SR-04, SR-05, SR-07, SR-10, SR-11 |
| Low | 2 | SR-08, SR-12 |

## Top 3 for Architect Attention

1. **SR-02** — client size budget at 99,997/100,000 bytes with two stacked dependencies on vnc-027 (delivery order + OQ2 size outcome). Architecture needs measured per-module byte estimates and a fallback.
2. **SR-01** — stamp rests on uncontracted Claude Code behavior (`--resume` id reuse, root-id inheritance); `stamp_miss` canary must be a design invariant with a defined trigger, not a counter.
3. **SR-09/SR-11** — cross-feature seams: `build-request.js` interception-seam contract vs vnc-027, and the still-open worktree cwd dump (this session's first task) gating AC-08.

## Knowledge Stewardship

- Queried: /uni-knowledge-search ×4 (lessons on attribution/gate failures; risk patterns; migration precedent; wire-contract lessons) — found #3486 (context_cycle payload-construction regression, informs SR-03), #924 (file-grouped parallel delivery, informs SR-09), #4358/#374/#681 (migration idempotency precedent, lowers SR-04 likelihood), #953 (human-override propagation rework class).
- Stored: nothing novel to store — all identified risks are feature-specific to vnc-030; the applicable cross-feature patterns (#3486, #924, #4358) already exist in Unimatrix.
