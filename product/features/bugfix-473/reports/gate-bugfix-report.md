# Gate Bugfix Report: bugfix-473

> Gate: Bugfix Validation
> Date: 2026-03-31
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Root cause addressed | PASS | Fix replaces shared `remaining_capacity` with independent `MAX_INFORMS_PER_TICK = 25` constant |
| No placeholders/stubs | PASS | No todo!, unimplemented!, TODO, FIXME found |
| Tests pass | PASS | All 5 new Phase 5 tests pass; 2583 unimatrix-server tests pass; col018 flakes are pre-existing embedding init failures |
| No new clippy warnings | PASS | 0 warnings/errors in nli_detection_tick.rs; 58 pre-existing errors in unimatrix-observe (unrelated, not introduced here) |
| No unsafe code | PASS | grep confirms zero `unsafe` in nli_detection_tick.rs |
| Fix is minimal | PASS | All 223 diff lines confined to nli_detection_tick.rs; no other files changed |
| Tests would have caught original bug | PASS | test_phase5_informs_always_gets_dedicated_budget directly encodes the broken invariant |
| Integration smoke tests | PASS | 22/22 smoke, 13/13 contradiction, 41/41 lifecycle (2 pre-existing xfails with GH#406/GH#291) |
| xfail markers have GH issues | PASS | Both xfails cite GH#406 and GH#291; pre-existing, unrelated to fix |
| Knowledge stewardship — investigator | N/A | No separate investigator report; fix-agent (473-agent-1) includes Queried + Stored entries |
| Knowledge stewardship — rust-dev | PASS | 473-agent-1-fix-report.md: Queried (briefing, entry #3955, #3937) + Stored (entry #3969 pattern) |
| Knowledge stewardship — tester | PASS | 473-agent-2-verify-report.md: Queried (briefing, 20 entries) + Stored nothing novel with reason |

## Detailed Findings

### Root Cause Addressed

**Status**: PASS

**Evidence**: The diagnosed root cause was `remaining_capacity = max_graph_inference_per_tick.saturating_sub(candidate_pairs.len())` in Phase 5, which zeroed out the Informs budget whenever Supports filled the shared cap. The fix removes this computation entirely and replaces it with:

```rust
const MAX_INFORMS_PER_TICK: usize = 25;
// ...
informs_metadata.shuffle(&mut rng);
informs_metadata.truncate(MAX_INFORMS_PER_TICK);
```

Supports and Informs now have completely independent caps. The log field `informs_candidates_accepted` now correctly reports non-zero values when Informs candidates exist.

The old cosine-sorted truncation is also removed, replaced with random shuffle (same pattern as `select_source_candidates`) to prevent deterministic re-selection across ticks.

### No Placeholders or Stubs

**Status**: PASS

**Evidence**: `grep -n "unsafe|todo!|unimplemented!|TODO|FIXME"` in nli_detection_tick.rs returns no matches.

### Tests Pass

**Status**: PASS

**Evidence**:
- All 5 new Phase 5 tests pass: `test_phase5_informs_always_gets_dedicated_budget`, `test_phase5_informs_small_pool_all_kept`, `test_phase5_informs_empty_pool_stays_empty`, `test_phase5_informs_shuffle_no_duplicates_valid_ids`, `test_phase5_informs_log_accounting_consistent`.
- Full `services::nli_detection_tick::tests` module: 51 passed, 0 failed.
- Full `cargo test --workspace`: clean run with 0 failures across all result lines (col018 embedding init flakes are pre-existing and transient — agent-1-fix-report confirms, reproduced once in validator run, passed on retry; root cause is embedding model initialization race in test environment, not this fix).

### No New Clippy Warnings

**Status**: PASS

**Evidence**: `cargo clippy -p unimatrix-server -- -D warnings` produces no output for nli_detection_tick.rs. The 58 errors in unimatrix-observe are pre-existing on main (confirmed by agent-2-verify-report: "present on `main` before the fix was applied").

### No Unsafe Code

**Status**: PASS

**Evidence**: `grep "unsafe" nli_detection_tick.rs` returns no output. No unsafe blocks were introduced.

### Fix is Minimal

**Status**: PASS

**Evidence**: `git diff main -- crates/unimatrix-server/src/services/nli_detection_tick.rs` confirms 223 lines changed, all within the single file `nli_detection_tick.rs`. No other files were modified by this fix.

### Tests Would Have Caught Original Bug

**Status**: PASS

**Evidence**: `test_phase5_informs_always_gets_dedicated_budget` constructs exactly the failure scenario (Supports fills its cap, Informs pool has MAX_INFORMS_PER_TICK + 10 entries) and asserts `informs.len() == MAX_INFORMS_PER_TICK`. The old code would produce `informs.len() == 0` here, failing the test. The 6 old tests that encoded the broken shared-cap behavior were correctly removed.

### Integration Smoke Tests

**Status**: PASS

**Evidence**: Per RISK-COVERAGE-REPORT.md and agent-2-verify-report.md:
- smoke: 22/22 PASS
- contradiction: 13/13 PASS
- lifecycle: 41 PASS, 2 xfailed (GH#406, GH#291), 1 xpassed (pre-existing self-heal, no action required)

### xfail Markers Have GH Issues

**Status**: PASS

**Evidence**: Both xfail entries in the lifecycle suite cite documented pre-existing issues:
- GH#406: multi-hop traversal
- GH#291: tick interval not drivable

Both were present before this fix and are unrelated to Phase 5 Informs budgeting.

### Knowledge Stewardship — rust-dev (473-agent-1)

**Status**: PASS

**Evidence**: `product/features/bugfix-473/agents/473-agent-1-fix-report.md` contains:
```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced entry #3955 (ADR-002 crt-037) and entry #3937
- Stored: entry #3969 "Never use remaining_capacity = cap - high_priority.len() as the low-priority type's budget in a shared tick cap" via /uni-store-pattern
```

### Knowledge Stewardship — tester (473-agent-2)

**Status**: PASS

**Evidence**: `product/features/bugfix-473/agents/473-agent-2-verify-report.md` contains:
```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — returned 20 entries covering NLI detection tick patterns
- Stored: nothing novel to store — direct application of existing pattern (#3949)
```
Reason given for not storing is specific and valid.

## Rework Required

None.

## Knowledge Stewardship

- Stored: nothing novel to store — this is a straightforward shared-cap bug; the systemic pattern (never steal low-priority budget from high-priority fill in a shared cap) was already captured by 473-agent-1 as entry #3969. No additional gate-level pattern warranted.
