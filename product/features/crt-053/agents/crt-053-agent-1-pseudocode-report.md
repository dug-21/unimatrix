# Agent Report — crt-053-agent-1-pseudocode

**Role**: Pseudocode specialist (Stage 3a)
**Feature**: crt-053 Active-Only PPR Expansion Seeds

## Deliverables
- `product/features/crt-053/pseudocode/OVERVIEW.md`
- `product/features/crt-053/pseudocode/search-seed-filter.md`

## Components covered
- `SearchService::search` Phase 0 seed build (`crates/unimatrix-server/src/services/search.rs`) — the single component. No decomposition (LOCKED single-edit feature).

## Edit-site verification (against live source — #4886 churn caution honored)
- Brief's line numbers HOLD exactly. Edit site is line **915**, inside `if self.ppr_expander_enabled {` at line **911**.
- `graph_expand` call: lines 919-924. `in_pool` dedup: line 929. Quarantine gate: line 950 (unchanged, R-11).

## OQ-2 — RESOLVED
`results_with_scores` IS the sole seed source for `graph_expand` inside the enabled branch.
`seed_ids` (915) is built only from `results_with_scores`; `graph_expand` receives exactly `&seed_ids`;
`in_pool` derives from `seed_ids`. No second seed path. FR-01 scope complete with the single filter; R-09 cleared.

## Findings that refine (not contradict) the brief
1. **No import edit needed.** `Status` is already in scope — imported at line 10 (`use unimatrix_core::{ ... Status ... }`) and already used in production code (lines 718, 727, 737, 765, 814, 1125). The brief's "import if needed (the only permissible adjacent edit)" turns out unnecessary. C-01 is tighter than stated: the diff is the filter clause alone.
2. **Type accuracy.** `results_with_scores` is `Vec<(EntryRecord, f64)>` (line 681), not `f32` as the brief/architecture state. Irrelevant to the predicate (score bound to `_`); documented for accuracy.

## Open questions
- **OQ-1** (fixture host for AC-01/AC-05) remains for the tester/architect — unchanged; not a pseudocode blocker.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern: PPR graph_expand seeds) → #3992 (BFS-from-seeds-before-ranking), #4434 (RelationType-to-PPR coordinated-sites), #3744 (Direction::Outgoing = reverse walk for power iteration — relevant to SR-06 framing). context_search (decision, topic crt-053) → #4917 ADR-001; retrieved full via context_get.
- Deviations from established patterns: none. The pseudocode reuses the integration surface verbatim; introduces no new symbol, helper, or config.
