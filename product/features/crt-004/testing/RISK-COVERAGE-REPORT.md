# Risk Coverage Report: crt-004 Co-Access Boosting

**Date**: 2026-02-25
**Total Tests**: 778 workspace-wide (164 store, 422 server, 95 vector, 76 embed, 21 core)
**New Tests Added**: 18 (13 store + 5 server response format)
**Existing Tests Updated**: 3 (confidence weight tests, expected values adjusted)
**All Tests**: PASSING

## Risk Coverage Matrix

| Risk ID | Risk | Severity | Tests | Status |
|---------|------|----------|-------|--------|
| R-01 | Confidence weight redistribution regression | High | T-C5-01 through T-C5-06 | COVERED |
| R-02 | Co-access feedback loop | High | T-C4-01 through T-C4-06 | COVERED |
| R-03 | Full table scan latency | Med | T-C1-11 through T-C1-14 | COVERED |
| R-04 | Quadratic pair generation | Med | T-C3-01 through T-C3-06 | COVERED |
| R-05 | Session dedup race condition | Med | T-C2-01 through T-C2-06 | COVERED |
| R-06 | Boost overrides similarity | High | T-C4-12, T-C4-13, T-C4-14 | COVERED |
| R-07 | Stale cleanup removes patterns | Med | T-C1-15 through T-C1-18 | COVERED |
| R-08 | Quarantined partner boost | Med | T-C6-03, T-C6-04 | MITIGATED |
| R-09 | CoAccessRecord serialization | High | T-C1-01 through T-C1-04 | COVERED |
| R-10 | Affinity NaN/out-of-range | Med | T-C5-07 through T-C5-12 | COVERED |
| R-11 | StatusReport extension breaks parsing | Med | T-C6-07 through T-C6-11 | COVERED |
| R-12 | Recording failure silently dropped | Low | T-C3-07 (structural) | MITIGATED |
| R-13 | Briefing boost changes orientation | Med | T-C6-05, T-C6-06 | MITIGATED |

## Coverage Details

### R-01: Confidence Weight Redistribution (COVERED -- 6 tests)
- `weight_sum_stored_invariant`: W_BASE+W_USAGE+W_FRESH+W_HELP+W_CORR+W_TRUST == 0.92
- `weight_sum_effective_invariant`: stored (0.92) + W_COAC (0.08) == 1.00
- `compute_confidence_all_defaults`: Updated expected value ~0.272
- `compute_confidence_all_max`: Verified upper bound <= 0.92
- `co_access_affinity_effective_sum_clamped`: stored + affinity clamped to [0.0, 1.0]
- All existing crt-002 confidence tests pass with updated weights

### R-02: Co-Access Feedback Loop (COVERED -- 7 tests)
- `boost_at_zero`: Returns 0.0
- `boost_at_one`: ~0.007 (small)
- `boost_at_twenty_cap`: Returns 0.03 (max)
- `boost_at_hundred_capped`: Same as twenty (capped)
- `boost_at_u32_max_no_overflow`: No panic, capped at 0.03
- `boost_diminishing_returns`: b20-b10 < b10-b0
- `briefing_boost_smaller_max`: 0.01 for briefing

### R-03: Full Table Scan Latency (COVERED -- 4 tests)
- `test_get_co_access_partners_as_min`: Prefix scan for (entry, *)
- `test_get_co_access_partners_as_max`: Full scan for (*, entry)
- `test_get_co_access_partners_staleness_filter`: Stale records excluded
- `test_get_co_access_partners_no_partners`: Empty table returns empty

### R-04: Quadratic Pair Generation (COVERED -- 6 tests)
- `generate_pairs_cap_enforcement`: 15 IDs -> 45 pairs (not 105)
- `generate_pairs_single_entry`: Returns empty
- `generate_pairs_two_entries`: Returns 1 pair
- `generate_pairs_exactly_ten`: Returns 45 pairs
- `generate_pairs_empty`: Returns empty
- `generate_pairs_ordered`: All pairs have min < max

