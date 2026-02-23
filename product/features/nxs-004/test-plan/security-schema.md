# Test Plan: security-schema

## Scope

Verify EntryRecord has 7 new fields, NewEntry has 3 new fields, serialization roundtrip works, and existing tests pass.

## Unit Tests (in crates/unimatrix-store/src/schema.rs)

### test_roundtrip_security_fields_populated
- Create an EntryRecord with all 7 new fields set to non-default values:
  - created_by = "agent-1", modified_by = "agent-2", content_hash = "abc123...", previous_hash = "def456...", version = 3, feature_cycle = "nxs-004", trust_source = "agent"
- Serialize with `serialize_entry()`, deserialize with `deserialize_entry()`.
- Assert all 24 fields match original values exactly.

### test_roundtrip_security_fields_defaults
- Create an EntryRecord with all 7 new fields at defaults:
  - created_by = "", modified_by = "", content_hash = "", previous_hash = "", version = 0, feature_cycle = "", trust_source = ""
- Serialize, deserialize. Assert all fields match.

### test_new_entry_extended_fields
- Construct a NewEntry with created_by = "agent-1", feature_cycle = "nxs-004", trust_source = "human".
- Assert fields are accessible and hold correct values (compile + runtime check).

## Integration Tests

### test_existing_store_tests_pass
- `cargo test -p unimatrix-store` -- all 85 existing tests pass.
- This validates the TestEntry builder updates and EntryRecord field additions don't break anything.

### test_existing_vector_tests_pass
- `cargo test -p unimatrix-vector` -- all 85 tests pass.
- VectorIndex tests create NewEntry instances via TestEntry builder.

### test_existing_embed_tests_pass
- `cargo test -p unimatrix-embed` -- all 76 tests pass.
- Embed tests don't use NewEntry but this confirms no workspace breakage.

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-07 | test_new_entry_extended_fields, test_existing_store_tests_pass, test_existing_vector_tests_pass |
| R-12 | test_existing_store_tests_pass, test_existing_vector_tests_pass, test_existing_embed_tests_pass |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-05 | test_roundtrip_security_fields_populated, test_roundtrip_security_fields_defaults |
| AC-06 | test_new_entry_extended_fields |
| AC-14 | test_existing_store_tests_pass |
| AC-15 | test_existing_vector_tests_pass |
| AC-16 | test_existing_embed_tests_pass |
| AC-17 | test_roundtrip_security_fields_populated |
