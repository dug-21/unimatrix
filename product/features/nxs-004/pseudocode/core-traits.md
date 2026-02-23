# Pseudocode: core-traits

## Purpose
Define the three core traits: EntryStore, VectorStore, EmbedService.

## New File: crates/unimatrix-core/src/traits.rs

### EntryStore

```
pub trait EntryStore: Send + Sync {
    fn insert(&self, entry: NewEntry) -> Result<u64, CoreError>;
    fn update(&self, entry: EntryRecord) -> Result<(), CoreError>;
    fn update_status(&self, id: u64, status: Status) -> Result<(), CoreError>;
    fn delete(&self, id: u64) -> Result<(), CoreError>;
    fn get(&self, id: u64) -> Result<EntryRecord, CoreError>;
    fn exists(&self, id: u64) -> Result<bool, CoreError>;
    fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>, CoreError>;
    fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>, CoreError>;
    fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<(), CoreError>;
    fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>, CoreError>;
    fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>, CoreError>;
    fn read_counter(&self, name: &str) -> Result<u64, CoreError>;
}
```

Note: compact() is excluded (requires &mut self, breaks object safety). See ADR-006.

### VectorStore

```
pub trait VectorStore: Send + Sync {
    fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<(), CoreError>;
    fn search(&self, query: &[f32], top_k: usize, ef_search: usize)
        -> Result<Vec<SearchResult>, CoreError>;
    fn search_filtered(&self, query: &[f32], top_k: usize, ef_search: usize,
        allowed_entry_ids: &[u64]) -> Result<Vec<SearchResult>, CoreError>;
    fn point_count(&self) -> usize;
    fn contains(&self, entry_id: u64) -> bool;
    fn stale_count(&self) -> usize;
}
```

### EmbedService

```
pub trait EmbedService: Send + Sync {
    fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError>;
    fn embed_entries(&self, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>, CoreError>;
    fn dimension(&self) -> usize;
}
```

## Object Safety Verification

All methods:
- Take `&self` (not `self` or generic)
- Return concrete types (no `impl Trait`)
- No generic type parameters on methods

This ensures `dyn EntryStore`, `dyn VectorStore`, `dyn EmbedService` are valid.

## Key Test Scenarios
- Compile-time: `fn _check(_: &dyn EntryStore) {}` compiles
- Compile-time: `fn _check(_: &dyn VectorStore) {}` compiles
- Compile-time: `fn _check(_: &dyn EmbedService) {}` compiles
- Compile-time: `Arc<dyn EntryStore>` is valid
- Traits are Send + Sync (compile check)
