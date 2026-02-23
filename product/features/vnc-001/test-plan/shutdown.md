# Test Plan: shutdown.rs

## Risks Covered
- R-08: Graceful shutdown fails to call compact() (High)
- R-09: VectorIndex::dump() failure during shutdown (High)

## Unit Tests

### LifecycleHandles and Arc management

```
test_try_unwrap_succeeds_when_sole_owner
  Arrange: create Arc<Store>, clone it into registry/audit/vector, then drop all clones
  Act: Arc::try_unwrap(store)
  Assert: Ok (sole owner)

test_try_unwrap_fails_with_outstanding_refs
  Arrange: create Arc<Store>, hold an extra clone
  Act: Arc::try_unwrap(store)
  Assert: Err (still references outstanding)

test_compact_called_on_successful_unwrap
  Arrange: create Store, insert data, wrap in Arc
  Act: unwrap, call compact()
  Assert: Ok (compact succeeds)
```

### Vector dump

```
test_vector_dump_creates_files
  Arrange: create Store + VectorIndex, insert some data
  Act: vector_index.dump(&vector_dir)
  Assert: dump files exist in vector_dir

test_vector_dump_failure_non_fatal
  Arrange: create VectorIndex, make vector_dir read-only
  Act: attempt dump
  Assert: returns Err but program can continue (not panic)
```

### Shutdown sequence ordering

```
test_shutdown_order_dump_before_drop
  This is a design-level test verified by code review:
  - dump() called while vector_index Arc is still alive
  - vector_index dropped AFTER dump completes
  - store try_unwrap attempted AFTER all other drops
```

## Integration Notes

Full shutdown tests (signal handling, MCP session close) require process-level testing and are in integration tests. Unit tests here focus on the Arc lifecycle management and vector dump behavior.
