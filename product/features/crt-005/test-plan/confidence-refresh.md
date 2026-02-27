# Test Plan: C5 Confidence Refresh

## Component

C5: Confidence Refresh (`crates/unimatrix-server/src/tools.rs` -- context_status handler)

## Risks Covered

| Risk | Description | Priority |
|------|-------------|----------|
| R-08 | Confidence refresh batch overflow | Med |
| R-16 | Staleness detection false positives | Med |

## Integration Tests (tools.rs / server tests)

### IT-C5-01: Confidence refresh with maintain=true and stale entries
- Store 10 entries with old timestamps (older than 24h threshold)
- Call context_status with maintain=true
- Assert: confidence_refreshed_count > 0 (up to 10)
- Assert: refreshed entries have updated confidence values
- Covers: R-08 scenario 2 (under batch cap), AC-09

### IT-C5-02: Confidence refresh with 200 stale entries (batch cap)
- Store 200 entries with old timestamps
- Call context_status with maintain=true
- Assert: confidence_refreshed_count == 100 (MAX_CONFIDENCE_REFRESH_BATCH)
- Assert: exactly 100 entries had confidence updated, 100 remain stale
- Covers: R-08 scenario 1, AC-19

### IT-C5-03: Second call refreshes remaining entries
- After IT-C5-02, call context_status with maintain=true again
- Assert: confidence_refreshed_count == 100 (remaining from first 200)
- Covers: R-08 scenario 3

### IT-C5-04: Oldest entries refreshed first
- Store entries with varying staleness (1 day, 2 days, 5 days, 10 days)
- Call context_status with maintain=true, batch cap smaller than total stale
- Verify the oldest entries (10 days, 5 days) were refreshed first
- Covers: R-08 scenario 4

### IT-C5-05: maintain=false skips confidence refresh
- Store entries with old timestamps
- Call context_status with maintain=false (or omitted, default)
- Assert: confidence_refreshed_count == 0
- Assert: no entry confidence values changed
- Covers: R-07 scenario 1

### IT-C5-06: Dimension scores computed even with maintain=false
- Store entries (some stale)
- Call context_status with maintain=false
- Assert: stale_confidence_count > 0 (stale entries detected for scoring)
- Assert: confidence_freshness_score < 1.0
- Assert: confidence_refreshed_count == 0 (no writes)
- Covers: R-07 scenario 4

### IT-C5-07: Individual refresh failure does not abort batch
- This is verified at code review level -- the loop uses match/warn for individual failures
- If testable: simulate one entry deletion between scan and refresh
- Assert: confidence_refreshed_count == (total stale - 1) or less
- Assert: no panic or error returned from context_status
- Covers: FM-02

### IT-C5-08: Stale entries with last_accessed_at = 0
- Store entries with last_accessed_at=0, updated_at=old
- Assert: entries are identified as stale based on updated_at
- Covers: R-16

### IT-C5-09: No stale entries means zero refresh
- Store entries with very recent timestamps
- Call context_status with maintain=true
- Assert: confidence_refreshed_count == 0 (nothing to refresh)
- Assert: stale_confidence_count == 0

## Edge Cases

### EC-C5-01: All entries have confidence 0.0
- Matches EC-01 from RISK-TEST-STRATEGY
- Store entries with default confidence (0.0) and old timestamps
- Call context_status with maintain=true
- Assert: entries are refreshed with newly computed confidence values

### EC-C5-02: Single stale entry
- Store 1 entry with old timestamp
- Call context_status with maintain=true
- Assert: confidence_refreshed_count == 1

## Dependencies

- C1 (schema migration): EntryRecord.confidence is f64
- C2 (f64 scoring): compute_confidence returns f64
- C4 (coherence module): staleness threshold constant, stale detection logic
- C7 (maintenance parameter): maintain parameter must be wired

## Estimated Test Count

- 9-11 integration tests
