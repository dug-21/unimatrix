# Test Plan: C1 Schema Extension

## Risk Coverage: R-06 (bincode positional encoding)

### T-C1-01: Roundtrip with helpful_count and unhelpful_count (R-06 scenario 1)
- Serialize EntryRecord with helpful_count=42, unhelpful_count=7
- Deserialize and verify values match
- Verifies: AC-02

### T-C1-02: Roundtrip with default fields (R-06 scenario 2)
- Serialize with all default fields
- Verify helpful_count=0, unhelpful_count=0 after deserialize

### T-C1-03: Roundtrip all fields populated (R-06 scenario 3)
- Serialize a record with ALL fields non-default
- Verify full roundtrip preserves all 26 fields

### T-C1-04: V1 bytes cannot deserialize as v2 (R-06 scenario 4)
- Serialize a 24-field record (V1 layout -- no helpful/unhelpful counts)
- Attempt to deserialize as current EntryRecord
- Verify deserialization fails (confirms positional encoding requires migration)

## FEATURE_ENTRIES Table

### T-C1-05: FEATURE_ENTRIES table created on Store::open (AC-01)
- Open a new store
- begin_read().open_multimap_table(FEATURE_ENTRIES) succeeds

### T-C1-06: 11 tables exist after open
- Open a new store
- Verify all 11 tables accessible via begin_read()

## Existing Test Compatibility

### T-C1-07: All existing schema tests pass with new fields
- Existing roundtrip tests updated to include helpful_count/unhelpful_count
- No behavioral change to existing functionality
