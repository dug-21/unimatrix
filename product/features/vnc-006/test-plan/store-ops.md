# Test Plan: StoreService

## Unit Tests

### TS-04: Atomic insert with audit (AC-05, R-02)
- Setup: Create StoreService with real Store
- Action: Insert entry via `store_ops.insert(entry, None, &mcp_ctx)`
- Verify: Entry exists in ENTRIES table
- Verify: Audit record exists in AUDIT_LOG
- Verify: Both are in the same committed transaction (read both in single read txn)

### TS-04b: Failed insert produces no partial state (R-02)
- Setup: Create StoreService
- Action: Trigger insert failure (e.g., invalid entry data that causes serialization error)
- Verify: No entry written, no audit written
- Note: May be difficult to trigger; could test with a crafted scenario

### TS-04c: Near-duplicate detection
- Setup: Insert entry A, then insert entry B with near-identical content
- Action: `store_ops.insert(entry_b, None, &ctx)`
- Verify: Returns `InsertResult { duplicate_of: Some(a_id), entry: existing_entry }`
- Verify: No new entry created

## Integration Tests

### TS-04d: Correct operation atomicity
- Setup: Insert original entry
- Action: `store_ops.correct(original_id, corrected_entry, reason, &ctx)`
- Verify: Original deprecated (status = Deprecated, superseded_by set)
- Verify: New correction created (supersedes set)
- Verify: Audit record for correction exists

### TS-04e: Correct on non-existent entry
- Action: `store_ops.correct(999999, entry, reason, &ctx)`
- Verify: Returns `Err(ServiceError::NotFound(999999))`

### TS-04f: StoreService validates write content (S1)
- Action: `store_ops.insert(entry_with_injection, None, &mcp_ctx)`
- Verify: Returns `Err(ServiceError::ContentRejected { ... })`
- Verify: No entry written
