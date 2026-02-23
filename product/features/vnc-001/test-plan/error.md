# Test Plan: error.rs

## Risks Covered
- R-13: Error responses leak internal details (High)

## Unit Tests

### ServerError to ErrorData mapping

```
test_entry_not_found_maps_to_32001
  Arrange: ServerError::Core(CoreError::Store(StoreError::EntryNotFound(42)))
  Act: convert to ErrorData
  Assert: code == -32001, message contains "42", message contains "Verify the ID"

test_core_error_maps_to_32603
  Arrange: ServerError::Core(CoreError::Store(StoreError::Transaction(...)))
  Act: convert to ErrorData
  Assert: code == -32603, message contains "Internal storage error"

test_capability_denied_maps_to_32003
  Arrange: ServerError::CapabilityDenied { agent_id: "test", capability: Write }
  Act: convert to ErrorData
  Assert: code == -32003, message contains "test", message contains "Write"

test_embed_not_ready_maps_to_32004
  Arrange: ServerError::EmbedNotReady
  Act: convert to ErrorData
  Assert: code == -32004, message contains "context_lookup"

test_embed_failed_maps_to_32004
  Arrange: ServerError::EmbedFailed("download error".into())
  Act: convert to ErrorData
  Assert: code == -32004, message contains "download error"

test_not_implemented_maps_to_32005
  Arrange: ServerError::NotImplemented("context_search".into())
  Act: convert to ErrorData
  Assert: code == -32005, message contains "vnc-002"

test_registry_error_maps_to_32603
  Arrange: ServerError::Registry("table corrupted".into())
  Act: convert to ErrorData
  Assert: code == -32603

test_audit_error_maps_to_32603
  Arrange: ServerError::Audit("disk full".into())
  Act: convert to ErrorData
  Assert: code == -32603
```

### Display trait

```
test_display_no_rust_types
  For each ServerError variant:
    Act: format!("{}", error)
    Assert: does NOT contain "StoreError", "CoreError", or other Rust type names
```

### From trait

```
test_from_core_error
  Arrange: CoreError::Store(...)
  Act: ServerError::from(core_error)
  Assert: matches ServerError::Core(_)
```
