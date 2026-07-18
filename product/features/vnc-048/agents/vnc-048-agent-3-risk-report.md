# Agent Report: vnc-048-agent-3-risk (Architecture-Risk mode)

**Deliverable**: `product/features/vnc-048/RISK-TEST-STRATEGY.md`

## Summary
14 risks: 3 Critical (R-01 resolver disagreement unproven, R-02 open-before-existence-gate, R-03 live-daemon vector clobber), 6 High, 5 Medium. Every SR-01..SR-11 traced. Behavioral-outcome coverage maps all 7 scope-lens entry points to scenarios driving the operator's real CLI invocation. Four deploy shapes covered as an axis incl. the host-bind-mount fail-loud corner.

## Gate non-negotiables
- **AC-09 disagreement seam (top weight)**: seed `X/<slug>` via runtime literal-slug layout, seed `X/<hash>` differently with a disjoint non-empty set B, read via `run_export_with_base(slug=…)`; assert emitted == A and ∩ B == ∅. A same-path N=1 test is CEREMONIAL (#4974) and does NOT satisfy SR-01 — flagged explicitly in the strategy.
- **R-03 S2 served-vector-from-`start`**: prove restore from `start` onward via a served vector query after `register→stop→import→start`, not from disk state.

## Gaps / flags for downstream
- AC-08 (export against live daemon read-only) vs R-03: export is read-only and permitted while a daemon is live; import is not. Tester must not conflate the two — the live-PID hard-error is import-only.
- In-container and local-dev base derivation (R-05 S2/S3) may be asserted at the derivation-unit level to avoid a container in CI; the `*_with_base` and host-bind-mount shapes get real integration coverage. Tester to confirm this split is acceptable.
- OQ-5 (`#5586` retag on AC-09/AC-10 evidence) is a vision-session call, not covered here — flagged, not filed.

## Knowledge Stewardship
- Queried: context_search "two resolvers path-hash slug disagree seam" → #4974 (ceremonial-seam/N=1 false confidence), #4950 (single-funnel resolver), #5344 (ADR-004 single-restart two-slug), #5087, #4962; context_search (pattern) "backup restore round-trip vector clobber" → #2676 (VectorIndex snapshot round-trip test pattern), #2621 (open_readonly no-migrate — AC-08 analogue), #2673/#3764. All applied.
- Declined: nothing novel to store — #4974 (ceremonial seam) and #5507 (two-resolver trap) already capture the operative patterns; vnc-048 risks are feature-specific interpretations. Promotable at retro only if "four-deploy-shape-as-coverage-axis" recurs in the sibling-CLI slug-awareness work.
