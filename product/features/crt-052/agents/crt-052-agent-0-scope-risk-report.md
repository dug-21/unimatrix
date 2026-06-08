# Agent Report: crt-052-agent-0-scope-risk

Mode: scope-risk. Produced `SCOPE-RISK-ASSESSMENT.md` (45 lines, under 100-line constraint).

## Risk Summary
- 9 scope-level risks (SR-01..SR-09). By severity: High 4 (SR-01, SR-02, SR-04, SR-07), Med 4 (SR-03, SR-05, SR-08, SR-09), Low 1 (SR-06).
- 4 assumptions flagged, each tied to a SCOPE section (AC-11, AC-03/OQ-6, Constraint 13, ADR-007 §2).

## Top 3 for Architect Attention
1. SR-01 — Option B held-buffer memory bound is unbounded by default (cap × held-count, no natural ceiling). Needs explicit held-session cap + independent stale-sweep TTL.
2. SR-04 — #700 seam-shape coupling: `take_transcripts_for_feature` return type must serve marker-recovery now or face an expensive retrofit (Constraint 4 single-reader invariant).
3. SR-07 — crt-033 memoization persist (#3793) is the secrets-breach trap: candidates must not ride the memoized `cycle_review_index` struct (AC-06 / #4721).

## Citation Note Applied
Used #4764 as the active ADR-008 entry (not superseded #4746). Treated vnc-030 ADR-007 (#4819) close/sweep + single-reader contract as binding despite the stale `deprecated` label (per PR #702 / #700).

## Knowledge Stewardship
- Queried: /uni-knowledge-search (context_search) for lesson-learned failures, risk patterns, drain/sweep lifecycle, fixture self-fulfilment. Findings: #981 (NULL feature_cycle silently breaks retrospective — informs SR-02), #4799 (per-turn drain pattern — confirms Option B problem space), #3359 (window/threshold mismatch over-deprecation — informs SR-08 hole/elision calibration), #4489 (predecessor-stub is scope-gate not design risk — framing), #4744 (buffer-cap on RetentionConfig — informs SR-01 memory bound). No prior cross-feature "risk pattern" entry for held-buffer/state-machine reuse.
- Stored: nothing novel to store — the dominant risks (Option B lifecycle, seam coupling, memoization persist) are feature-specific to crt-052; no 2+-feature pattern emerged beyond what #4799 / #4489 already capture.
