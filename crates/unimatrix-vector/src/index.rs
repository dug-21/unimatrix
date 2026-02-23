use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use anndists::dist::DistDot;
use hnsw_rs::prelude::*;
use unimatrix_store::Store;

use crate::config::VectorConfig;
use crate::error::{Result, VectorError};
use crate::filter::EntryIdFilter;

/// A search result containing an entry ID and its similarity score.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchResult {
    /// The entry ID from unimatrix-store.
    pub entry_id: u64,
    /// Similarity score: `1.0 - distance`. Higher means more similar.
    /// Range is typically [0.0, 1.0] for L2-normalized vectors.
    pub similarity: f32,
}

/// Internal bidirectional map between entry IDs and hnsw data IDs.
struct IdMap {
    data_to_entry: HashMap<u64, u64>,
    entry_to_data: HashMap<u64, u64>,
}

impl IdMap {
    fn new() -> Self {
        IdMap {
            data_to_entry: HashMap::new(),
            entry_to_data: HashMap::new(),
        }
    }

    fn active_count(&self) -> usize {
        self.entry_to_data.len()
    }
}

/// The vector similarity search index.
///
/// Wraps hnsw_rs with thread-safe access, bidirectional ID mapping,
/// and coordination with the unimatrix-store VECTOR_MAP table.
///
/// `VectorIndex` is `Send + Sync` and shareable via `Arc<VectorIndex>`.
pub struct VectorIndex {
    hnsw: RwLock<Hnsw<'static, f32, DistDot>>,
    store: Arc<Store>,
    config: VectorConfig,
    next_data_id: AtomicU64,
    id_map: RwLock<IdMap>,
}

impl VectorIndex {
    /// Create a new empty vector index.
    ///
    /// The index starts in insert mode with zero vectors.
    pub fn new(store: Arc<Store>, config: VectorConfig) -> Result<Self> {
        let hnsw = Hnsw::<f32, DistDot>::new(
            config.max_nb_connection,
            config.max_elements,
            config.max_layer,
            config.ef_construction,
            DistDot,
        );

        Ok(VectorIndex {
            hnsw: RwLock::new(hnsw),
            store,
            config,
            next_data_id: AtomicU64::new(0),
            id_map: RwLock::new(IdMap::new()),
        })
    }

    /// Create a VectorIndex from pre-loaded components.
    /// Used by the persistence module during load.
    pub(crate) fn from_parts(
        hnsw: Hnsw<'static, f32, DistDot>,
        store: Arc<Store>,
        config: VectorConfig,
        next_data_id: u64,
        id_map_data: Vec<(u64, u64)>,
    ) -> Self {
        let mut data_to_entry = HashMap::with_capacity(id_map_data.len());
        let mut entry_to_data = HashMap::with_capacity(id_map_data.len());
        for (entry_id, data_id) in id_map_data {
            entry_to_data.insert(entry_id, data_id);
            data_to_entry.insert(data_id, entry_id);
        }

        VectorIndex {
            hnsw: RwLock::new(hnsw),
            store,
            config,
            next_data_id: AtomicU64::new(next_data_id),
            id_map: RwLock::new(IdMap {
                data_to_entry,
                entry_to_data,
            }),
        }
    }

    /// Validate that the embedding dimension matches the configured dimension.
    fn validate_dimension(&self, embedding: &[f32]) -> Result<()> {
        if embedding.len() != self.config.dimension {
            return Err(VectorError::DimensionMismatch {
                expected: self.config.dimension,
                got: embedding.len(),
            });
        }
        Ok(())
    }

    /// Validate that the embedding contains no NaN or infinity values.
    fn validate_embedding(&self, embedding: &[f32]) -> Result<()> {
        for (i, &val) in embedding.iter().enumerate() {
            if val.is_nan() {
                return Err(VectorError::InvalidEmbedding(format!(
                    "NaN at index {i}"
                )));
            }
            if val.is_infinite() {
                return Err(VectorError::InvalidEmbedding(format!(
                    "infinity at index {i}"
                )));
            }
        }
        Ok(())
    }

