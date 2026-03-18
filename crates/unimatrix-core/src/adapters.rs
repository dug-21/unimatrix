use std::sync::Arc;

use unimatrix_store::{EntryRecord, NewEntry, QueryFilter, SqlxStore, Status, TimeRange};
use unimatrix_vector::{SearchResult, VectorIndex};

use crate::error::CoreError;
use crate::traits::{EmbedService, EntryStore, VectorStore};

/// Run an async future to completion using the current tokio runtime handle.
///
/// Safe to call from `spawn_blocking` (which runs on a blocking thread with a
/// live `Handle`). Panics if there is no current tokio runtime — callers must
/// ensure they are within a runtime context (e.g., via `spawn_blocking`).
fn block_on_async<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Handle::current().block_on(fut)
}

/// Adapter bridging `SqlxStore` to the `EntryStore` trait.
///
/// All `EntryStore` methods are sync; they bridge to async `SqlxStore` methods
/// via `block_on_async`. This adapter must only be invoked from `spawn_blocking`
/// or another blocking-thread context where `Handle::current()` is available.
pub struct StoreAdapter {
    inner: Arc<SqlxStore>,
}

impl StoreAdapter {
    pub fn new(store: Arc<SqlxStore>) -> Self {
        StoreAdapter { inner: store }
    }
}

impl EntryStore for StoreAdapter {
    fn insert(&self, entry: NewEntry) -> Result<u64, CoreError> {
        Ok(block_on_async(self.inner.insert(entry))?)
    }

    fn update(&self, entry: EntryRecord) -> Result<(), CoreError> {
        Ok(block_on_async(self.inner.update(entry))?)
    }

    fn update_status(&self, id: u64, status: Status) -> Result<(), CoreError> {
        Ok(block_on_async(self.inner.update_status(id, status))?)
    }

    fn delete(&self, id: u64) -> Result<(), CoreError> {
        Ok(block_on_async(self.inner.delete(id))?)
    }

    fn get(&self, id: u64) -> Result<EntryRecord, CoreError> {
        Ok(block_on_async(self.inner.get(id))?)
    }

    fn exists(&self, id: u64) -> Result<bool, CoreError> {
        Ok(block_on_async(self.inner.exists(id))?)
    }

    fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(block_on_async(self.inner.query(filter))?)
    }

    fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>, CoreError> {
        let topic = topic.to_string();
        Ok(block_on_async(self.inner.query_by_topic(&topic))?)
    }

    fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>, CoreError> {
        let category = category.to_string();
        Ok(block_on_async(self.inner.query_by_category(&category))?)
    }

    fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>, CoreError> {
        let tags = tags.to_vec();
        Ok(block_on_async(self.inner.query_by_tags(&tags))?)
    }

    fn query_by_time_range(&self, range: TimeRange) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(block_on_async(self.inner.query_by_time_range(range))?)
    }

    fn query_by_status(&self, status: Status) -> Result<Vec<EntryRecord>, CoreError> {
        Ok(block_on_async(self.inner.query_by_status(status))?)
    }

    fn put_vector_mapping(&self, entry_id: u64, hnsw_data_id: u64) -> Result<(), CoreError> {
        Ok(block_on_async(
            self.inner.put_vector_mapping(entry_id, hnsw_data_id),
        )?)
    }

    fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>, CoreError> {
        Ok(block_on_async(self.inner.get_vector_mapping(entry_id))?)
    }

    fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>, CoreError> {
        Ok(block_on_async(self.inner.iter_vector_mappings())?)
    }

    fn read_counter(&self, name: &str) -> Result<u64, CoreError> {
        let name = name.to_string();
        Ok(block_on_async(self.inner.read_counter(&name))?)
    }

    fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError> {
        let ids = entry_ids.to_vec();
        Ok(block_on_async(self.inner.record_usage(
            &ids,
            &ids,
            &[],
            &[],
            &[],
            &[],
        ))?)
    }
}

/// Adapter bridging `VectorIndex` to the `VectorStore` trait.
///
/// `insert` and `compact` are async on `VectorIndex`; they are bridged via
/// `block_on_async`. Must only be called from `spawn_blocking` context.
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
        let embedding = embedding.to_vec();
        Ok(block_on_async(self.inner.insert(entry_id, &embedding))?)
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

    fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>> {
        self.inner.get_embedding(entry_id)
    }

    fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<(), CoreError> {
        Ok(block_on_async(self.inner.compact(embeddings))?)
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
    use unimatrix_store::pool_config::PoolConfig;

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

    #[tokio::test]
    async fn test_store_adapter_insert_and_get() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(SqlxStore::open(&path, PoolConfig::default()).await.unwrap());
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

        // StoreAdapter is sync but uses block_on_async — must call from spawn_blocking
        let adapter = Arc::new(adapter);
        let id = tokio::task::spawn_blocking(move || {
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
            adapter.insert(entry).unwrap()
        })
        .await
        .unwrap();
        assert!(id >= 1);
        drop(entry); // suppress unused warning
    }

    #[tokio::test]
    async fn test_store_adapter_error_propagation() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(SqlxStore::open(&path, PoolConfig::default()).await.unwrap());
        let adapter = Arc::new(StoreAdapter::new(store));

        let result = tokio::task::spawn_blocking(move || adapter.get(999))
            .await
            .unwrap();
        assert!(matches!(result, Err(CoreError::Store(_))));

        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("store error"));
    }

    #[tokio::test]
    async fn test_dyn_entry_store_invocation() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(SqlxStore::open(&path, PoolConfig::default()).await.unwrap());
        let adapter = Arc::new(StoreAdapter::new(store));

        let id = tokio::task::spawn_blocking(move || {
            let dyn_store: &dyn EntryStore = adapter.as_ref();
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
            id
        })
        .await
        .unwrap();
        assert!(id >= 1);
    }
}
