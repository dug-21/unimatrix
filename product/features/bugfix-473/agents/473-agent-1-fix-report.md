# Agent Report: 473-agent-1-fix

**Feature:** bugfix-473
**Task:** Fix Phase 5 of `run_graph_inference_tick` — give Informs an independent per-tick budget

## Changes Made

### File Modified
`crates/unimatrix-server/src/services/nli_detection_tick.rs`

### Change 1: Module-level constant
Added `MAX_INFORMS_PER_TICK: usize = 25` after the `use` block, with doc comment explaining the rationale (internal throughput knob, not operator-tunable, same justification as MAX_SOURCES_PER_TICK).

### Change 2: Phase 5 logic replacement
Removed:
- `remaining_capacity` computation (`max_graph_inference_per_tick.saturating_sub(candidate_pairs.len())`)
- Cosine-sort on `informs_metadata`
- `informs_metadata.truncate(remaining_capacity)`

Added:
- Random shuffle via `rand::seq::SliceRandom` + `rand::rng()` (same pattern as `select_source_candidates` in `nli_detection.rs`)
- `informs_metadata.truncate(MAX_INFORMS_PER_TICK)`
- `informs_kept` variable to keep log line correct: `dropped = total - kept`

### Change 3: Tests replaced
Removed 6 tests encoding the broken shared-cap behavior. Added 5 tests asserting the correct invariant.

## New Tests

| Function | Invariant Asserted |
|---|---|
| `test_phase5_informs_always_gets_dedicated_budget` | Informs gets MAX_INFORMS_PER_TICK even when Supports fills its cap completely |
| `test_phase5_informs_small_pool_all_kept` | Pool smaller than budget — all candidates kept |
| `test_phase5_informs_empty_pool_stays_empty` | Empty pool stays empty after shuffle+truncate |
| `test_phase5_informs_shuffle_no_duplicates_valid_ids` | No duplicate pairs; all IDs come from original pool |
| `test_phase5_informs_log_accounting_consistent` | accepted + dropped == total (SR-03); kept == MAX_INFORMS_PER_TICK |

## Replaced Tests

| Old Name | Disposition |
|---|---|
| `test_phase5_supports_fills_cap_zero_informs_accepted` | Removed — asserted the broken behavior (Informs=0 when Supports fills cap) |
| `test_phase5_partial_cap_informs_fills_remainder` | Removed — asserted cosine-sort+remainder logic |
| `test_phase5_no_supports_all_informs_up_to_cap` | Removed — tested shared-cap path |
| `test_phase5_merged_len_never_exceeds_max_cap_property` | Removed — property of shared-cap invariant |
| `test_phase5_cap_zero_produces_empty_merged` | Removed — tested shared-cap edge case |
| `test_phase5_remaining_computed_after_truncation` | Removed — tested `remaining_capacity` that no longer exists |

## Test Results

- `cargo test -p unimatrix-server`: **2583 passed, 0 failed**
- 5 new Phase 5 tests: all pass
- Pre-existing failures: none introduced (one flaky embedding init test in col018 was pre-existing and disappeared on rerun)

## Issues

None. Fix was clean and confined to Phase 5 as specified.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — surfaced entry #3955 (ADR-002 crt-037, the original shared-cap design decision) and entry #3937 (nli_detection_tick.rs pattern). Entry #3955 documents the now-superseded shared-cap design. The fix supersedes that ADR.
- Stored: entry #3969 "Never use remaining_capacity = cap - high_priority.len() as the low-priority type's budget in a shared tick cap" via /uni-store-pattern