    /// Insert a vector for the given entry ID.
    ///
    /// Validates embedding dimension and float values. Inserts into hnsw_rs,
    /// writes to VECTOR_MAP, and updates the bidirectional IdMap.
    ///
    /// If the entry already has a vector (re-embedding), the old point remains
    /// in hnsw_rs but is tracked as stale. VECTOR_MAP is overwritten with the
    /// new data ID.
    pub fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<()> {
        self.validate_dimension(embedding)?;
        self.validate_embedding(embedding)?;

        // Generate monotonic data ID
        let data_id = self.next_data_id.fetch_add(1, Ordering::Relaxed);

        // Insert into hnsw_rs (needs write lock only for insert_slice in practice,
        // but hnsw_rs insert_slice takes &self so read lock would suffice for the
        // insert call itself. However, we need consistent mode management.)
        {
            let hnsw = self.hnsw.write().unwrap_or_else(|e| e.into_inner());
            let data_vec = embedding.to_vec();
            hnsw.insert_slice((&data_vec, data_id as usize));
        }

        // Write to VECTOR_MAP (crash-safe)
        self.store.put_vector_mapping(entry_id, data_id)?;

        // Update IdMap
        {
            let mut id_map = self.id_map.write().unwrap_or_else(|e| e.into_inner());
            if let Some(old_data_id) = id_map.entry_to_data.insert(entry_id, data_id) {
                // Re-embedding: remove old reverse mapping so stale point is invisible
                id_map.data_to_entry.remove(&old_data_id);
            }
            id_map.data_to_entry.insert(data_id, entry_id);
        }

        Ok(())
    }

    /// Search for the most similar vectors to the query embedding.
    ///
    /// Returns up to `top_k` results sorted by similarity descending.
    /// Returns an empty vec if the index is empty or `top_k` is 0.
    pub fn search(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
    ) -> Result<Vec<SearchResult>> {
        self.validate_dimension(query)?;
        self.validate_embedding(query)?;

        if top_k == 0 {
            return Ok(vec![]);
        }

        let effective_ef = ef_search.max(top_k);

        // Search hnsw_rs
        let neighbours;
        {
            let hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner());
            if hnsw.get_nb_point() == 0 {
                return Ok(vec![]);
            }
            neighbours = hnsw.search(query, top_k, effective_ef);
        }

