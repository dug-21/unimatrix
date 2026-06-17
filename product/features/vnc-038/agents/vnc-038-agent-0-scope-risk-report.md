# Agent Report: vnc-038-agent-0-scope-risk

**Mode:** scope-risk
**Output:** product/features/vnc-038/SCOPE-RISK-ASSESSMENT.md (47 lines)

## Summary
7 scope-level risks. By severity: 5 High (SR-01, SR-02, SR-03, SR-04, SR-07), 2 Med (SR-05, SR-06). 4 assumptions flagged, each tied to a SCOPE.md section.

## Top 3 for architect attention
- **SR-03 — ceremonial-funnel trap on observe.** vnc-034 shipped the single-funnel ceremonially once (#4974: `let _store` discard + parallel adapter, green at N=1). Observe is being moved onto that same funnel — repeat risk. Prove isolation at N=2.
- **SR-01 — dumb-client bet.** Bet only pays if EVERY client path-composition site is eliminated, not just observe. Enumerate as a closed set; invariant-test verbatim posting.
- **SR-04 — hard-cutover blast radius.** Central route-grammar change (DefaultResolver retire, reserved-slug set) risks breaking local UDS (AC-10) and the MCP seam if over-scoped beyond served-project model.

## Knowledge Stewardship
- Queried: /uni-knowledge-search (context_search) for lesson-learned/pattern/outcome on bundle parity, hard cutover, slug routing, rework -- found #4974 (ceremonial seam, direct precedent for SR-03), #5079 (vnc-038 routing structural consequences, SR-07), #4956 (parity corpus hex/pub(crate) mechanics, SR-02), #4949 (ADR-005 default alias, SR-04). vnc-034-topic lesson-learned search returned no results.
- Stored: nothing novel to store -- the recurring risk patterns (ceremonial seam, parity dual-side atomicity) are already captured as #4974/#4956/#5079; no cross-feature pattern visible across 2+ features beyond what exists. Feature-specific risks live in SCOPE-RISK-ASSESSMENT.md per stewardship rules.
