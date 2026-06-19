# Agent Report: vnc-040-vision-guardian

## Result
ALIGNMENT-REPORT.md produced. **No variances requiring human approval.**

Counts: PASS 6, WARN 0, VARIANCE 0, FAIL 0.

## Summary
- Vision Alignment: PASS — advances #4946 (one isolation seam, graceful degradation) and #4678 (configured-not-rebuilt); delivers C6 (#5148).
- Milestone Fit: PASS — successor to vnc-034 + crt-056 seam; A/B split keeps scope minimal.
- Scope Gaps: PASS — all 9 SCOPE goals/ACs carried into spec FRs/ACs.
- Scope Additions: PASS — one in-scope design refinement, no new knobs.
- Architecture Consistency: PASS.
- Risk Completeness: PASS — SR-01…SR-08 fully traced; 11 risks / 27 scenarios.

## Flagged item assessed (per spawn prompt)
The `[embedding]`-section lock resolving to pin-global-wins + forward guard (R-06) does **NOT** weaken the model invariant. Intent ("merged config can never describe a model the served handle is not") is preserved two ways: (1) `embed_handle` `Arc::clone`d outside any merge (ARCH §6a) — no 2nd model can load; (2) the only descriptor that exists, `inference.embedding_model_sha256`, is already global-wins (#4655). The "whole-section" language was defensive over-specification of a section that does not exist; the resolution is the honest implementation of the same intent, with A2/R-06 guarding the `VectorConfig::default()` dependency. AC-04 still asserts the behavioral guarantee.

## Report path
product/features/vnc-040/ALIGNMENT-REPORT.md

## Knowledge Stewardship
- Queried: /uni-query-patterns for vision alignment patterns -- surfaced #2298 (config-key semantic divergence), #3742 (future branch must match scope deferral, WARN pattern), #3337 (arch/spec string divergence). None triggered: embedding refinement is documented (not silent), deferred Feature B matches scope deferral.
- Stored: nothing novel to store -- variances are feature-specific (over-specified scope assumption corrected by code inspection), not a cross-feature recurring misalignment; #3742 already covers the relevant pattern and this design satisfies it.