        self.map_neighbours_to_results(&neighbours)
    }

    /// Search with a pre-computed allow-list of entry IDs.
    ///
    /// Only entries in `allowed_entry_ids` can appear in results.
    /// Entry IDs without vector mappings are silently skipped.
    /// Returns an empty vec if no valid mappings exist.
    pub fn search_filtered(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
        allowed_entry_ids: &[u64],
    ) -> Result<Vec<SearchResult>> {
        self.validate_dimension(query)?;
        self.validate_embedding(query)?;

        if top_k == 0 || allowed_entry_ids.is_empty() {
            return Ok(vec![]);
        }

        // Translate entry IDs to data IDs
        let allowed_data_ids;
        {
            let id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner());
            allowed_data_ids = allowed_entry_ids
                .iter()
                .filter_map(|&eid| {
                    id_map.entry_to_data.get(&eid).map(|&did| did as usize)
                })
                .collect::<Vec<usize>>();
        }

        if allowed_data_ids.is_empty() {
            return Ok(vec![]);
        }

        let filter = EntryIdFilter::new(allowed_data_ids);
        let effective_ef = ef_search.max(top_k);

        let neighbours;
        {
            let hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner());
            if hnsw.get_nb_point() == 0 {
                return Ok(vec![]);
            }
            neighbours = hnsw.search_filter(query, top_k, effective_ef, Some(&filter));
        }

        self.map_neighbours_to_results(&neighbours)
    }

    /// Map hnsw_rs Neighbour results to SearchResult via IdMap.
    fn map_neighbours_to_results(
        &self,
        neighbours: &[Neighbour],
    ) -> Result<Vec<SearchResult>> {
        let id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner());

        let mut results = Vec::with_capacity(neighbours.len());
        let mut seen = HashSet::new();

        for n in neighbours {
            let data_id = n.d_id as u64;
            if let Some(&entry_id) = id_map.data_to_entry.get(&data_id) {
                // Deduplicate by entry_id (re-embedded entries may appear twice)
                if seen.insert(entry_id) {
                    results.push(SearchResult {
                        entry_id,
                        similarity: 1.0 - n.distance,
                    });
                }
            }
            // Stale points (data_id not in map) are silently skipped
        }

        // Sort by similarity descending
        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    /// Number of vectors in the hnsw_rs index (includes stale re-embedded points).
    pub fn point_count(&self) -> usize {
        let hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner());
        hnsw.get_nb_point()
    }

    /// Check if an entry has a current vector mapping.
    pub fn contains(&self, entry_id: u64) -> bool {
        let id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner());
        id_map.entry_to_data.contains_key(&entry_id)
    }

    /// Number of stale points (from re-embedding) in the hnsw_rs index.
    pub fn stale_count(&self) -> usize {
        let point_count = self.point_count();
        let id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner());
        point_count.saturating_sub(id_map.active_count())
    }

    /// Get a reference to the index configuration.
    pub fn config(&self) -> &VectorConfig {
        &self.config
    }

    /// Get the hnsw_rs read lock (for persistence dump).
    pub(crate) fn hnsw_read(
        &self,
    ) -> std::sync::RwLockReadGuard<'_, Hnsw<'static, f32, DistDot>> {
        self.hnsw.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Get the current next_data_id value (for persistence dump).
    pub(crate) fn next_data_id_value(&self) -> u64 {
        self.next_data_id.load(Ordering::Relaxed)
    }

    /// Get a reference to the store (used by test helpers and persistence).
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn store(&self) -> &Arc<Store> {
        &self.store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        assert_results_sorted, assert_search_contains, assert_search_excludes,
        random_normalized_embedding, seed_vectors, TestVectorIndex,
    };

    // -- Priority 1: R-02 Dimension Mismatch (FIRST) --

    #[test]
    fn test_insert_wrong_dimension_128() {
        let tvi = TestVectorIndex::new();
        let emb = vec![0.0f32; 128];
        let result = tvi.vi().insert(1, &emb);
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 128
            })
        ));
    }

    #[test]
    fn test_insert_wrong_dimension_512() {
        let tvi = TestVectorIndex::new();
        let emb = vec![0.0f32; 512];
        let result = tvi.vi().insert(1, &emb);
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 512
            })
        ));
    }

    #[test]
    fn test_insert_wrong_dimension_0() {
        let tvi = TestVectorIndex::new();
        let result = tvi.vi().insert(1, &[]);
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 0
            })
        ));
    }

    #[test]
    fn test_insert_wrong_dimension_383() {
        let tvi = TestVectorIndex::new();
        let emb = vec![0.0f32; 383];
        let result = tvi.vi().insert(1, &emb);
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 383
            })
        ));
    }

    #[test]
    fn test_insert_wrong_dimension_385() {
        let tvi = TestVectorIndex::new();
        let emb = vec![0.0f32; 385];
        let result = tvi.vi().insert(1, &emb);
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 385
            })
        ));
    }

    #[test]
    fn test_search_wrong_dimension() {
        let tvi = TestVectorIndex::new();
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).unwrap();
        let query = vec![0.0f32; 128];
        let result = tvi.vi().search(&query, 10, 32);
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 128
            })
        ));
    }

    #[test]
    fn test_search_filtered_wrong_dimension() {
        let tvi = TestVectorIndex::new();
        let query = vec![0.0f32; 256];
        let result = tvi.vi().search_filtered(&query, 10, 32, &[1]);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[test]
    fn test_insert_correct_dimension_succeeds() {
        let tvi = TestVectorIndex::new();
        let emb = random_normalized_embedding(384);
        let result = tvi.vi().insert(1, &emb);
        assert!(result.is_ok());
    }

    // -- Priority 1: W2 Invalid Embedding Validation --

    #[test]
    fn test_insert_nan_embedding() {
        let tvi = TestVectorIndex::new();
        let mut emb = random_normalized_embedding(384);
        emb[10] = f32::NAN;
        let result = tvi.vi().insert(1, &emb);
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[test]
    fn test_insert_infinity_embedding() {
        let tvi = TestVectorIndex::new();
        let mut emb = random_normalized_embedding(384);
        emb[0] = f32::INFINITY;
        let result = tvi.vi().insert(1, &emb);
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[test]
    fn test_insert_neg_infinity_embedding() {
        let tvi = TestVectorIndex::new();
        let mut emb = random_normalized_embedding(384);
        emb[0] = f32::NEG_INFINITY;
        let result = tvi.vi().insert(1, &emb);
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[test]
    fn test_search_nan_query() {
        let tvi = TestVectorIndex::new();
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).unwrap();
        let mut query = random_normalized_embedding(384);
        query[5] = f32::NAN;
        let result = tvi.vi().search(&query, 10, 32);
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[test]
    fn test_search_infinity_query() {
        let tvi = TestVectorIndex::new();
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).unwrap();
        let mut query = random_normalized_embedding(384);
        query[0] = f32::INFINITY;
        let result = tvi.vi().search(&query, 10, 32);
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    // -- Priority 2: R-01 IdMap Desync --

    #[test]
    fn test_insert_idmap_consistent_with_vector_map() {
        let tvi = TestVectorIndex::new();
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).unwrap();

        assert!(tvi.vi().contains(1));
        let data_id = tvi.store().get_vector_mapping(1).unwrap();
        assert!(data_id.is_some());
    }

    #[test]
    fn test_insert_100_vectors_all_consistent() {
        let tvi = TestVectorIndex::new();
        let ids = seed_vectors(tvi.vi(), tvi.store(), 100);
        for id in &ids {
            assert!(tvi.vi().contains(*id));
            assert!(tvi.store().get_vector_mapping(*id).unwrap().is_some());
        }
    }

    #[test]
    fn test_reembed_idmap_updated() {
        let tvi = TestVectorIndex::new();
        let emb_a = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb_a).unwrap();
        let old_data_id = tvi.store().get_vector_mapping(1).unwrap().unwrap();

        let emb_b = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb_b).unwrap();
        let new_data_id = tvi.store().get_vector_mapping(1).unwrap().unwrap();

        assert_ne!(old_data_id, new_data_id);
        assert!(tvi.vi().contains(1));
    }

    // -- Priority 3: R-03 Filtered Search --

    #[test]
    fn test_filtered_search_restricts_results() {
        let tvi = TestVectorIndex::new();
        let ids = seed_vectors(tvi.vi(), tvi.store(), 10);
        let allowed = vec![ids[0], ids[1], ids[2]];
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search_filtered(&query, 10, 32, &allowed).unwrap();

        for r in &results {
            assert!(
                allowed.contains(&r.entry_id),
                "entry {} not in allowed list",
                r.entry_id
            );
        }
    }

    #[test]
    fn test_filtered_search_empty_allow_list() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 5);
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search_filtered(&query, 10, 32, &[]).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_filtered_search_unknown_ids() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 5);
        let query = random_normalized_embedding(384);
        let results = tvi
            .vi()
            .search_filtered(&query, 10, 32, &[9999, 9998])
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_filtered_search_mixed_known_unknown() {
        let tvi = TestVectorIndex::new();
        let ids = seed_vectors(tvi.vi(), tvi.store(), 5);
        let query = random_normalized_embedding(384);
        let allowed = vec![ids[0], 9999];
        let results = tvi.vi().search_filtered(&query, 10, 32, &allowed).unwrap();
        assert!(results.len() <= 1);
        if !results.is_empty() {
            assert_eq!(results[0].entry_id, ids[0]);
        }
    }

    #[test]
    fn test_filtered_search_exclusion() {
        let tvi = TestVectorIndex::new();
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).unwrap();
        tvi.vi().insert(2, &emb).unwrap(); // same embedding

        // Filter to exclude entry 2
        let results = tvi.vi().search_filtered(&emb, 10, 32, &[1]).unwrap();
        assert_search_contains(&results, 1);
        assert_search_excludes(&results, 2);
    }

    // -- Priority 4: R-06 Re-Embedding --

    #[test]
    fn test_reembed_search_finds_latest() {
        let tvi = TestVectorIndex::new();
        let emb_a = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb_a).unwrap();

        let emb_b = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb_b).unwrap(); // re-embed

        let results = tvi.vi().search(&emb_b, 1, 32).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].entry_id, 1);
    }

    #[test]
    fn test_reembed_contains_still_true() {
        let tvi = TestVectorIndex::new();
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        assert!(tvi.vi().contains(1));
    }

    #[test]
    fn test_reembed_stale_count() {
        let tvi = TestVectorIndex::new();
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        assert_eq!(tvi.vi().stale_count(), 0);
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        assert_eq!(tvi.vi().stale_count(), 1);
    }

    #[test]
    fn test_reembed_point_count_increases() {
        let tvi = TestVectorIndex::new();
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        assert_eq!(tvi.vi().point_count(), 1);
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        assert_eq!(tvi.vi().point_count(), 2);
    }

    #[test]
    fn test_reembed_5_times() {
        let tvi = TestVectorIndex::new();
        for _ in 0..5 {
            tvi.vi()
                .insert(1, &random_normalized_embedding(384))
                .unwrap();
        }
        assert_eq!(tvi.vi().stale_count(), 4);
        assert_eq!(tvi.vi().point_count(), 5);
        assert!(tvi.vi().contains(1));
        assert!(tvi.store().get_vector_mapping(1).unwrap().is_some());
    }

    #[test]
    fn test_reembed_vector_map_updated() {
        let tvi = TestVectorIndex::new();
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        let first = tvi.store().get_vector_mapping(1).unwrap().unwrap();
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        let second = tvi.store().get_vector_mapping(1).unwrap().unwrap();
        assert_ne!(first, second);
    }

    // -- Priority 6: R-07 Empty Index --

    #[test]
    fn test_search_empty_index() {
        let tvi = TestVectorIndex::new();
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 10, 32).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_filtered_empty_index() {
        let tvi = TestVectorIndex::new();
        let query = random_normalized_embedding(384);
        let results = tvi
            .vi()
            .search_filtered(&query, 10, 32, &[1, 2, 3])
            .unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_point_count_empty() {
        let tvi = TestVectorIndex::new();
        assert_eq!(tvi.vi().point_count(), 0);
    }

    #[test]
    fn test_contains_empty() {
        let tvi = TestVectorIndex::new();
        assert!(!tvi.vi().contains(42));
    }

    #[test]
    fn test_stale_count_empty() {
        let tvi = TestVectorIndex::new();
        assert_eq!(tvi.vi().stale_count(), 0);
    }

    // -- Priority 7: R-08 Similarity Scores --

    #[test]
    fn test_self_similarity() {
        let tvi = TestVectorIndex::new();
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).unwrap();
        let results = tvi.vi().search(&emb, 1, 32).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_id, 1);
        assert!(
            (results[0].similarity - 1.0).abs() < 0.01,
            "self-similarity should be ~1.0, got {}",
            results[0].similarity
        );
    }

    #[test]
    fn test_orthogonal_similarity() {
        let tvi = TestVectorIndex::new();
        let mut emb_a = vec![0.0f32; 384];
        emb_a[0] = 1.0;
        let mut emb_b = vec![0.0f32; 384];
        emb_b[1] = 1.0;

        tvi.vi().insert(1, &emb_a).unwrap();
        tvi.vi().insert(2, &emb_b).unwrap();

        let results = tvi.vi().search(&emb_a, 2, 32).unwrap();
        assert_eq!(results[0].entry_id, 1);
        assert!(
            (results[0].similarity - 1.0).abs() < 0.01,
            "self-similarity should be ~1.0, got {}",
            results[0].similarity
        );
        if results.len() > 1 {
            assert!(
                results[1].similarity.abs() < 0.1,
                "orthogonal similarity should be ~0.0, got {}",
                results[1].similarity
            );
        }
    }

    #[test]
    fn test_results_sorted_descending() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 20);
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 10, 32).unwrap();
        assert_results_sorted(&results);
    }

    // -- Priority 9: R-10 Self-Search Validation (AC-13) --

    #[test]
    fn test_self_search_50_entries() {
        let tvi = TestVectorIndex::new();
        let mut embeddings = Vec::new();
        let mut ids = Vec::new();

        for i in 0..50 {
            let emb = random_normalized_embedding(384);
            let entry = unimatrix_store::NewEntry {
                title: format!("Entry {i}"),
                content: format!("Content {i}"),
                topic: "test".to_string(),
                category: "vector".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: unimatrix_store::Status::Active,
                created_by: String::new(),
                feature_cycle: String::new(),
                trust_source: String::new(),
            };
            let eid = tvi.store().insert(entry).unwrap();
            tvi.vi().insert(eid, &emb).unwrap();
            embeddings.push(emb);
            ids.push(eid);
        }

        for (emb, &id) in embeddings.iter().zip(ids.iter()) {
            let results = tvi.vi().search(emb, 1, 32).unwrap();
            assert_eq!(
                results[0].entry_id, id,
                "self-search for entry {id} returned {} instead",
                results[0].entry_id
            );
        }
    }

    // -- AC-02: New Empty Index --

    #[test]
    fn test_new_index_empty() {
        let tvi = TestVectorIndex::new();
        assert_eq!(tvi.vi().point_count(), 0);
        assert!(!tvi.vi().contains(1));
        assert_eq!(tvi.vi().stale_count(), 0);
    }

    #[test]
    fn test_new_index_config() {
        let tvi = TestVectorIndex::new();
        assert_eq!(tvi.vi().config().dimension, 384);
        assert_eq!(tvi.vi().config().max_nb_connection, 16);
    }

    // -- AC-15: Send + Sync --

    #[test]
    fn test_vector_index_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VectorIndex>();
    }

    // -- Edge Cases --

    #[test]
    fn test_search_top_k_zero() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 5);
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 0, 32).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_top_k_larger_than_index() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 3);
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 100, 32).unwrap();
        assert!(results.len() <= 3);
    }

    #[test]
    fn test_search_ef_less_than_top_k() {
        let tvi = TestVectorIndex::new();
        seed_vectors(tvi.vi(), tvi.store(), 10);
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 10, 1).unwrap();
        assert!(!results.is_empty());
    }

    #[test]
    fn test_data_id_uniqueness() {
        let tvi = TestVectorIndex::new();
        let ids = seed_vectors(tvi.vi(), tvi.store(), 100);
        let data_ids: HashSet<u64> = ids
            .iter()
            .map(|&id| tvi.store().get_vector_mapping(id).unwrap().unwrap())
            .collect();
        assert_eq!(data_ids.len(), 100);
    }

    #[test]
    fn test_usize_at_least_8_bytes() {
        assert!(std::mem::size_of::<usize>() >= 8);
    }

    #[test]
    fn test_insert_point_count() {
        let tvi = TestVectorIndex::new();
        assert_eq!(tvi.vi().point_count(), 0);
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .unwrap();
        assert_eq!(tvi.vi().point_count(), 1);
        seed_vectors(tvi.vi(), tvi.store(), 10);
        assert_eq!(tvi.vi().point_count(), 11);
    }
}
