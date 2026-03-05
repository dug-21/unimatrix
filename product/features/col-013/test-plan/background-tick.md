# Test Plan: background-tick (Wave 3)

## Unit Tests

### T-BT-01: context_status ignores maintain=true (AC-16)
- Input: Call context_status handler with maintain=true
- Expected: No error, no maintenance operations triggered, valid report returned
- Location: crates/unimatrix-server/ (mcp/tools.rs handler test)

### T-BT-02: StatusReport includes last_maintenance_run (AC-16)
- Input: Call context_status after tick metadata populated
- Expected: Report includes last_maintenance_run field

### T-BT-03: StatusReport includes coherence_by_source (AC-19)
- Input: Store entries with different trust_sources ("human", "auto")
- Expected: coherence_by_source contains entries for each trust_source

### T-BT-04: ExtractionStatsResponse serializes correctly
- Input: ExtractionStats with known values
- Expected: JSON includes entries_extracted_total, rules_fired, etc.

## Integration Tests

### T-BT-05: Background tick fires (AC-13)
- Setup: Start server with short tick interval (1 second for test)
- Wait: 2 seconds
- Assert: tick_metadata.last_run is set (non-None)
- Note: Uses tokio::time::pause() to advance time

### T-BT-06: Maintenance runs in background (AC-14)
- Setup: Create stale entries, start tick
- Wait: For tick to fire
- Assert: Confidence refreshed for stale entries
- Note: Requires Store with entries older than staleness threshold

### T-BT-07: Extraction pipeline triggers on tick (AC-15)
- Setup: Insert synthetic observations, start tick
- Wait: For tick to fire
- Assert: New auto-extracted entry exists in store with trust_source="auto"

### T-BT-08: Near-duplicate quality gate (AC-07)
- Setup: Store entry "convention about testing patterns"
- Input: Propose near-identical entry
- Expected: Rejected (cosine >= 0.92)
- Note: Requires embedding service

### T-BT-09: Contradiction quality gate (AC-08)
- Setup: Store entry "Always use bincode"
- Input: Propose "Never use bincode"
- Expected: Rejected (contradiction detected)

### T-BT-10: Auto-entry metadata (AC-12)
- Setup: Run extraction pipeline with qualifying observations
- Assert: Stored entry has trust_source="auto", tags include "auto-extracted", "rule:{name}"

### T-BT-11: All existing tests pass after Wave 3 (AC-20d)
- Command: cargo test --workspace
- Expected: All tests pass

## Risk Coverage

| Risk | Tests |
|------|-------|
| R-02 (silent tick failure) | T-BT-02, T-BT-05 |
| R-01 (low-quality entries) | T-BT-08, T-BT-09, T-BT-10 |
| R-03 (CRT regressions) | T-BT-03 |
| R-04 (observation query perf) | T-BT-07 (implicit via watermark) |
