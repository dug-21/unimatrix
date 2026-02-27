# crt-004 Acceptance Criteria Map

| AC-ID | Description | Verification Method | Verification Detail | Status |
|-------|-------------|--------------------|--------------------|--------|
| AC-01 | CO_ACCESS redb table exists with (u64, u64) -> &[u8] schema, ordered keys | test | Unit test: open table, insert ordered pair, read back | PENDING |
| AC-02 | CoAccessRecord contains count: u32 and last_updated: u64, serializable via bincode serde path | test | Unit test: serialize_co_access / deserialize_co_access roundtrip at boundary values | PENDING |
| AC-03 | Co-access pairs recorded when result set contains 2+ entries | test | Integration test: search returns 3 entries, verify 3 pairs in CO_ACCESS table | PENDING |
| AC-04 | Pair generation capped at MAX_CO_ACCESS_ENTRIES (default: 10) | test | Unit test: generate_pairs with 15 IDs -> 45 pairs (from first 10 only) | PENDING |
| AC-05 | Ordered keys (min(a,b), max(a,b)) deduplicate symmetric pairs | test | Unit test: co_access_key(5,3) == co_access_key(3,5) == (3,5) | PENDING |
| AC-06 | Co-access count incremented atomically on re-encounter | test | Integration test: record_co_access_pairs for same pair twice, verify count=2 | PENDING |
| AC-07 | last_updated timestamp set on every increment | test | Integration test: record pair, record again, verify last_updated >= first recording | PENDING |
| AC-08 | Session dedup prevents same agent from inflating co-access counts | test | Unit test: filter_co_access_pairs returns pair first time, empty second time | PENDING |
| AC-09 | context_search applies co-access boost after similarity+confidence re-ranking | test | Integration test: seed CO_ACCESS with known pairs, search, verify boosted entry moved up in results | PENDING |
| AC-10 | Co-access boost is additive and capped at MAX_CO_ACCESS_BOOST | test | Unit test: boost formula at count=100 returns MAX_CO_ACCESS_BOOST (0.03) | PENDING |
| AC-11 | Top result(s) used as anchors for co-access boost | test | Unit test: compute_search_boost with 3 anchors, verify only anchor partners receive boost | PENDING |
| AC-12 | Stale pairs excluded from boost calculations (default: 30 days) | test | Integration test: seed pair with old last_updated, verify excluded from compute_search_boost | PENDING |
| AC-13 | context_status reports total_co_access_pairs, active_co_access_pairs, top clusters | test | Integration test: seed CO_ACCESS pairs, call context_status, verify stats in response | PENDING |
| AC-14 | Confidence formula expanded with co-access factor, weights sum preserved | test | Unit test: W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 0.92; W_COAC == 0.08; total == 1.00 | PENDING |
| AC-15 | Weight redistribution preserves sum-to-1.0 invariant | test | Unit test: stored weights (0.92) + co-access weight (0.08) == 1.00 | PENDING |
| AC-16 | Co-access affinity computed from partner count and avg confidence | test | Unit test: co_access_affinity(10, 0.8) returns expected value in [0.0, 0.08] | PENDING |
| AC-17 | context_briefing applies direct co-access boost with very small weight (0.01) | test | Integration test: seed CO_ACCESS, call context_briefing, verify boost applied to entry ordering | PENDING |
| AC-18 | Stale pairs cleaned up during context_status | test | Integration test: seed stale pairs, call context_status, verify pairs removed from CO_ACCESS | PENDING |
| AC-19 | All new code has unit tests; integration tests verify recording, dedup, boost, staleness | test | cargo test -- verify new test count increase | PENDING |
| AC-20 | Existing tests pass (no regressions from weight redistribution) | test | cargo test -- all crt-002 confidence tests pass with updated expected values | PENDING |
| AC-21 | Co-access recording is fire-and-forget | grep | Verify spawn_blocking usage in record_usage_for_entries co-access step; no .await on result before tool response | PENDING |
| AC-22 | #![forbid(unsafe_code)], no new crate dependencies | shell | cargo build; diff Cargo.toml for no new deps; compiler enforces forbid(unsafe_code) | PENDING |
