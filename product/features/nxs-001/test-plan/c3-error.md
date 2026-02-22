# C3: Error Test Plan

## R12/AC-15: Error Type Discrimination

### test_error_display_entry_not_found
- StoreError::EntryNotFound(42).to_string() contains "42"

### test_error_display_invalid_status
- StoreError::InvalidStatus(99).to_string() contains "99"

### test_error_display_serialization
- StoreError::Serialization("bad data".into()).to_string() contains "bad data"

### test_error_display_deserialization
- StoreError::Deserialization("corrupt".into()).to_string() contains "corrupt"

### test_error_is_std_error
- Verify StoreError implements std::error::Error (compile-time check + runtime source() calls)

### test_error_from_redb_types
- Verify From impls compile and produce correct variants (compile-time verification)
