# Agent Report: col-028-agent-3-risk (Architecture-Risk)

## Output

- Produced: `product/features/col-028/RISK-TEST-STRATEGY.md`

## Risk Summary

| Priority | Count | Risks |
|----------|-------|-------|
| Critical | 3 | R-01 (D-01 dedup collision), R-02 (positional column drift), R-03 (phase snapshot race) |
| High | 6 | R-04 (dual get_state), R-05 (schema cascade), R-06 (UDS compile), R-07 (context_get weight), R-08 (briefing weight), R-09 (test helper compile) |
| Medium | 5 | R-10 through R-13, plus IR-01 through IR-04 integration risks |
| Low | 4 | R-14 through R-16, EC edge cases |

**Total**: 16 risks identified, 24 acceptance criteria traced, 7 scope risks fully mapped.

## Top Risks Requiring Careful Test Coverage

**R-01 (Critical)**: The D-01 dedup collision is the highest-risk correctness issue in this feature. Patterns #3503 and #3510 confirm it is real and previously documented. The test strategy requires a negative test (verify guard is load-bearing, not redundant) in addition to the AC-07 positive scenario. The briefing-then-get sequence must be tested end-to-end through UsageService, not in isolation.

**R-02 (Critical)**: Positional column drift across the four-site atomic change unit (analytics.rs INSERT, two SELECTs, row_to_query_log) produces silent runtime data corruption with no compile signal. AC-17 round-trip test is the only automated guard. Failure mode FM-04 is particularly subtle: reading column 8 (source) as phase returns a plausible-looking string without panicking.

**R-05 (High, Likelihood=High)**: Schema version cascade across migration_v15_to_v16.rs and server.rs is a recurring miss (pattern #2933). AC-22 grep check (`grep -r 'schema_version.*== 16' crates/`) must be a mandatory gate step, not optional.

## Delivery Ordering Note

Part 2 (schema changes to QueryLogRecord::new signature) must be completed before or in the same commit as Part 1 call sites that pass phase to QueryLogRecord::new. The UDS compile fix (AC-23) is a blocker for workspace compilation and has no dependencies — it should be done first.

## Knowledge Stewardship

- Queried: /uni-knowledge-search for `"lesson-learned failures gate rejection"` — no directly applicable lessons found for this domain.
- Queried: /uni-knowledge-search for `"risk pattern"` category:pattern — found #2933 (schema version cascade), applied to R-05.
- Queried: /uni-knowledge-search for `"UsageDedup weight-0 dedup slot"` — found #3503, #3510; elevated R-01 to Critical.
- Queried: /uni-knowledge-search for `"SQLite migration schema version cascade"` — found #2933, #2937; directly applied to AC-22 gate requirement.
- Queried: /uni-knowledge-search for `"analytics drain phase-snapshot integration test"` — found #3004; applied to IR-02 and R-10 coverage requirement.
- Stored: nothing novel to store — all relevant patterns already exist in Unimatrix (#3503, #3510, #2933, #3004).