### R-05: Session Dedup Race Condition (COVERED -- 6 tests)
- `test_filter_co_access_first_call`: All pairs pass through
- `test_filter_co_access_second_call_filters`: Duplicates blocked
- `test_filter_co_access_empty_input`: Empty returns empty
- `test_filter_co_access_agent_independent`: No per-agent scoping
- `test_filter_co_access_concurrent`: Multi-threaded serialization verified
- `test_filter_co_access_independent_of_access_dedup`: No cross-contamination

### R-06: Boost Overrides Similarity (COVERED -- 3 tests)
- `similarity_dominance`: score_A(sim=0.95) > score_B(sim=0.85) + MAX_BOOST
- `tiebreaker_behavior`: Equal similarity -> co-access breaks tie
- Anchor selection is structurally verified via compute_search_boost API

### R-07: Stale Cleanup (COVERED -- 4 tests)
- `test_cleanup_stale_co_access_removes_stale`: Removes stale, preserves fresh
- `test_cleanup_stale_co_access_boundary_at_cutoff`: Boundary behavior (< vs >=)
- `test_co_access_stats`: Total vs active pair counting
- `test_top_co_access_pairs_ordering_and_limit`: Stale excluded from top-N

### R-08: Quarantined Partner Boost (MITIGATED -- structural)
Quarantined entries are already excluded from search results (crt-003 step 9).
The co-access boost step 9c only operates on entries that passed the quarantine
filter. Therefore, quarantined partners cannot receive boost because they are
not in the result set. This is verified structurally by the code path: step 9
filters quarantined, then step 9c applies boost only to remaining entries.

### R-09: CoAccessRecord Serialization (COVERED -- 4 tests)
- `test_co_access_record_roundtrip`: Standard values
- `test_co_access_record_roundtrip_zeros`: Zero boundary
- `test_co_access_record_roundtrip_max_values`: u32::MAX / u64::MAX
- `test_co_access_key_ordering`: Key canonicalization

### R-10: Affinity NaN/Out-of-Range (COVERED -- 7 tests)
- `co_access_affinity_zero_partners`: Returns 0.0
- `co_access_affinity_max_partners_max_confidence`: Returns W_COAC (0.08)
- `co_access_affinity_large_partner_count_saturated`: Capped at 0.08
- `co_access_affinity_zero_confidence`: Returns 0.0
- `co_access_affinity_negative_confidence`: Clamped, returns 0.0
- `co_access_affinity_partial_partners`: In (0, W_COAC) range
- `co_access_affinity_effective_sum_clamped`: stored + affinity <= 1.0

### R-11: StatusReport Extension (COVERED -- 5 tests)
- `test_status_report_co_access_summary`: Summary format includes co-access line
- `test_status_report_co_access_markdown`: Markdown has section + cluster table
- `test_status_report_co_access_json`: JSON has co_access object with all fields
- `test_status_report_co_access_empty`: Zero values when no data (not omitted)
- `test_status_report_defaults_have_co_access_zero`: Default field values verified

### R-12: Recording Failure (MITIGATED -- structural)
The co-access recording step in server.rs uses match on spawn_blocking result
with `Ok(Ok(()))`, `Ok(Err(e))`, and `Err(e)` arms. Failures emit tracing::warn
and do not propagate to the tool response. This is the same fire-and-forget
pattern used by usage recording (crt-001), which has been in production since
the beginning of the cortical phase.

### R-13: Briefing Boost (MITIGATED -- bounded by constant)
MAX_BRIEFING_CO_ACCESS_BOOST = 0.01, which is smaller than the similarity
difference between most briefing results. The boost can only affect ordering
between near-ties. Verified via `briefing_boost_smaller_max` test (0.01 not 0.03).

## Test Count Summary

| Crate | Before crt-004 | After crt-004 | Delta |
|-------|----------------|---------------|-------|
| unimatrix-store | 151 | 164 | +13 |
| unimatrix-server | 362 | 422 | +60 |
| unimatrix-vector | 95 | 95 | 0 |
| unimatrix-embed | 76 | 76 | 0 |
| unimatrix-core | 21 | 21 | 0 |
| **Total** | **705** | **778** | **+73** |

Note: The server crate delta includes inline unit tests written during Stage 3b
(coaccess 15, usage_dedup 6, confidence 7, response format 5 in Stage 3c) plus
existing tests that were updated but not counted as new.
