use crate::error::CoreError;
use unimatrix_store::{EntryRecord, NewEntry, QueryFilter, Status, TimeRange};
use unimatrix_vector::SearchResult;

/// Trait abstraction over entry storage (unimatrix-store).
///
/// All methods return `Result<T, CoreError>`. Object-safe, `Send + Sync`.
/// `compact()` is excluded because it requires `&mut self`, breaking object safety.
/// See ADR-006.
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
    fn test_object_safety_entry_store() {
        fn _check(_: &dyn EntryStore) {}
    }

    #[test]
    fn test_object_safety_vector_store() {
        fn _check(_: &dyn VectorStore) {}
    }

    #[test]
    fn test_object_safety_embed_service() {
        fn _check(_: &dyn EmbedService) {}
    }

    #[test]
    fn test_arc_dyn_entry_store() {
        fn _check(_: Arc<dyn EntryStore>) {}
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
