# Test Plan: core-traits

## Scope

Verify trait definitions, object safety, Send+Sync bounds, and method signatures.

## Compilation Tests (in crates/unimatrix-core/src/traits.rs or tests/)

### test_object_safety_entry_store
- `fn _check(_: &dyn EntryStore) {}` -- must compile.
- Verifies EntryStore is object-safe.

### test_object_safety_vector_store
- `fn _check(_: &dyn VectorStore) {}` -- must compile.

### test_object_safety_embed_service
- `fn _check(_: &dyn EmbedService) {}` -- must compile.

### test_arc_dyn_entry_store
- `fn _check(_: std::sync::Arc<dyn EntryStore>) {}` -- must compile.
- Verifies Arc<dyn EntryStore> is valid (object safety + Sized not required).

### test_arc_dyn_vector_store
- `fn _check(_: std::sync::Arc<dyn VectorStore>) {}` -- must compile.

### test_arc_dyn_embed_service
- `fn _check(_: std::sync::Arc<dyn EmbedService>) {}` -- must compile.

### test_send_sync_entry_store
- `fn _assert_send_sync<T: Send + Sync>() {}; _assert_send_sync::<dyn EntryStore>();` -- must compile.
- Since EntryStore: Send + Sync, dyn EntryStore is Send + Sync.

### test_send_sync_vector_store
- Same pattern for VectorStore.

### test_send_sync_embed_service
- Same pattern for EmbedService.

## Method Count Verification

### test_entry_store_method_count
- Verify EntryStore has exactly 16 methods by implementing a mock (or verifying the trait compiles with a struct implementing all 16).
- Methods: insert, update, update_status, delete, get, exists, query, query_by_topic, query_by_category, query_by_tags, query_by_time_range, query_by_status, put_vector_mapping, get_vector_mapping, iter_vector_mappings, read_counter.

### test_vector_store_method_count
- Verify VectorStore has exactly 6 methods: insert, search, search_filtered, point_count, contains, stale_count.

### test_embed_service_method_count
- Verify EmbedService has exactly 3 methods: embed_entry, embed_entries, dimension.

## Risk Coverage

| Risk | Covered By |
|------|-----------|
| R-05 | test_object_safety_entry_store, test_object_safety_vector_store, test_object_safety_embed_service, test_arc_dyn_* |

## AC Coverage

| AC | Covered By |
|----|-----------|
| AC-01 | test_object_safety_* (traits exist and are usable) |
| AC-02 | test_entry_store_method_count |
| AC-03 | test_vector_store_method_count |
| AC-04 | test_embed_service_method_count |
| AC-20 | test_object_safety_* |
| AC-21 | test_send_sync_* |
