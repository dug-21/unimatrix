# crt-019 Pseudocode Agent Report

**Agent ID**: crt-019-agent-1-pseudocode
**Date**: 2026-03-14
**Scope**: Per-component pseudocode for all 7 components in crt-019

## Files Produced

| File | Lines | Notes |
|------|-------|-------|
| `product/features/crt-019/pseudocode/OVERVIEW.md` | ~100 | Data flow, shared types, sequencing constraints |
| `product/features/crt-019/pseudocode/confidence-formula-engine.md` | ~220 | Full function pseudocode, call-site inventory, test list |
| `product/features/crt-019/pseudocode/confidence-state.md` | ~165 | ConfidenceState struct, ServiceLayer wiring, read/write patterns |
| `product/features/crt-019/pseudocode/empirical-prior-computation.md` | ~210 | compute_empirical_prior, compute_observed_spread, Step 2b pseudocode |
| `product/features/crt-019/pseudocode/confidence-refresh-batch.md` | ~135 | Duration guard, alpha0/beta0 snapshot-before-loop, batch 500 |
| `product/features/crt-019/pseudocode/deliberate-retrieval-signal.md` | ~240 | UsageContext.access_weight, context_get/lookup changes, R-01/R-11 gates |
| `product/features/crt-019/pseudocode/query-skills.md` | ~45 | Documentation-only changes |
| `product/features/crt-019/pseudocode/test-infrastructure.md` | ~280 | Full test inventory, AC verification map, T-REG-02 ordering |

## Self-Check

- [x] Architecture output was read before writing any pseudocode
- [x] No invented interface names — all names traced to ARCHITECTURE.md, IMPLEMENTATION-BRIEF.md, or source files
- [x] Output is per-component (OVERVIEW.md + 7 component files), not monolithic
- [x] Each component file includes function signatures, error handling, and test scenarios
- [x] No TODO, placeholder functions, or TBD sections — all open questions resolved or explicitly flagged
- [x] Shared types defined in OVERVIEW.md match usage in component files
- [x] All output files within `product/features/crt-019/pseudocode/`

## Key Design Decisions Encoded in Pseudocode

1. **T-REG-02 first** (C-02, R-04): Documented explicitly in test-infrastructure.md as
   the absolute first change before any constant modification. ORDERING section is
   prominent.

2. **R-11 gate documented** (deliberate-retrieval-signal.md): The `flat_map` repeat
   approach for doubled access is conditioned on a store-layer unit test
   `[42, 42] -> access_count += 2`. Fallback to explicit `(id, increment)` pairs
   is documented. Implementation agents must run this test first.

3. **R-01 closure signature** (deliberate-retrieval-signal.md): The `Box<dyn Fn(...) + Send>`
   closure is fully pseudocoded, including the capture of `alpha0`/`beta0` from
   `ConfidenceState` BEFORE `spawn_blocking`. The store's signature change requirement
   is flagged.

4. **C-04 no second spawn** (deliberate-retrieval-signal.md): `params.helpful.or(Some(true))`
   is a single in-process field assignment before the existing `spawn_blocking`. Zero
   new task spawns.

5. **C-05 dedup-before-multiply** (deliberate-retrieval-signal.md): `filter_access` runs
   before the `flat_map` multiplier. Deduped entries produce 0 increments, not 2.

6. **R-06 initial value** (confidence-state.md): `ConfidenceState::new()` uses
   `observed_spread = 0.1471`, `confidence_weight = 0.184`. Zero initialization is
   explicitly called out as a regression risk.

7. **FM-03 poison recovery** (all files): `unwrap_or_else(|e| e.into_inner())` pattern
   is documented on every lock acquisition.

8. **IR-02 snapshot pattern** (confidence-refresh-batch.md): The alpha0/beta0 snapshot
   is taken ONCE before the ids_and_confs map, not inside the spawn_blocking loop.

9. **ADR-002 threshold** (empirical-prior-computation.md): `MINIMUM_VOTED_POPULATION = 10`
   is used (not 5). The zero-variance degeneracy guard returns cold-start defaults
   rather than propagating NaN.

10. **EC-03 u32 cast** (confidence-formula-engine.md): `helpful as f64` before
    arithmetic prevents overflow at `u32::MAX` in `helpfulness_score`.

## Open Questions for Implementation Agents

1. **R-11 outcome determines access doubling approach**: The implementation agent
   must run `test_store_duplicate_id_doubles_access_count` first. If the store
   deduplicates, switch to `record_usage_with_confidence` taking `&[(u64, u32)]`
   pairs. Pseudocode covers both branches; the branch selection is gated on the
   test result.

2. **record_usage_with_confidence exact new type**: The store layer owns this
   signature. The implementation agent must determine whether
   `Box<dyn Fn(&EntryRecord, u64) -> f64 + Send>` or `FnMut` is more appropriate
   given the store's implementation. The pseudocode specifies `Box<dyn Fn(...) + Send>`
   as the primary target.

3. **compute_observed_spread percentile method**: The pseudocode uses the
   nearest-rank method for p5/p95. If the codebase has an existing percentile
   utility, use it for consistency. Otherwise the nearest-rank approach is simple
   and sufficient.

## Knowledge Stewardship

- Queried: /uni-query-patterns for `unimatrix-engine` confidence scoring — found
  existing ADRs about W_COAC removal (crt-013, #705), two-mechanism co-access
  architecture (#706), and server spawn_blocking single-task pattern (vnc-010).
  No prior confidence formula procedures in active knowledge.
- Queried: /uni-query-patterns for `unimatrix-server` services layer patterns —
  found poison recovery pattern from `CategoryAllowlist` (documented in MEMORY.md),
  `UsageDedup` dedup-before-vote convention, fire-and-forget usage recording pattern.
- Deviations from established patterns: none. All pseudocode follows existing
  conventions: `unwrap_or_else` poison recovery, single `spawn_blocking` per operation,
  snapshot-before-spawn for prior capture.
