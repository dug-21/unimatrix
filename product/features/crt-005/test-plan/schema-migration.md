# Test Plan: C1 Schema Migration v2 -> v3

## Component

C1: Schema Migration (`crates/unimatrix-store/src/schema.rs`, `crates/unimatrix-store/src/migration.rs`)

## Risks Covered

| Risk | Description | Priority |
|------|-------------|----------|
| R-01 | Schema migration v2->v3 fails or produces corrupt entries | High |
| R-13 | V2EntryRecord struct field count or order mismatch | Critical |

## Unit Tests (migration.rs)

### UT-C1-01: V2EntryRecord roundtrip deserialization
- Serialize an EntryRecord with current v2 schema (confidence: f32)
- Deserialize using V2EntryRecord
- Assert all 26 fields match the original values
- Assert confidence is f32 type in V2EntryRecord
- Covers: R-13 scenarios 3-4

### UT-C1-02: V2EntryRecord field order matches bincode positional encoding
- Serialize EntryRecord with known field values using bincode
- Deserialize as V2EntryRecord
- Verify every field has the expected value (not shifted due to misaligned positions)
- Covers: R-13 scenario 2

### UT-C1-03: V2EntryRecord with serde(default) fields
- Serialize an EntryRecord where helpful_count=0, unhelpful_count=0, confidence=0.0
- Deserialize as V2EntryRecord
- Assert default fields are correctly read
- Covers: R-13 scenario 5

## Integration Tests (migration.rs)

### IT-C1-01: Migration with known f32 confidence values
- Create a v2 database using `create_v2_database` helper
- Insert entries with confidence values: 0.5, 0.85, 0.99, 0.0
- Run migrate_if_needed
- Read all entries back
- Assert confidence == original_f32 as f64 for each entry
- Assert schema_version == 3
- Covers: R-01 scenarios 1, 4

### IT-C1-02: Migration with f32 boundary confidence values
- Create v2 database with entries having:
  - confidence = 0.0 (default)
  - confidence = f32::MIN_POSITIVE
  - confidence = 1.0 - f32::EPSILON
  - confidence = f32::EPSILON
- Run migration
- Assert each f64 value == original_f32 as f64 (IEEE 754 lossless)
- Covers: R-01 scenario 3

### IT-C1-03: Migration of empty database
- Create v2 database with no entries
- Run migrate_if_needed
- Assert schema_version == 3
- Assert no error
- Covers: R-01 scenario 7

### IT-C1-04: Migration idempotency
- Create v2 database, insert entries, run migration
- Call migrate_if_needed a second time
- Assert no error, no changes to entries
- Assert schema_version still 3
- Covers: R-01 scenario 5

### IT-C1-05: Full migration chain v0 -> v1 -> v2 -> v3
- Create a v0 database using `create_legacy_database` helper
- Call migrate_if_needed (should run all 3 migrations)
- Assert schema_version == 3
- Assert all entries readable with f64 confidence
- Covers: R-01 scenario 6

### IT-C1-06: create_v2_database test helper
- Create a helper function for test setup that:
  - Opens a fresh redb database
  - Sets schema_version = 2
  - Writes entries with f32 confidence using the V2EntryRecord struct
- Used by all C1 integration tests

## Edge Cases

### EC-C1-01: Entry with confidence 0.0 (pre-crt-002 entries)
- Matches EC-01 from RISK-TEST-STRATEGY
- Verify 0.0_f32 as f64 == 0.0_f64 exactly

### EC-C1-02: Database with thousands of entries
- Not required at current scale
- Verify migration loop handles at least 100 entries without issue

## Assertions

- All f64 comparisons use exact equality (IEEE 754 guarantees f32 as f64 is lossless)
- Schema version checked via COUNTERS table read
- Entry count verified before and after migration (unchanged)

## Dependencies

- None (C1 is first in build order)

## Estimated Test Count

- 3 unit tests
- 6 integration tests (including helper)
- ~9 total new tests
