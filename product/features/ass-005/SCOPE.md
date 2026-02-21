# ass-005: Learning Model Assessment

**Phase**: Assimilate (Research)
**Parent**: Pre-Roadmap Spike, Track 1C
**Date**: 2026-02-20
**Status**: In Progress

---

## Objective

Read sona's ReasoningBank, LoRA, and EWC++ code (not to vendor, but to understand). Compare sona-style ML learning with a simpler metadata lifecycle approach. Determine which model to design the MCP tool interface around.

## Research Questions

| # | Question | Why It Matters for Interface Design |
|---|----------|-------------------------------------|
| Q1 | What does ReasoningBank's `store_pattern` / `find_patterns` API look like? | Informs whether our learning tools should mirror this shape or use a simpler model |
| Q2 | Does K-means++ clustering produce useful groupings for development knowledge? | Determines whether `memory_search` should return cluster/category information |
| Q3 | What does the trajectory model (begin/step/end/reward) actually track? | Informs whether we need explicit trajectory tools or implicit session tracking |
| Q4 | Is confidence scoring meaningful for code knowledge? (decay rates, promotion thresholds) | Determines whether confidence is a first-class field in search results |
| Q5 | Would a simpler metadata state machine (active/aging/deprecated + correction links) cover 90% of the value? | Key decision: sona-style ML learning vs. lifecycle metadata learning |

## Deliverable

**D3: Learning Model Comparison** — sona's approach vs. metadata lifecycle approach. Recommendation for which model to design the interface around.

## Tracking

Research findings stored in `research/` subdirectory.
Final comparison: `research/D3-learning-model-comparison.md`
