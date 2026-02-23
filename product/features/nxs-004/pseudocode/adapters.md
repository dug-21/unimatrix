# Pseudocode: adapters

## Purpose
Implement domain adapters that bridge concrete types to core traits.

## New File: crates/unimatrix-core/src/adapters.rs

### StoreAdapter

```
use std::sync::Arc;
use unimatrix_store::Store;
use crate::error::CoreError;
use crate::traits::EntryStore;

pub struct StoreAdapter {
    inner: Arc<Store>,
}

impl StoreAdapter {
    pub fn new(store: Arc<Store>) -> Self {
        StoreAdapter { inner: store }
    }
}

impl EntryStore for StoreAdapter {
    fn insert(&self, entry: NewEntry) -> Result<u64, CoreError> {
        Ok(self.inner.insert(entry)?)
    }

    fn update(&self, entry: EntryRecord) -> Result<(), CoreError> {
        Ok(self.inner.update(entry)?)
    }

    fn update_status(&self, id: u64, status: Status) -> Result<(), CoreError> {
        Ok(self.inner.update_status(id, status)?)
    }

    fn delete(&self, id: u64) -> Result<(), CoreError> {
        Ok(self.inner.delete(id)?)
    }

    fn get(&self, id: u64) -> Result<EntryRecord, CoreError> {
        Ok(self.inner.get(id)?)
    }

    fn exists(&self, id: u64) -> Result<bool, CoreError> {
        Ok(self.inner.exists(id)?)
    }

    // ... all remaining methods follow the same pattern:
    // call self.inner.method() and convert error via ?
}
```

### VectorAdapter

```
pub struct VectorAdapter {
    inner: Arc<VectorIndex>,
}

impl VectorAdapter {
    pub fn new(index: Arc<VectorIndex>) -> Self {
        VectorAdapter { inner: index }
    }
}

impl VectorStore for VectorAdapter {
    fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<(), CoreError> {
        Ok(self.inner.insert(entry_id, embedding)?)
    }

    fn search(&self, query: &[f32], top_k: usize, ef_search: usize)
        -> Result<Vec<SearchResult>, CoreError> {
        Ok(self.inner.search(query, top_k, ef_search)?)
    }

    fn search_filtered(&self, query: &[f32], top_k: usize, ef_search: usize,
        allowed: &[u64]) -> Result<Vec<SearchResult>, CoreError> {
        Ok(self.inner.search_filtered(query, top_k, ef_search, allowed)?)
    }

    fn point_count(&self) -> usize {
        self.inner.point_count()
    }

    fn contains(&self, entry_id: u64) -> bool {
        self.inner.contains(entry_id)
    }

    fn stale_count(&self) -> usize {
        self.inner.stale_count()
    }
}
```

### EmbedAdapter

```
pub struct EmbedAdapter {
    inner: Arc<dyn EmbeddingProvider>,
    separator: String,
}

impl EmbedAdapter {
    pub fn new(provider: Arc<dyn EmbeddingProvider>) -> Self {
        EmbedAdapter {
            inner: provider,
            separator: ": ".to_string(),
        }
    }
}

impl EmbedService for EmbedAdapter {
    fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError> {
        Ok(unimatrix_embed::embed_entry(
            self.inner.as_ref(), title, content, &self.separator
        )?)
    }

    fn embed_entries(&self, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>, CoreError> {
        Ok(unimatrix_embed::embed_entries(
            self.inner.as_ref(), entries, &self.separator
        )?)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
}
```

## Error Handling
All adapter methods use `?` to convert crate-specific errors to CoreError via From impls.

## Key Test Scenarios
- StoreAdapter implements EntryStore (compile check via dyn usage)
- VectorAdapter implements VectorStore (compile check)
- EmbedAdapter implements EmbedService (compile check)
- StoreAdapter::insert through trait returns correct ID
- StoreAdapter::get through trait returns correct entry
- Error propagation: StoreAdapter get(999) returns CoreError::Store(EntryNotFound)
