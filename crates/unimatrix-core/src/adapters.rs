use std::sync::Arc;

use unimatrix_vector::{SearchResult, VectorIndex};

use crate::error::CoreError;
use crate::traits::{EmbedService, VectorStore};

/// Run an async future to completion using the current tokio runtime handle.
///
/// Safe to call from `spawn_blocking` (which runs on a blocking thread with a
/// live `Handle`). Panics if there is no current tokio runtime — callers must
/// ensure they are within a runtime context (e.g., via `spawn_blocking`).
fn block_on_async<F: std::future::Future>(fut: F) -> F::Output {
    tokio::runtime::Handle::current().block_on(fut)
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
}
