# Agent Report: crt-049-agent-6-retrospective

**Component**: Component 5 — `render_knowledge_reuse` in `crates/unimatrix-server/src/mcp/response/retrospective.rs`
**Feature**: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

---

## Files Modified

- `crates/unimatrix-server/src/mcp/response/retrospective.rs`
- `crates/unimatrix-server/src/mcp/tools.rs` (compile-gap fix only — see Blockers)

---

## Changes Made

### render_knowledge_reuse (retrospective.rs lines 992–1090)

1. **AC-17 GATE guard**: Replaced `if reuse.search_exposure_count == 0` with `if reuse.total_served == 0 && reuse.search_exposure_count == 0`. Injection-only cycles now render correctly.

2. **Summary line**: Replaced `"**Distinct entries served**: {search_exposure_count}"` with `"**Entries served to agents (reads + injections)**: {total_served}  |  **Stored this cycle**: {total_stored}"` followed by two sub-metric bullet lines.

3. **Sub-metric lines**: Added `"- Search exposures (distinct): {search_exposure_count}"` and `"- Explicit reads (distinct): {explicit_read_count}"`.

4. **by_category relabel**: `"**By category (all {} served)**"` → `"**Search exposure categories (all {} exposed)**"`.

5. **Explicit read categories section**: Added `if !reuse.explicit_read_by_category.is_empty()` block rendering `"**Explicit read categories**: ..."` sorted by count descending then name ascending, before the top cross-feature entries table.

6. **Existing test updates**: Updated `test_knowledge_reuse_full` and `test_knowledge_reuse_section` to assert the new labels instead of the old `"Distinct entries served"` string.

### tools.rs (compile-gap fix)

The `knowledge_reuse.rs` agent extended `compute_knowledge_reuse` with two new parameters (`explicit_read_ids: &HashSet<u64>`, `explicit_read_meta: &HashMap<u64, EntryMeta>`) but did not update the call site in `tools.rs`. This caused a compile error blocking all agent test suites. Added empty-ref stubs at the call site with a comment identifying the pending tools.rs agent.

---

## Tests

**New tests added** (6):
- `test_render_knowledge_reuse_golden_output_all_sections` — AC-07: all three labeled lines, correct values, correct ordering, legacy label absent
- `test_render_knowledge_reuse_explicit_read_categories_section` — AC-07: explicit read categories section renders with category names
- `test_render_knowledge_reuse_no_explicit_read_categories_when_empty` — section omitted when map is empty
- `test_render_knowledge_reuse_injection_only_cycle_not_suppressed` — AC-17 GATE: total_served=3, both sub-counts=0, guard does not fire
- `test_render_knowledge_reuse_zero_guard_both_zero` — both zero: early-return fires correctly
- `test_render_knowledge_reuse_no_legacy_distinct_entries_served_label` — regression: legacy label never appears

**Test results** (retrospective module):
- 147 passed, 0 failed

**Pre-existing failures** (not my scope):
- `mcp::knowledge_reuse::tests::test_compute_knowledge_reuse_explicit_read_by_category_populated` — agent 3's knowledge_reuse.rs tests
- `mcp::knowledge_reuse::tests::test_compute_knowledge_reuse_no_early_return_for_explicit_read_only_cycle` — agent 3's knowledge_reuse.rs tests

---

## Issues / Blockers

**Compile gap from parallel agents**: `compute_knowledge_reuse` signature was extended by agent 3 (knowledge_reuse.rs) without the call site in `tools.rs` being updated. This blocked the entire test suite. Fixed with empty-ref stubs in tools.rs — the tools.rs agent (agent 4) must replace these stubs with the real `explicit_ids` and `explicit_meta_map` from the attributed observation slice. See the comment in tools.rs at the `explicit_read_ids_empty` variable.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entry #4216 (crt-049 ADR on total_served aliasing) confirmed the render function was the target; no novel patterns surfaced from briefing.
- Stored: entry #4220 "Stub call sites immediately when a parallel agent extends a function signature across file boundaries" via `/uni-store-pattern` — discovered the compile-blocking gap pattern during this implementation.
