# Gate 3a Report: vnc-043

> Gate: 3a (Component Design Review)
> Date: 2026-07-05 (rework iteration 1 re-validation)
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Architecture alignment | PASS | Two components match architecture decomposition; reused signatures identical to Integration Surface; ADR-001/002/003 reflected |
| 2. Specification coverage | PASS | FR-1..FR-10 + NFR-1..NFR-7 all have corresponding pseudocode; no scope additions (sort helper is the ADR-003 mechanism, not new scope) |
| 3. Risk coverage | PASS | All 11 risks (R-01..R-11) map to test scenarios; Critical trio R-03/R-04/R-07 detailed; integration + edge cases present |
| 4. Interface consistency | PASS | Shared types coherent across OVERVIEW/components; spawn-prompt interface invariants all honored |
| 5. Knowledge stewardship compliance | PASS | Stewardship blocks now present in BOTH pseudocode/OVERVIEW.md and test-plan/OVERVIEW.md with `Queried:` evidence + `Stored: nothing novel -- {reason}`; design content unchanged |

5 / 5 checks PASS. Prior blocker (Check 5) remediated; no design drift.

## Rework Iteration 1 Re-Validation

Prior result was REWORKABLE FAIL on Check 5 only. Rework added a `## Knowledge Stewardship`
block to both `pseudocode/OVERVIEW.md` (lines 77-86) and `test-plan/OVERVIEW.md` (lines 104-112).

