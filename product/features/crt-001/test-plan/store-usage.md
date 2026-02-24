# Test Plan: C3 Store Usage Methods

## Risk Coverage: R-02 (Counter update atomicity)

### T-C3-01: record_usage 5 entries all updated (R-02 scenario 1)
- Call record_usage with 5 entry IDs as both all_ids and access_ids
- Verify all 5 have access_count=1 and last_accessed_at > 0
- Verifies: AC-05, AC-14

### T-C3-02: record_usage overlapping sets (R-02 scenario 2)
- all_ids=[1,2,3], access_ids=[1,2], helpful_ids=[2,3]
- Verify entry 1: access_count=1, helpful_count=0
- Verify entry 2: access_count=1, helpful_count=1
- Verify entry 3: access_count=0, helpful_count=1

### T-C3-03: record_usage non-existent entry (R-02 scenario 3)
- Call with a mix of valid and non-existent entry IDs
- Verify valid entries updated, non-existent skipped, no error
- Verifies: AC-14 (graceful degradation)

### T-C3-04: record_usage empty all_ids (R-02 scenario 4)
- Call record_usage with empty all_ids
- Verify no error, no transaction opened

### T-C3-05: record_usage cumulative increments (R-02 scenario 5)
- Call record_usage twice for the same entries
- Verify access_count=2 (store has no dedup)

### T-C3-06: record_usage preserves other fields (R-02 scenario 6)
- After record_usage, verify title, content, topic, tags, content_hash, version unchanged

## Risk Coverage: R-07 (last_accessed_at staleness)

### T-C3-07: last_accessed_at always updated (R-07 scenario 1,2)
- Call record_usage with entry in all_ids but NOT in access_ids
- Verify last_accessed_at updated even without access_count increment

## Risk Coverage: R-14 (record_usage partial batch)

### T-C3-08: Partial batch with deleted entries (R-14 scenario 1)
- Create entries 1,2,3. Delete entry 2. Call record_usage with [1,2,3]
- Verify entries 1 and 3 updated, no error

### T-C3-09: All non-existent entries (R-14 scenario 2)
- Call record_usage with IDs [997,998,999] (none exist)
- Verify no error

### T-C3-10: Vote correction in record_usage (R-16 scenarios)
- Call record_usage with helpful_ids=[1], decrement_unhelpful_ids=[1]
- Verify helpful_count=1, unhelpful_count=0 (was 1 before)
- Verifies: AC-16

### T-C3-11: Saturating subtraction (R-16 scenario 4)
- Entry with helpful_count=0
- Call record_usage with decrement_helpful_ids=[id]
- Verify helpful_count remains 0 (no underflow)

## Risk Coverage: R-04 (FEATURE_ENTRIES orphan writes)

### T-C3-12: record_feature_entries valid entries (R-04 scenario 1)
- Call record_feature_entries("crt-001", [1,2,3])
- Read multimap, verify all 3 present under "crt-001"
- Verifies: AC-08 (store level)

### T-C3-13: record_feature_entries idempotency (R-04 scenario 2, AC-09)
- Call record_feature_entries("crt-001", [1]) twice
- Verify only one entry in multimap

### T-C3-14: record_feature_entries non-existent entry (R-04 scenario 3)
- Call with entry_id=999 (does not exist in ENTRIES)
- Verify it's still inserted (multimap doesn't validate)

### T-C3-15: record_feature_entries empty feature string (R-04 scenario 4)
- Call with feature=""
- Verify it works (empty string is valid key)

### T-C3-16: record_feature_entries query (R-04 scenario 5)
- Insert entries for feature "crt-001", query multimap
- Verify all linked entry IDs returned

### T-C3-17: record_feature_entries empty entry_ids
- Call with empty entry_ids slice
- Verify no error, no transaction opened
