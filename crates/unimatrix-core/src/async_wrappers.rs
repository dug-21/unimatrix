//! Feature-gated async wrappers using `tokio::task::spawn_blocking`.
//!
//! Enabled via the `async` feature flag.

use std::sync::Arc;

use unimatrix_vector::SearchResult;

use crate::error::CoreError;
use crate::traits::{EmbedService, VectorStore};

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

    /// Retrieve the stored embedding for an entry (crt-010: supersession injection).
    pub async fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>> {
        let inner = Arc::clone(&self.inner);
        tokio::task::spawn_blocking(move || inner.get_embedding(entry_id))
            .await
            .unwrap_or(None)
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