- Pseudocode OVERVIEW block: `Queried:` = `context_briefing` surfacing ADR-001/003 vnc-043
  (#5448/#5450), lessons #4562/#4526, ADR-005 vnc-018 (#4479), ADR-003 vnc-018 (#4490);
  `Deviations: none`; `Stored: nothing novel` with reason (patterns already recorded). Satisfies
  the read-only-agent `Queried:` obligation.
- Test-plan OVERVIEW block: `Queried:` = `context_briefing` + `context_search`
  (#5448/#5449/#5450, #4479, #4493, #4562, #4526); `Stored: nothing novel` with reason. Prior
  WARN cleared.
- Design content re-confirmed unchanged: components table, data-flow diagram, reused-function
  surface, shared types, and shared invariants (exact `==1` dispatch pre-lock, presentation-only
  set-preserving sort, twin-literal byte-equality #869, no wire/struct change) are identical to
  what Checks 1-4 validated. The stewardship blocks are additive at file end — no drift into the
  passing checks.

Checks 1-4 not re-litigated beyond this drift confirmation (per rework scope).

## Detailed Findings

### Check 1 — Architecture alignment
**Status**: PASS
**Evidence**:
- Component breakdown matches: pseudocode OVERVIEW defines exactly `subgraph-depth1-dispatch` and `doc-surfaces`, mirroring ARCHITECTURE.md §Component Breakdown (handler dispatch + 4 doc edit points).
- Reused function surface is byte-identical to ARCHITECTURE.md §Integration Surface: `handle_subgraph(store, typed_graph_state, params) -> Result<SubgraphResponse, ErrorData>` and `subgraph_via_db(store, seed_ids, max_depth, max_nodes, petgraph_dirs, edge_types, resolve_supersessions)`. No new `subgraph_sql` helper invented (architecture's explicit prohibition honored).
- ADR consistency: ADR-001 (reuse + insertion after `resolve_supersessions` @ `:162`, before lock), ADR-002 (twin-literal + #869 byte guard), ADR-003 (uniform sort keys + ≥30 fan-in truncation) all carried verbatim into pseudocode invariants.

### Check 2 — Specification coverage
**Status**: PASS
**Evidence**:
- FR-1/FR-2 → doc-surfaces edit points 1&2 (schemars docs, drop "neighbors only", add subgraph). FR-3/FR-4 → edit points 3&4 (twin literals, filter availability + staleness carve-out). FR-5 → snapshot-absence confirmed in architecture, re-grep step in test plan. FR-6 → exact `== 1` early return before `use_fallback`. FR-7 → depth-1 data flow preserves edge_types/direction/hydration/metadata/max_nodes/resolve_supersessions. FR-8 → depth>1 SET unchanged. FR-9 → uniform `sort_subgraph_output` on both paths. FR-10 → chain/current/neighbors untouched.
- NFR-1 (wire lock), NFR-2 (no lock on depth-1), NFR-3 (SET parity test), NFR-4 (stable sort determinism), NFR-5 (`truncated` surfaced), NFR-6 (load-bearing regression), NFR-7 (≥30 fan-in) all represented.
- No unrequested features. The added private `sort_subgraph_output` helper is the structural mechanism ADR-003/FR-9 mandate, not scope creep.

### Check 3 — Risk coverage
**Status**: PASS
**Evidence**: Every risk in RISK-TEST-STRATEGY.md maps to at least one named scenario in the test plans:
- R-01 dispatch (`test_subgraph_depth1_routes_live`, depth2/10 not-live, boundary 0/absent)
- R-02 cold-start fallback (`test_bfs_cold_start_empty_result`, `..._use_fallback_true...`, new empty-graph depth>1 fallback)
- R-03 SET parity (`test_subgraph_depth1_set_parity_vs_warm_cache`, absent/`[]`/explicit + supersession modes)
- R-04 load-bearing (dedup on d1, dangling under cap, `MAX_EDGES_UPPER`)
- R-05 hydration/tag parity
- R-06 ordering both depths + DoD determinism + mandatory sweep
- R-07 doc drift (#869 guard + extended substrings + schemars-doc presence)
- R-08 lock — review-checklist item (correctly noted as review, not runtime)
- R-09 truncation (≥30 false / >199 true)
- R-10 freshness both ways (d1 visible / d>1 within-tick not)
- R-11 direction label invariant
Critical risks (R-03, R-04, R-07) receive the deepest scenario sets; priorities reflected. Integration risks (handler↔helper boundary, dual-caller, query-count) and edge cases (empty/unknown seed, `max_nodes==0`, self-loop, absent vs `[]`) enumerated.

### Check 4 — Interface consistency
**Status**: PASS
**Evidence**: Spawn-prompt interface invariants each verified in the pseudocode:
- Reused surface, no wire/struct change — OVERVIEW §Shared types locks `GraphParams`/`SubgraphResponse`/`EdgeRecord`/`EntryRecord`; doc-only edits to `direction`/`edge_types`.
- Depth-1 takes no `TypedGraphState` lock — dispatch component invariant 1 + correctness constraint: early return precedes `typed_graph_state.read()`.
- Exact `== 1` before `use_fallback` — explicit ("never `<= 1`/range"; inserted before the lock/snapshot block).
- Presentation-only set-preserving sort on both depths — `sort_subgraph_output` called in both `subgraph_via_db` and warm-BFS assembly; runs after dangling filter, never mutates set, `truncated`/`depth_reached` untouched, stable sort.
- Twin-literal byte-equality guard #869 — doc-surfaces requires identical edits to both literals, guard stays green.
No contradictions between OVERVIEW and the two component files.

**Non-blocking open-question reconciliation (verified there is room, not failed on):**
1. schemars-doc assertion mechanism — test-plan doc-surfaces offers `schema_for!(GraphParams)` preferred with an in-crate presence-check fallback. Room preserved for 3b/3c.
2. Exact new substring literals agreed coder↔tester in one PR — test-plan doc-surfaces states "Coder + tester agree the exact substring literals ... one PR." Preserved.
3. Warm (non-empty) `TypedRelationGraph` fixture for the depth>1 within-tick staleness test — `test_subgraph_depth_gt1_within_tick_write_not_visible` explicitly requires a warm/non-empty cache (empty would fall back to live and mask the assertion). Preserved.

### Check 5 — Knowledge stewardship compliance
**Status**: PASS (was FAIL; remediated in rework iteration 1)
**Evidence**:
- Architect: `agents/vnc-043-agent-1-architect-report.md` has `## Knowledge Stewardship` with `Queried:` (context_briefing, vnc-018/019 ADRs) and `Stored:` (#5448/#5449/#5450). Satisfies active-storage obligation.
- Risk-strategist: RISK-TEST-STRATEGY.md §Knowledge Stewardship has `Queried:` (context_search hits #5396/#4474/#4473) and `Stored: nothing novel to store -- {reason}`. Satisfies (present with reason).
- Pseudocode (read-only agent): `pseudocode/OVERVIEW.md` now carries a `## Knowledge Stewardship` block (lines 77-86) with `Queried:` (context_briefing → ADR-001/003 #5448/#5450, lessons #4562/#4526, ADR-005 vnc-018 #4479, ADR-003 vnc-018 #4490), `Deviations: none`, and `Stored: nothing novel` with reason. The read-only-agent `Queried:` obligation is met.
- Test-plan (Stage-3a artifact): `test-plan/OVERVIEW.md` now carries a `## Knowledge Stewardship` block (lines 104-112) with `Queried:` (context_briefing + context_search: #5448/#5449/#5450, #4479, #4493, #4562, #4526) and `Stored: nothing novel` with reason. Prior WARN cleared.
All four design-phase artifacts now satisfy the ADR-003 composite stewardship check.

## Rework Required

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| Pseudocode has no `## Knowledge Stewardship` / `Queried:` evidence | uni-pseudocode | Add stewardship block (Queried: `/uni-query-patterns` evidence; Stored:/"nothing novel -- reason") to `pseudocode/OVERVIEW.md` or an `agents/` report |
| (WARN) Test-plan 3a has no stewardship block | uni-tester | Optional: add `Queried:` block to `test-plan/OVERVIEW.md` if 3a test-plan is treated as a design artifact |

## Notes

The design content itself is sound — architecture alignment, spec coverage, risk-to-scenario mapping, and interface invariants all pass, and the three non-blocking reconciliation items have explicit room in the plans. The only blocker is a process/stewardship omission that is cheap to remedy without touching any design decision.
