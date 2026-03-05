//! Feature-gated async wrappers using `tokio::task::spawn_blocking`.
//!
//! Enabled via the `async` feature flag.

use std::sync::Arc;

use unimatrix_store::{EntryRecord, NewEntry, QueryFilter, Status, TimeRange};
use unimatrix_vector::SearchResult;

use crate::error::CoreError;
use crate::traits::{EmbedService, EntryStore, VectorStore};

/// Async wrapper for any `EntryStore` implementation.
pub struct AsyncEntryStore<T: EntryStore + 'static> {
    inner: Arc<T>,
}

impl<T: EntryStore + 'static> AsyncEntryStore<T> {
    pub fn new(inner: Arc<T>) -> Self {
        AsyncEntryStore { inner }
    }

    pub async fn insert(&self, entry: NewEntry) -> Result<u64, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.insert(entry))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn update(&self, entry: EntryRecord) -> Result<(), CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.update(entry))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn update_status(&self, id: u64, status: Status) -> Result<(), CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.update_status(id, status))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn delete(&self, id: u64) -> Result<(), CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.delete(id))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn get(&self, id: u64) -> Result<EntryRecord, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.get(id))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn exists(&self, id: u64) -> Result<bool, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.exists(id))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn query(&self, filter: QueryFilter) -> Result<Vec<EntryRecord>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.query(filter))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn query_by_topic(&self, topic: &str) -> Result<Vec<EntryRecord>, CoreError> {
        let inner = Arc::clone(&self.inner);
        let topic = topic.to_string();
        tokio::task::spawn_blocking(move || inner.query_by_topic(&topic))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn query_by_category(&self, category: &str) -> Result<Vec<EntryRecord>, CoreError> {
        let inner = Arc::clone(&self.inner);
        let category = category.to_string();
        tokio::task::spawn_blocking(move || inner.query_by_category(&category))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn query_by_tags(&self, tags: &[String]) -> Result<Vec<EntryRecord>, CoreError> {
        let inner = Arc::clone(&self.inner);
        let tags = tags.to_vec();
        tokio::task::spawn_blocking(move || inner.query_by_tags(&tags))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn query_by_time_range(
        &self,
        range: TimeRange,
    ) -> Result<Vec<EntryRecord>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.query_by_time_range(range))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn query_by_status(
        &self,
        status: Status,
    ) -> Result<Vec<EntryRecord>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.query_by_status(status))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn put_vector_mapping(
        &self,
        entry_id: u64,
        hnsw_data_id: u64,
    ) -> Result<(), CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.put_vector_mapping(entry_id, hnsw_data_id))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn get_vector_mapping(&self, entry_id: u64) -> Result<Option<u64>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.get_vector_mapping(entry_id))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn iter_vector_mappings(&self) -> Result<Vec<(u64, u64)>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.iter_vector_mappings())
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn read_counter(&self, name: &str) -> Result<u64, CoreError> {
        let inner = Arc::clone(&self.inner);
        let name = name.to_string();
        tokio::task::spawn_blocking(move || inner.read_counter(&name))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError> {
        let inner = Arc::clone(&self.inner);
        let ids = entry_ids.to_vec();
        tokio::task::spawn_blocking(move || inner.record_access(&ids))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }
}

/// Async wrapper for any `VectorStore` implementation.
pub struct AsyncVectorStore<T: VectorStore + 'static> {
    inner: Arc<T>,
}

impl<T: VectorStore + 'static> AsyncVectorStore<T> {
    pub fn new(inner: Arc<T>) -> Self {
        AsyncVectorStore { inner }
    }

    pub async fn insert(&self, entry_id: u64, embedding: Vec<f32>) -> Result<(), CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.insert(entry_id, &embedding))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn search(
        &self,
        query: Vec<f32>,
        top_k: usize,
        ef_search: usize,
    ) -> Result<Vec<SearchResult>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.search(&query, top_k, ef_search))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn search_filtered(
        &self,
        query: Vec<f32>,
        top_k: usize,
        ef_search: usize,
        allowed_entry_ids: Vec<u64>,
    ) -> Result<Vec<SearchResult>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || {
            inner.search_filtered(&query, top_k, ef_search, &allowed_entry_ids)
        })
        .await
        .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn point_count(&self) -> usize {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.point_count())
            .await
            .unwrap_or(0)
    }

    pub async fn contains(&self, entry_id: u64) -> bool {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.contains(entry_id))
            .await
            .unwrap_or(false)
    }

    pub async fn stale_count(&self) -> usize {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.stale_count())
            .await
            .unwrap_or(0)
    }
}

/// Async wrapper for any `EmbedService` implementation.
pub struct AsyncEmbedService<T: EmbedService + 'static> {
    inner: Arc<T>,
}

impl<T: EmbedService + 'static> AsyncEmbedService<T> {
    pub fn new(inner: Arc<T>) -> Self {
        AsyncEmbedService { inner }
    }

    pub async fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError> {
        let inner = Arc::clone(&self.inner);
        let title = title.to_string();
        let content = content.to_string();
        tokio::task::spawn_blocking(move || inner.embed_entry(&title, &content))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn embed_entries(
        &self,
        entries: Vec<(String, String)>,
    ) -> Result<Vec<Vec<f32>>, CoreError> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.embed_entries(&entries))
            .await
            .map_err(|e| CoreError::JoinError(e.to_string()))?
    }

    pub async fn dimension(&self) -> usize {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.dimension())
            .await
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::StoreAdapter;

    #[tokio::test]
    async fn test_async_insert_and_get() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(unimatrix_store::Store::open(&path).unwrap());
        let adapter = StoreAdapter::new(store);
        let async_store = AsyncEntryStore::new(Arc::new(adapter));

        let entry = NewEntry {
            title: "Async Test".to_string(),
            content: "Content".to_string(),
            topic: "auth".to_string(),
            category: "convention".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: Status::Active,
            created_by: "agent-1".to_string(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };

        let id = async_store.insert(entry).await.unwrap();
        assert!(id >= 1);

        let record = async_store.get(id).await.unwrap();
        assert_eq!(record.title, "Async Test");
        assert_eq!(record.version, 1);
    }

    #[tokio::test]
    async fn test_async_error_propagation() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        let store = Arc::new(unimatrix_store::Store::open(&path).unwrap());
        let adapter = StoreAdapter::new(store);
        let async_store = AsyncEntryStore::new(Arc::new(adapter));

        let result = async_store.get(999).await;
        assert!(matches!(result, Err(CoreError::Store(_))));
    }

    #[test]
    fn test_async_wrappers_are_send() {
        fn _check<T: Send>() {}
        _check::<AsyncEntryStore<StoreAdapter>>();
    }
}
