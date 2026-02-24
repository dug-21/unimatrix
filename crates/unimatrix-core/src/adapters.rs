use std::sync::Arc;

use unimatrix_store::{EntryRecord, NewEntry, QueryFilter, Status, Store, TimeRange};
use unimatrix_vector::{SearchResult, VectorIndex};

use crate::error::CoreError;
use crate::traits::{EmbedService, EntryStore, VectorStore};

/// Adapter bridging `Store` to the `EntryStore` trait.
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

    fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(self.inner.query(filter)?)
    }

    fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(self.inner.query_by_topic(topic)?)
    }

    fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(self.inner.query_by_category(category)?)
    }

    fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(self.inner.query_by_tags(tags)?)
    }

    fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(self.inner.query_by_time_range(range)?)
    }

    fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(self.inner.query_by_status(status)?)
    }

    fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<(), CoreError> {
        Ok(self.inner.put_vector_mapping(entry_id, hnsw_data_id)?)
    }

    fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>, CoreError> {
        Ok(self.inner.get_vector_mapping(entry_id)?)
    }

    fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>, CoreError> {
        Ok(self.inner.iter_vector_mappings()?)
    }

    fn read_counter(&self, name: &str) -> Result<u64, CoreError> {
        Ok(self.inner.read_counter(name)?)
    }

    fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError> {
        Ok(self
            .inner
            .record_usage(entry_ids, entry_ids, &[], &[], &[], &[])?)
    }
}

/// Adapter bridging `VectorIndex` to the `VectorStore` trait.
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

    fn search(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
    ) -> Result<Vec<SearchResult>, CoreError> {
        Ok(self.inner.search(query, top_k, ef_search)?)
    }

    fn search_filtered(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
        allowed_entry_ids: &[u64],
    ) -> Result<Vec<SearchResult>, CoreError> {
        Ok(self
            .inner
            .search_filtered(query, top_k, ef_search, allowed_entry_ids)?)
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

/// Adapter bridging `EmbeddingProvider` to the `EmbedService` trait.
pub struct EmbedAdapter {
    inner: Arc<dyn unimatrix_embed::EmbeddingProvider>,
    separator: String,
}

impl EmbedAdapter {
    pub fn new(provider: Arc<dyn unimatrix_embed::EmbeddingProvider>) -> Self {
        EmbedAdapter {
            inner: provider,
            separator: ": ".to_string(),
        }
    }
}

impl EmbedService for EmbedAdapter {
    fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError> {
        Ok(unimatrix_embed::embed_entry(
            self.inner.as_ref(),
            title,
            content,
            &self.separator,
        )?)
    }

    fn embed_entries(&self, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>, CoreError> {
        Ok(unimatrix_embed::embed_entries(
            self.inner.as_ref(),
            entries,
            &self.separator,
        )?)
    }

    fn dimension(&self) -> usize {
        self.inner.dimension()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_adapter_implements_entry_store() {
        fn _check(_: &dyn EntryStore) {}
        // Compile-time only: StoreAdapter implements EntryStore
    }

    #[test]
    fn test_vector_adapter_implements_vector_store() {
        fn _check(_: &dyn VectorStore) {}
        // Compile-time only: VectorAdapter implements VectorStore
    }

    #[test]
    fn test_embed_adapter_implements_embed_service() {
        fn _check(_: &dyn EmbedService) {}
        // Compile-time only: EmbedAdapter implements EmbedService
    }

    #[test]
    fn test_store_adapter_insert_and_get() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Arc::new(Store::open(&path).unwrap());
        let adapter = StoreAdapter::new(store);

        let entry = NewEntry {
            title: "Test".to_string(),
            content: "Content".to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "agent-1".to_string(),
            feature_cycle: "nxs-004".to_string(),
            trust_source: "agent".to_string(),
        };

        let id = adapter.insert(entry).unwrap();
        assert!(id >= 1);

        let record = adapter.get(id).unwrap();
        assert_eq!(record.title, "Test");
        assert_eq!(record.content, "Content");
        assert_eq!(record.created_by, "agent-1");
        assert_eq!(record.version, 1);
    }

    #[test]
    fn test_store_adapter_error_propagation() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Arc::new(Store::open(&path).unwrap());
        let adapter = StoreAdapter::new(store);

        let result = adapter.get(999);
        assert!(matches!(result, Err(CoreError::Store(_))));

        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("store error"));
    }

    #[test]
    fn test_dyn_entry_store_invocation() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.redb");
        let store = Arc::new(Store::open(&path).unwrap());
        let adapter = StoreAdapter::new(store);
        let dyn_store: &dyn EntryStore = &adapter;

        let entry = NewEntry {
            title: "Dyn".to_string(),
            content: "Test".to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: String::new(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };

        let id = dyn_store.insert(entry).unwrap();
        let record = dyn_store.get(id).unwrap();
        assert_eq!(record.title, "Dyn");
    }
}
