use crate::error::CoreError;
use unimatrix_vector::SearchResult;

/// Trait abstraction over vector similarity search (unimatrix-vector).
///
/// Object-safe, `Send + Sync`.
pub trait VectorStore: Send + Sync {
    fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<(), CoreError>;
    fn search(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
    ) -> Result<Vec<SearchResult>, CoreError>;
    fn search_filtered(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
        allowed_entry_ids: &[u64],
    ) -> Result<Vec<SearchResult>, CoreError>;
    fn point_count(&self) -> usize;
    fn contains(&self, entry_id: u64) -> bool;
    fn stale_count(&self) -> usize;
    /// Retrieve the stored embedding for an entry. Returns None if no mapping exists (crt-010).
    fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>>;
    /// Rebuild the HNSW graph from active entry embeddings, eliminating stale nodes.
    /// Object-safe: `&self`, concrete types, no generics.
    fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<(), CoreError>;
}

/// Trait abstraction over embedding generation (unimatrix-embed).
///
/// Object-safe, `Send + Sync`.
pub trait EmbedService: Send + Sync {
    fn embed_entry(&self, title: &str, content: &str) -> Result<Vec<f32>, CoreError>;
    fn embed_entries(&self, entries: &[(String, String)]) -> Result<Vec<Vec<f32>>, CoreError>;
    fn dimension(&self) -> usize;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    // Object safety compile-time checks

    #[test]
    fn test_object_safety_vector_store() {
        fn _check(_: &dyn VectorStore) {}
    }

    #[test]
    fn test_object_safety_embed_service() {
        fn _check(_: &dyn EmbedService) {}
    }

    #[test]
    fn test_arc_dyn_vector_store() {
        fn _check(_: Arc<dyn VectorStore>) {}
    }

    #[test]
    fn test_arc_dyn_embed_service() {
        fn _check(_: Arc<dyn EmbedService>) {}
    }
}
