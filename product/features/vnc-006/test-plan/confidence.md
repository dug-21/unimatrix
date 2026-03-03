# Test Plan: ConfidenceService

## Unit Tests

### TS-11: Batch confidence recompute
- Setup: Create store with 3 entries
- Action: `confidence.recompute(&[id1, id2, id3])`
- Verify: All three entries have updated confidence values (non-zero)
- Note: Use tokio::time::sleep briefly to allow spawn_blocking to complete

### TS-12: Empty slice is no-op (FR-03.4)
- Action: `confidence.recompute(&[])`
- Verify: No panic, no spawn_blocking call (function returns immediately)

### TS-13: Non-existent entry logged and skipped (R-05)
- Setup: Create store with 1 entry
- Action: `confidence.recompute(&[entry_id, 999999])`
- Verify: Valid entry gets confidence updated
- Verify: No panic from missing entry (warn log emitted)
- Verify: Batch continues past failure

## Integration Test

### TS-13b: Confidence after insert+recompute
- Setup: Insert entry via StoreService
- Action: `confidence.recompute(&[entry_id])`
- Wait briefly for spawn_blocking
- Verify: Entry confidence is non-zero
