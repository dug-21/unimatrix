# Test Plan: adapters

## Scope

Verify StoreAdapter, VectorAdapter, and EmbedAdapter implement their respective traits, delegate correctly, and convert errors.

## Unit Tests (in crates/unimatrix-core/src/adapters.rs or tests/)

### test_store_adapter_implements_entry_store
- Construct `StoreAdapter::new(Arc::new(store))`.
- Cast to `&dyn EntryStore` -- must compile.
- This is a compile-time trait implementation check.

### test_store_adapter_insert_through_trait
- Create a TestDb, wrap in `StoreAdapter`.
- Call `EntryStore::insert(adapter, new_entry)`.
- Assert: returns Ok(id) where id >= 1.
- Call `EntryStore::get(adapter, id)`.
- Assert: returned entry matches inserted values.

### test_store_adapter_get_through_trait
- Insert an entry directly via Store.
- Call `EntryStore::get(&adapter, id)`.
- Assert: title, content, topic, category match.

### test_store_adapter_error_propagation
- Call `EntryStore::get(&adapter, 999)` (nonexistent ID).
- Assert: returns `Err(CoreError::Store(_))`.
- Assert: Display output contains "store error".

### test_store_adapter_error_preserves_context
- Call `EntryStore::get(&adapter, 999)`.
- Match on `CoreError::Store(ref e)`.
- Assert: `e` is `StoreError::EntryNotFound(999)`.
- Call `std::error::Error::source(&core_err)` -- assert it returns Some.

### test_vector_adapter_implements_vector_store
- Construct `VectorAdapter::new(Arc::new(vector_index))`.
- Cast to `&dyn VectorStore` -- must compile.

### test_vector_adapter_point_count
- Create a VectorIndex, insert some vectors.
- Wrap in VectorAdapter.
- Call `VectorStore::point_count(&adapter)`.
- Assert: returns expected count.

### test_embed_adapter_implements_embed_service
- Construct `EmbedAdapter::new(Arc::new(provider))`.
- Cast to `&dyn EmbedService` -- must compile.

### test_embed_adapter_dimension
- Create an EmbedAdapter wrapping a provider with known dimension (384).
- Call `EmbedService::dimension(&adapter)`.
- Assert: returns 384.

## Integration Tests

### test_dyn_entry_store_invocation
- Construct StoreAdapter, box as `Box<dyn EntryStore>`.
- Call insert() and get() through the trait object.
- Assert: round-trip succeeds.
- This validates R-05 (trait object usability with concrete adapter).

### test_adapter_all_entry_store_methods
- For each of the 16 EntryStore methods:
  - Call through the adapter with valid input.
  - Assert: no panic, returns Ok or expected Err.
- This ensures the adapter delegates all methods, not just a subset.

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-05 | test_store_adapter_implements_entry_store, test_vector_adapter_implements_vector_store, test_embed_adapter_implements_embed_service, test_dyn_entry_store_invocation |
| R-08 | test_store_adapter_error_propagation, test_store_adapter_error_preserves_context |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-12 | test_store_adapter_implements_entry_store, test_vector_adapter_implements_vector_store, test_embed_adapter_implements_embed_service |
| AC-02 | test_adapter_all_entry_store_methods |
