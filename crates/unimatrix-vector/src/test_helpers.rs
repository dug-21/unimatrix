//! Reusable test infrastructure for unimatrix-vector and downstream crates.
//!
//! Available within this crate via `#[cfg(test)]` and to downstream crates
//! via the `test-support` feature flag.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use unimatrix_store::Store;

use crate::index::{SearchResult, VectorIndex};
use crate::config::VectorConfig;

/// A test VectorIndex backed by a temporary store.
///
/// Creates a fresh database and index on construction. Automatically
/// cleans up the temporary directory when dropped.
pub struct TestVectorIndex {
    _dir: tempfile::TempDir,
    dir_path: PathBuf,
    store: Arc<Store>,
    index: VectorIndex,
}

impl TestVectorIndex {
    /// Create a new test VectorIndex with default configuration.
    pub fn new() -> Self {
        Self::with_config(VectorConfig::default())
    }

    /// Create a new test VectorIndex with custom configuration.
    pub fn with_config(config: VectorConfig) -> Self {
        let dir = tempfile::TempDir::new().expect("failed to create temp dir");
        let dir_path = dir.path().to_path_buf();
        let db_path = dir.path().join("test.redb");
        let store = Arc::new(Store::open(&db_path).expect("failed to open test store"));
        let index =
            VectorIndex::new(store.clone(), config).expect("failed to create test index");

        TestVectorIndex {
            _dir: dir,
            dir_path,
            store,
            index,
        }
    }

    /// Get a reference to the VectorIndex.
    pub fn vi(&self) -> &VectorIndex {
        &self.index
    }

    /// Get the store (for VECTOR_MAP verification and entry insertion).
    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Get the temp directory path (for persistence tests).
    pub fn dir(&self) -> &Path {
        &self.dir_path
    }
}

/// Generate a random L2-normalized embedding of the given dimension.
///
/// Uses a deterministic-enough random source for test reproducibility.
/// The returned vector has unit length (L2 norm ~= 1.0).
pub fn random_normalized_embedding(dim: usize) -> Vec<f32> {
    use rand::Rng;
    let mut rng = rand::rng();

    // Generate random values in [-1, 1]
    let raw: Vec<f32> = (0..dim).map(|_| rng.random_range(-1.0..1.0)).collect();

    // L2-normalize
    let norm = raw.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm < f32::EPSILON {
        // Avoid zero vector: set first element to 1.0
        let mut v = vec![0.0f32; dim];
        if !v.is_empty() {
            v[0] = 1.0;
        }
        return v;
    }

    raw.iter().map(|x| x / norm).collect()
}

/// Assert that a specific entry_id appears in search results.
pub fn assert_search_contains(results: &[SearchResult], entry_id: u64) {
    assert!(
        results.iter().any(|r| r.entry_id == entry_id),
        "expected entry_id {entry_id} in results: {results:?}"
    );
}

/// Assert that a specific entry_id does NOT appear in search results.
pub fn assert_search_excludes(results: &[SearchResult], entry_id: u64) {
    assert!(
        !results.iter().any(|r| r.entry_id == entry_id),
        "expected entry_id {entry_id} to be ABSENT from results: {results:?}"
    );
}

/// Assert that results are sorted by similarity descending.
pub fn assert_results_sorted(results: &[SearchResult]) {
    for window in results.windows(2) {
        assert!(
            window[0].similarity >= window[1].similarity,
            "results not sorted descending: {} < {}",
            window[0].similarity,
            window[1].similarity
        );
    }
}

/// Insert `count` random normalized vectors into the index, returning entry IDs.
///
/// Creates store entries first (needed for VECTOR_MAP consistency),
/// then inserts random embeddings.
pub fn seed_vectors(vi: &VectorIndex, store: &Store, count: usize) -> Vec<u64> {
    let dim = vi.config().dimension;
    let mut ids = Vec::with_capacity(count);

    for i in 0..count {
        let entry = unimatrix_store::NewEntry {
            title: format!("Vector entry {i}"),
            content: format!("Content for vector entry {i}"),
            topic: "test".to_string(),
            category: "vector".to_string(),
            tags: vec![],
            source: "test".to_string(),
            status: unimatrix_store::Status::Active,
            created_by: String::new(),
            feature_cycle: String::new(),
            trust_source: String::new(),
        };
        let entry_id = store.insert(entry).expect("failed to insert store entry");

        let embedding = random_normalized_embedding(dim);
        vi.insert(entry_id, &embedding)
            .expect("failed to insert vector");

        ids.push(entry_id);
    }

    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_embedding_dimension() {
        for dim in [128, 384, 768] {
            let emb = random_normalized_embedding(dim);
            assert_eq!(emb.len(), dim);
        }
    }

    #[test]
    fn test_random_embedding_l2_normalized() {
        let emb = random_normalized_embedding(384);
        let norm_sq: f32 = emb.iter().map(|x| x * x).sum();
        assert!(
            (norm_sq - 1.0).abs() < 0.001,
            "expected L2 norm ~1.0, got sqrt({norm_sq})"
        );
    }

    #[test]
    fn test_random_embedding_not_all_zeros() {
        let emb = random_normalized_embedding(384);
        assert!(emb.iter().any(|&x| x != 0.0));
    }

    #[test]
    fn test_random_embeddings_different() {
        let emb1 = random_normalized_embedding(384);
        let emb2 = random_normalized_embedding(384);
        assert_ne!(emb1, emb2);
    }
}
