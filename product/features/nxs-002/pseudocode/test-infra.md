# C9: Test Infrastructure -- Pseudocode

## Purpose

Reusable test helpers for this crate and downstream crates. Follows nxs-001's test infrastructure patterns (TestDb, seed functions, assertion helpers).

## File: `crates/unimatrix-vector/src/test_helpers.rs`

```
//! Reusable test infrastructure for unimatrix-vector and downstream crates.
//!
//! Available within this crate via #[cfg(test)] and to downstream crates
//! via the `test-support` feature flag.

use std::sync::Arc;
use crate::{VectorIndex, VectorConfig, SearchResult};
use unimatrix_store::Store;
use unimatrix_store::test_helpers::TestDb;

/// A test VectorIndex backed by a temporary store.
/// Provides convenience methods for testing.
STRUCT TestVectorIndex:
    _db: TestDb               // keeps temp dir alive
    index: VectorIndex

IMPL TestVectorIndex:
    /// Create a new test VectorIndex with default config.
    pub fn new() -> Self:
        db = TestDb::new()
        store = Arc::new(... )  // NOTE: need to handle ownership
        // TestDb owns the Store, but VectorIndex needs Arc<Store>
        // Solution: TestVectorIndex creates its own Store from a temp path

        // Actually, need to create Store ourselves for Arc wrapping:
        dir = tempfile::TempDir::new().unwrap()
        path = dir.path().join("test.redb")
        store = Arc::new(Store::open(&path).unwrap())
        index = VectorIndex::new(store.clone(), VectorConfig::default()).unwrap()

        TestVectorIndex { _dir: dir, store, index }

    /// Create with custom config.
    pub fn with_config(config: VectorConfig) -> Self:
        dir = tempfile::TempDir::new().unwrap()
        path = dir.path().join("test.redb")
        store = Arc::new(Store::open(&path).unwrap())
        index = VectorIndex::new(store.clone(), config).unwrap()
        TestVectorIndex { _dir: dir, store, index }

    /// Get a reference to the VectorIndex.
    pub fn vi(&self) -> &VectorIndex:
        &self.index

    /// Get the store (for VECTOR_MAP verification).
    pub fn store(&self) -> &Arc<Store>:
        &self.store

    /// Get the temp directory path (for persistence tests).
    pub fn dir(&self) -> &Path:
        self._dir.path()

/// Generate a random L2-normalized embedding of the given dimension.
pub fn random_normalized_embedding(dim: usize) -> Vec<f32>:
    use rand::Rng;
    let mut rng = rand::rng();

    // Generate random values
    raw: Vec<f32> = (0..dim).map(|_| rng.random_range(-1.0..1.0)).collect()

    // L2-normalize
    norm = raw.iter().map(|x| x * x).sum::<f32>().sqrt()
    if norm == 0.0:
        // Avoid zero vector -- set first element to 1.0
        raw[0] = 1.0
        return raw

    raw.iter().map(|x| x / norm).collect()

/// Assert that a specific entry_id appears in search results.
pub fn assert_search_contains(results: &[SearchResult], entry_id: u64):
    assert!(
        results.iter().any(|r| r.entry_id == entry_id),
        "expected entry_id {} in results: {:?}",
        entry_id, results
    )

/// Assert that a specific entry_id does NOT appear in search results.
pub fn assert_search_excludes(results: &[SearchResult], entry_id: u64):
    assert!(
        !results.iter().any(|r| r.entry_id == entry_id),
        "expected entry_id {} to be ABSENT from results: {:?}",
        entry_id, results
    )

/// Assert that results are sorted by similarity descending.
pub fn assert_results_sorted(results: &[SearchResult]):
    for window in results.windows(2):
        assert!(
            window[0].similarity >= window[1].similarity,
            "results not sorted descending: {} < {}",
            window[0].similarity, window[1].similarity
        )

/// Insert count random normalized vectors into the index, returning entry_ids.
/// Entry IDs start at 1 and increment.
pub fn seed_vectors(vi: &VectorIndex, store: &Store, count: usize) -> Vec<u64>:
    use unimatrix_store::test_helpers::TestEntry;

    let mut ids = Vec::new()
    for i in 0..count:
        // Insert a store entry first to get an entry_id
        entry = TestEntry::new("test", "vector")
            .with_title(&format!("Vector entry {i}"))
            .build()
        entry_id = store.insert(entry).unwrap()

        // Generate and insert random embedding
        embedding = random_normalized_embedding(vi.config().dimension)
        vi.insert(entry_id, &embedding).unwrap()

        ids.push(entry_id)
    ids
```

## Design Notes

- **TestVectorIndex owns its temp dir**: The temp directory is kept alive by the struct. When dropped, cleanup happens automatically.
- **Store ownership**: `TestVectorIndex` creates its own `Arc<Store>` from a temp path, not reusing `TestDb` (which owns the Store without Arc). This avoids lifetime issues.
- **seed_vectors creates store entries first**: Unlike nxs-001's `seed_entries`, we also need real store entries for VECTOR_MAP consistency. Each seeded vector has a corresponding entry in the ENTRIES table.
- **random_normalized_embedding**: Uses `rand::rng()` (rand 0.9 API) to generate random vectors, then L2-normalizes. Handles the edge case of an all-zero random vector.
- **assert_search_excludes**: Added beyond the IMPLEMENTATION-BRIEF spec because filtered search tests need to verify exclusion.
- **assert_results_sorted**: Verifies the descending similarity invariant that all search methods must maintain.
