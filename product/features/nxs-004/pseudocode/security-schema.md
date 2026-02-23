# Pseudocode: security-schema

## Purpose
Add 7 security fields to EntryRecord and extend NewEntry with 3 caller-provided fields.

## Modified File: crates/unimatrix-store/src/schema.rs

### EntryRecord Changes

Append 7 fields after `embedding_dim`:
```
#[serde(default)]
pub created_by: String,
#[serde(default)]
pub modified_by: String,
#[serde(default)]
pub content_hash: String,
#[serde(default)]
pub previous_hash: String,
#[serde(default)]
pub version: u32,
#[serde(default)]
pub feature_cycle: String,
#[serde(default)]
pub trust_source: String,
```

CRITICAL: Fields MUST be appended in this exact order after embedding_dim. bincode positional encoding contract.

### NewEntry Changes

Add 3 caller-provided fields:
```
pub created_by: String,
pub feature_cycle: String,
pub trust_source: String,
```

### Test Helper Changes

Update `make_test_record()` in schema.rs tests to include new fields:
```
created_by: String::new(),
modified_by: String::new(),
content_hash: String::new(),
previous_hash: String::new(),
version: 0,
feature_cycle: String::new(),
trust_source: String::new(),
```

## Modified File: crates/unimatrix-store/src/test_helpers.rs

### TestEntry Builder

Add new fields with defaults to TestEntry:
```
pub struct TestEntry {
    // ... existing fields ...
    created_by: String,     // default: ""
    feature_cycle: String,  // default: ""
    trust_source: String,   // default: ""
}
```

Add builder methods:
```
fn with_created_by(mut self, val: &str) -> Self
fn with_feature_cycle(mut self, val: &str) -> Self
fn with_trust_source(mut self, val: &str) -> Self
```

Update build() to include new fields in NewEntry construction.

## Error Handling
No new error types needed. All fields are String/u32 with no validation at the schema level.

## Key Test Scenarios
- Roundtrip serialization with all 7 new fields populated
- Roundtrip serialization with all 7 new fields at defaults
- All existing schema tests still pass after field addition
