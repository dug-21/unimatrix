# C9: Test Infrastructure Test Plan

## AC-19: Reusable Test Infrastructure

### Structural Verification

- TestDb struct exists with new() constructor
- TestDb creates temp directory + database, auto-cleans on drop
- TestEntry builder exists with new(topic, category) constructor
- TestEntry supports .with_tags(), .with_status(), .with_content(), .with_source(), .with_title()
- TestEntry.build() returns NewEntry with sensible defaults
- assert_index_consistent() function exists and verifies all 6 index tables
- assert_index_absent() function exists
- seed_entries() function populates database with deterministic data

### Usage in Tests

- All test files in the crate use TestDb for database creation
- All test files use TestEntry for entry construction
- assert_index_consistent is used after every insert in R1/R2 test suites

### Feature Flag

- `test-support` feature makes test_helpers module public for downstream crates
