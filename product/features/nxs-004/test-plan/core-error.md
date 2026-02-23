# Test Plan: core-error

## Scope

Verify CoreError enum, From conversions, Display, and std::error::Error implementation.

## Unit Tests (in crates/unimatrix-core/src/error.rs)

### test_from_store_error
- Create a StoreError::EntryNotFound(42).
- Convert: `let core_err: CoreError = store_err.into();`
- Assert: matches `CoreError::Store(_)`.

### test_from_vector_error
- Create a VectorError (e.g., DimensionMismatch).
- Convert to CoreError.
- Assert: matches `CoreError::Vector(_)`.

### test_from_embed_error
- Create an EmbedError variant.
- Convert to CoreError.
- Assert: matches `CoreError::Embed(_)`.

### test_display_store_error
- Create CoreError::Store(StoreError::EntryNotFound(42)).
- Assert: `format!("{err}")` contains "store error" and the inner error message.

### test_display_vector_error
- Create CoreError::Vector(some_vector_error).
- Assert: `format!("{err}")` contains "vector error".

### test_display_embed_error
- Create CoreError::Embed(some_embed_error).
- Assert: `format!("{err}")` contains "embed error".

### test_display_join_error
- Create CoreError::JoinError("task panicked".to_string()).
- Assert: `format!("{err}")` contains "async task error" and "task panicked".

### test_error_source_store
- Create CoreError::Store(store_err).
- Call `std::error::Error::source(&core_err)`.
- Assert: source is Some.
- Assert: source downcast to StoreError succeeds.

### test_error_source_join
- Create CoreError::JoinError("msg".to_string()).
- Assert: `source()` returns None.

### test_core_error_is_send_sync
- Compile-time check: `fn _check<T: Send + Sync>() {}; _check::<CoreError>();`

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-08 | test_from_store_error, test_from_vector_error, test_from_embed_error, test_display_store_error, test_error_source_store |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-01 (partial) | CoreError exists in unimatrix-core |
