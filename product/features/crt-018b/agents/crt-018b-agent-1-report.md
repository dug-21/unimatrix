# Agent Report: crt-018b-agent-1-pseudocode

## Completion Status: COMPLETE

All 7 pseudocode files produced and written to
`product/features/crt-018b/pseudocode/`.

## Files Produced

| File | Component | Lines |
|------|-----------|-------|
| `OVERVIEW.md` | Cross-component | 98 |
| `effectiveness-state.md` | Component 1: EffectivenessState cache | ~110 |
| `background-tick-writer.md` | Component 2: Background tick writer | ~180 |
| `search-utility-delta.md` | Component 3: Search utility delta | ~200 |
| `briefing-tiebreaker.md` | Component 4: Briefing tiebreaker | ~185 |
| `auto-quarantine-guard.md` | Component 5: Auto-quarantine guard | ~190 |
| `auto-quarantine-audit.md` | Component 6: Auto-quarantine audit | ~170 |

## Open Questions (Flagged, Not Assumed)

### OPEN QUESTION 1 — Per-entry classification list in `EffectivenessReport`

The background tick writer needs to iterate all classified entries to build the new `categories`
HashMap in `EffectivenessState`. `EffectivenessReport` has aggregate `by_category: Vec<(EffectivenessCategory, u32)>` (counts, not IDs) and three partial lists: `top_ineffective`,
`noisy_entries`, `unmatched_entries`. These do not cover Effective and Settled entries by ID.

**Options for implementation agent:**
- Option A: `StatusService.compute_report()` already builds a full flat list internally during
  Phase 8 (see `status.rs`). Expose it as an additional field on `EffectivenessReport`
  (e.g., `all_entries: Vec<EntryEffectiveness>`). This is the cleanest approach.
- Option B: Reconstruct the per-entry map by reading entries from the store separately in
  `maintenance_tick` — but this adds SQL overhead at tick time.
- Option C: Modify `StatusService.compute_report()` to also return the flat entry list as a
  separate return value alongside `StatusReport`.

Implementation agent must choose and implement one approach. Option A is recommended.

### OPEN QUESTION 2 — `EntryEffectiveness.entry_category` field absent

`EntryEffectiveness` (in `unimatrix-engine/src/effectiveness/mod.rs`) has `entry_id`, `title`,
`topic`, `trust_source`, `category` (effectiveness category enum), `injection_count`,
`success_rate`, `helpfulness_ratio`. It does NOT have a `knowledge_category: String` field
(i.e., the entry's type: "decision", "convention", "lesson-learned", etc.).

The auto-quarantine audit event (FR-11) requires `entry_category` (knowledge category).

**Options for implementation agent:**
- Option A: Fetch the `EntryRecord` from the store at quarantine time to get the category —
  this is within the `spawn_blocking` context so synchronous SQL is fine.
- Option B: Add `knowledge_category: String` to `EntryEffectiveness` and populate it in
  `compute_report()` Phase 8.
- Option C: Use `trust_source` as a fallback (less informative but no new SQL).

Option A is recommended — it is isolated to the auto-quarantine code path and requires no
change to the engine crate.

## Critical Constraints Encoded

All critical constraints from the spawn prompt are reflected in pseudocode:

| Constraint | Location in pseudocode |
|-----------|----------------------|
| Lock ordering: read generation, DROP guard, then lock mutex (R-01, ADR-001) | `search-utility-delta.md` snapshot section, `briefing-tiebreaker.md` snapshot section, `OVERVIEW.md` lock ordering diagram |
| Write lock dropped before `quarantine_entry()` SQL (NFR-02, R-13) | `background-tick-writer.md` step 7-9, `auto-quarantine-guard.md` counter reset section |
| `utility_delta` inside `status_penalty` multiplication (ADR-003) | `search-utility-delta.md` Step 7 and Step 8 pseudocode, numeric example in test scenario 3 |
| `EffectivenessStateHandle` non-optional on `BriefingService::new()` (ADR-004) | `briefing-tiebreaker.md` constructor section |
| `effectiveness_priority` scale: Effective=2, Settled=1, Unmatched/nil=0, Ineffective=-1, Noisy=-2 | `briefing-tiebreaker.md` `effectiveness_priority` function |
| Poison recovery `.unwrap_or_else(|e| e.into_inner())` on ALL lock ops | `effectiveness-state.md` error handling, `search-utility-delta.md` error handling, `briefing-tiebreaker.md` error handling |
| `AUTO_QUARANTINE_CYCLES` validated at startup (0 disables, >1000 = error) | `background-tick-writer.md` `parse_auto_quarantine_cycles()` |
| Only Ineffective/Noisy entries quarantined (AC-14, R-11) | `background-tick-writer.md` step 7 counter scan, `auto-quarantine-guard.md` defensive category check |
| `consecutive_bad_cycles` hold on tick error (ADR-002) | `background-tick-writer.md` Err branch |
| Counter removed (not zeroed) for entries absent from tick classification set | `background-tick-writer.md` step 6 `.retain()` |

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server services` patterns — findings:
  `ConfidenceState` in `services/confidence.rs` is the exact structural template for
  `EffectivenessState`. All field patterns, lock patterns, and `new_handle()` factory
  method replicated verbatim.
- Queried: `/uni-query-patterns` for `background.rs maintenance_tick` — findings: existing
  `maintenance_tick` signature, `compute_report()` call, and error handling pattern observed
  in source. New parameters thread through `run_single_tick` following the `confidence_state`
  pattern already established.
- Queried: `/uni-query-patterns` for `BriefingService sort` — findings: existing sort in
  `process_injection_history` (Step 3) uses single-key `sort_by` on confidence. Composite
  key extension follows the pattern already present in the feature convention sort
  (feature-tag primary, confidence secondary).
- Deviations from established patterns: none. All pseudocode follows established codebase
  patterns without invention.
