# C2: Schema Test Plan

## R4/AC-16: Schema Evolution (WRITE FIRST)

### test_schema_evolution_reduced_struct
- Create a "v1" reduced struct with only core fields (no serde(default) fields)
- Serialize it with bincode::serde::encode_to_vec using standard() config
- Deserialize as full EntryRecord
- Assert: confidence == 0.0, last_accessed_at == 0, access_count == 0, supersedes == None, superseded_by == None, correction_count == 0, embedding_dim == 0

### test_schema_evolution_old_bytes_new_fields
- Serialize a full EntryRecord
- Deserialize into the same struct (simulating "new code reads old data")
- All existing fields preserved

## R3/AC-02: Serialization Round-Trip

### test_roundtrip_all_fields_populated
- Create EntryRecord with all fields set to non-default values
- Serialize, deserialize, assert ==

### test_roundtrip_empty_strings
- title = "", content = "", topic = "", category = "", source = ""
- Round-trip preserves empty strings

### test_roundtrip_empty_tags
- tags = vec![]
- Round-trip preserves empty vec

### test_roundtrip_f32_edge_values
- confidence: 0.0, 1.0, f32::MIN_POSITIVE, 0.999999
- Each round-trips correctly

### test_roundtrip_u64_boundary_values
- id: 0, 1, u64::MAX - 1, u64::MAX
- created_at, updated_at with boundary values

### test_roundtrip_option_none_and_some
- supersedes: None vs Some(42)
- superseded_by: None vs Some(99)

### test_roundtrip_all_status_variants
- Status::Active, Status::Deprecated, Status::Proposed
- Each serializes/deserializes correctly

### test_roundtrip_large_content
- content = 100KB string
- Round-trip preserves exactly

### test_roundtrip_unicode
- topic = unicode, content with emoji and CJK chars
- Round-trip preserves byte-exact

## Status Conversions

### test_status_try_from_valid
- 0u8 -> Active, 1u8 -> Deprecated, 2u8 -> Proposed

### test_status_try_from_invalid
- 3u8, 255u8 -> StoreError::InvalidStatus
