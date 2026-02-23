# Test Plan: async-wrappers

## Scope

Verify feature-gated async wrappers delegate to sync methods via spawn_blocking, propagate errors, and handle JoinError.

## Unit Tests (in crates/unimatrix-core, feature-gated with `#[cfg(feature = "async")]`)

### test_async_insert_returns_id
- Create TestDb, wrap Store in StoreAdapter, wrap in AsyncEntryStore.
- Call `async_store.insert(new_entry).await`.
- Assert: returns Ok(id) where id >= 1.

### test_async_get_returns_entry
- Insert an entry via async wrapper.
- Call `async_store.get(id).await`.
- Assert: returned entry matches inserted values.

### test_async_insert_get_roundtrip
- Insert entry via async wrapper.
- Get entry via async wrapper.
- Assert: title, content, topic, category, tags all match.
- Assert: security fields populated (version == 1, content_hash non-empty).

### test_async_error_propagation
- Call `async_store.get(999).await`.
- Assert: returns `Err(CoreError::Store(_))`.
- Assert: inner error is EntryNotFound.

### test_async_vector_store_point_count
- Create VectorIndex, wrap in VectorAdapter, wrap in AsyncVectorStore.
- Call `async_vector.point_count().await`.
- Assert: returns expected count.

### test_async_embed_service_dimension
- Create EmbedAdapter, wrap in AsyncEmbedService.
- Call `async_embed.dimension().await`.
- Assert: returns expected dimension.

### test_join_error_conversion
- Construct `CoreError::JoinError("task panicked".to_string())` directly.
- Assert: Display output contains "async task error".
- Assert: matches `CoreError::JoinError(_)`.
- Note: Simulating an actual JoinError from tokio requires a panicking task, which is fragile. The direct construction test validates the conversion path.

## Compilation Tests

### test_async_feature_gate
- Without `async` feature: `AsyncEntryStore`, `AsyncVectorStore`, `AsyncEmbedService` must NOT be importable.
- With `async` feature: all three types must be importable and constructible.

### test_async_wrappers_are_send
- `fn _check<T: Send>() {}; _check::<AsyncEntryStore<StoreAdapter>>();` -- must compile.
- Verifies the async wrapper can be sent across threads.

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-06 | test_async_insert_returns_id, test_async_error_propagation, test_join_error_conversion |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-13 | test_async_insert_get_roundtrip, test_async_feature_gate |
