# Test Plan: C4 EntryStore Trait Extension

## Risk Coverage: R-05 (Trait object safety)

### T-C4-01: Object safety compile check -- &dyn (R-05 scenario 1)
- fn _check(_: &dyn EntryStore) {} must compile
- Verifies: AC-11

### T-C4-02: Object safety compile check -- Arc<dyn> (R-05 scenario 2)
- fn _check(_: Arc<dyn EntryStore>) {} must compile

### T-C4-03: record_access through async wrapper (R-05 scenario 3)
- Create AsyncEntryStore wrapping StoreAdapter
- Call record_access(&[1, 2, 3]).await
- Verify entries have access_count=1, last_accessed_at > 0

## StoreAdapter Delegation

### T-C4-04: StoreAdapter record_access delegates correctly
- Insert 3 entries
- Call adapter.record_access(&[1, 2, 3])
- Verify all 3 have access_count=1, last_accessed_at > 0

### T-C4-05: record_access error propagation
- Call record_access with non-existent entry IDs
- Verify graceful handling (record_usage skips missing entries)
