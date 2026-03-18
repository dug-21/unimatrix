use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use anndists::dist::DistDot;
use hnsw_rs::prelude::*;
use unimatrix_store::SqlxStore;

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
    /// Promoted from f32 to f64 in crt-005 for scoring pipeline precision.
    pub similarity: f64,
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
    store: Arc<SqlxStore>,
    config: VectorConfig,
    next_data_id: AtomicU64,
    id_map: RwLock<IdMap>,
}

impl VectorIndex {
    /// Create a new empty vector index.
    ///
    /// The index starts in insert mode with zero vectors.
    pub fn new(store: Arc<SqlxStore>, config: VectorConfig) -> Result<Self> {
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
        store: Arc<SqlxStore>,
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
                return Err(VectorError::InvalidEmbedding(format!("NaN at index {i}")));
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
    pub async fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<()> {
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
        self.store.put_vector_mapping(entry_id, data_id).await?;

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
                .filter_map(|&eid| id_map.entry_to_data.get(&eid).map(|&did| did as usize))
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
    fn map_neighbours_to_results(&self, neighbours: &[Neighbour]) -> Result<Vec<SearchResult>> {
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
                        // Cast order matters (R-04): promote f32 distance to f64 first,
                        // then subtract from f64 1.0. NOT (1.0_f32 - distance) as f64.
                        similarity: 1.0_f64 - n.distance as f64,
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

    /// Retrieve the stored embedding for an entry.
    ///
    /// Returns `None` if the entry has no vector mapping or the underlying
    /// HNSW point data cannot be retrieved. Iterates ALL layers via `IterPoint`
    /// (IntoIterator for &PointIndexation) to find the point by its data_id
    /// (origin_id). This is O(n) but called infrequently (only during
    /// supersession injection, crt-010).
    ///
    /// IMPORTANT: hnsw_rs assigns each point to a single layer at insertion
    /// time (randomly, probability ~1/M per level). A point at level L exists
    /// ONLY in points_by_layer[L] — not in layer 0. `get_layer_iterator(0)`
    /// therefore misses ~6% of points (those assigned level >= 1). The
    /// IntoIterator impl (IterPoint) traverses all layers from 0 through
    /// entry_point_level, covering every inserted point. (GH#286)
    pub fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>> {
        let data_id = {
            let id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner());
            id_map.entry_to_data.get(&entry_id).copied()?
        };

        let hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner());
        let point_indexation = hnsw.get_point_indexation();

        // Iterate all layers (IterPoint via IntoIterator) to find by origin_id.
        // get_layer_iterator(0) was wrong — points are stored at their assigned
        // layer only, not always at layer 0. (bugfix GH#286)
        for point in point_indexation {
            if point.get_origin_id() == data_id as usize {
                return Some(point.get_v().to_vec());
            }
        }

        None
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
    pub(crate) fn hnsw_read(&self) -> std::sync::RwLockReadGuard<'_, Hnsw<'static, f32, DistDot>> {
        self.hnsw.read().unwrap_or_else(|e| e.into_inner())
    }

    /// Get the current next_data_id value (for persistence dump).
    pub(crate) fn next_data_id_value(&self) -> u64 {
        self.next_data_id.load(Ordering::Relaxed)
    }

    /// Get a reference to the store (used by test helpers and persistence).
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) fn store(&self) -> &Arc<SqlxStore> {
        &self.store
    }

    /// Allocate the next HNSW data ID without performing any insertion.
    ///
    /// Used by the server's combined write transaction to write VECTOR_MAP
    /// in the same transaction as entry insert + audit (GH #14 fix).
    /// The returned data_id is unique and monotonically increasing.
    pub fn allocate_data_id(&self) -> u64 {
        self.next_data_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Rebuild the HNSW graph from the provided active entry embeddings,
    /// eliminating stale routing nodes. Uses build-new-then-swap (ADR-004).
    ///
    /// Steps:
    /// 1. Build a new HNSW graph with sequential data_ids starting from 0
    /// 2. Write VECTOR_MAP first (single transaction) -- if this fails, old graph untouched
    /// 3. Atomically swap in-memory graph and IdMap
    /// 4. Reset next_data_id
    ///
    /// Embeddings remain f32 (HNSW domain). The caller obtains embeddings
    /// via the embed service and passes pre-computed pairs.
    pub async fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<()> {
        // Step 1: Build new HNSW graph
        let new_hnsw = Hnsw::<f32, DistDot>::new(
            self.config.max_nb_connection,
            self.config.max_elements,
            self.config.max_layer,
            self.config.ef_construction,
            DistDot,
        );

        let mut new_data_to_entry = HashMap::with_capacity(embeddings.len());
        let mut new_entry_to_data = HashMap::with_capacity(embeddings.len());
        let mut vector_map_entries = Vec::with_capacity(embeddings.len());

        for (idx, (entry_id, embedding)) in embeddings.iter().enumerate() {
            let data_id = idx as u64;
            self.validate_dimension(embedding)?;
            self.validate_embedding(embedding)?;
            new_hnsw.insert_slice((embedding, data_id as usize));
            new_data_to_entry.insert(data_id, *entry_id);
            new_entry_to_data.insert(*entry_id, data_id);
            vector_map_entries.push((*entry_id, data_id));
        }

        // Step 2: Write VECTOR_MAP first (ADR-004: VECTOR_MAP-first ordering)
        // If this fails, return error -- old graph untouched
        self.store.rewrite_vector_map(&vector_map_entries).await?;

        // Step 3: Atomic in-memory swap
        {
            let mut hnsw = self.hnsw.write().unwrap_or_else(|e| e.into_inner());
            let mut id_map = self.id_map.write().unwrap_or_else(|e| e.into_inner());
            *hnsw = new_hnsw;
            *id_map = IdMap {
                data_to_entry: new_data_to_entry,
                entry_to_data: new_entry_to_data,
            };
        }

        // Step 4: Reset next_data_id
        self.next_data_id
            .store(embeddings.len() as u64, Ordering::Relaxed);

        Ok(())
    }

    /// Insert into HNSW index and update IdMap only.
    ///
    /// Skips the VECTOR_MAP write (caller already wrote it in a combined
    /// transaction). The `data_id` must have been allocated via [`allocate_data_id`].
    pub fn insert_hnsw_only(&self, entry_id: u64, data_id: u64, embedding: &[f32]) -> Result<()> {
        self.validate_dimension(embedding)?;
        self.validate_embedding(embedding)?;

        // Insert into hnsw_rs
        {
            let hnsw = self.hnsw.write().unwrap_or_else(|e| e.into_inner());
            let data_vec = embedding.to_vec();
            hnsw.insert_slice((&data_vec, data_id as usize));
        }

        // Update IdMap (no VECTOR_MAP write)
        {
            let mut id_map = self.id_map.write().unwrap_or_else(|e| e.into_inner());
            if let Some(old_data_id) = id_map.entry_to_data.insert(entry_id, data_id) {
                id_map.data_to_entry.remove(&old_data_id);
            }
            id_map.data_to_entry.insert(data_id, entry_id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{
        TestVectorIndex, assert_results_sorted, assert_search_contains, assert_search_excludes,
        random_normalized_embedding, seed_vectors,
    };

    // -- Priority 1: R-02 Dimension Mismatch (FIRST) --

    #[tokio::test]
    async fn test_insert_wrong_dimension_128() {
        let tvi = TestVectorIndex::new().await;
        let emb = vec![0.0f32; 128];
        let result = tvi.vi().insert(1, &emb).await;
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 128
            })
        ));
    }

    #[tokio::test]
    async fn test_insert_wrong_dimension_512() {
        let tvi = TestVectorIndex::new().await;
        let emb = vec![0.0f32; 512];
        let result = tvi.vi().insert(1, &emb).await;
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 512
            })
        ));
    }

    #[tokio::test]
    async fn test_insert_wrong_dimension_0() {
        let tvi = TestVectorIndex::new().await;
        let result = tvi.vi().insert(1, &[]).await;
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 0
            })
        ));
    }

    #[tokio::test]
    async fn test_insert_wrong_dimension_383() {
        let tvi = TestVectorIndex::new().await;
        let emb = vec![0.0f32; 383];
        let result = tvi.vi().insert(1, &emb).await;
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 383
            })
        ));
    }

    #[tokio::test]
    async fn test_insert_wrong_dimension_385() {
        let tvi = TestVectorIndex::new().await;
        let emb = vec![0.0f32; 385];
        let result = tvi.vi().insert(1, &emb).await;
        assert!(matches!(
            result,
            Err(VectorError::DimensionMismatch {
                expected: 384,
                got: 385
            })
        ));
    }

    #[tokio::test]
    async fn test_search_wrong_dimension() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).await.unwrap();
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

    #[tokio::test]
    async fn test_search_filtered_wrong_dimension() {
        let tvi = TestVectorIndex::new().await;
        let query = vec![0.0f32; 256];
        let result = tvi.vi().search_filtered(&query, 10, 32, &[1]);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[tokio::test]
    async fn test_insert_correct_dimension_succeeds() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);
        let result = tvi.vi().insert(1, &emb).await;
        assert!(result.is_ok());
    }

    // -- Priority 1: W2 Invalid Embedding Validation --

    #[tokio::test]
    async fn test_insert_nan_embedding() {
        let tvi = TestVectorIndex::new().await;
        let mut emb = random_normalized_embedding(384);
        emb[10] = f32::NAN;
        let result = tvi.vi().insert(1, &emb).await;
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[tokio::test]
    async fn test_insert_infinity_embedding() {
        let tvi = TestVectorIndex::new().await;
        let mut emb = random_normalized_embedding(384);
        emb[0] = f32::INFINITY;
        let result = tvi.vi().insert(1, &emb).await;
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[tokio::test]
    async fn test_insert_neg_infinity_embedding() {
        let tvi = TestVectorIndex::new().await;
        let mut emb = random_normalized_embedding(384);
        emb[0] = f32::NEG_INFINITY;
        let result = tvi.vi().insert(1, &emb).await;
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[tokio::test]
    async fn test_search_nan_query() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).await.unwrap();
        let mut query = random_normalized_embedding(384);
        query[5] = f32::NAN;
        let result = tvi.vi().search(&query, 10, 32);
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[tokio::test]
    async fn test_search_infinity_query() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).await.unwrap();
        let mut query = random_normalized_embedding(384);
        query[0] = f32::INFINITY;
        let result = tvi.vi().search(&query, 10, 32);
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    // -- Priority 2: R-01 IdMap Desync --

    #[tokio::test]
    async fn test_insert_idmap_consistent_with_vector_map() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).await.unwrap();

        assert!(tvi.vi().contains(1));
        let data_id = tvi.store().get_vector_mapping(1).await.unwrap();
        assert!(data_id.is_some());
    }

    #[tokio::test]
    async fn test_insert_100_vectors_all_consistent() {
        let tvi = TestVectorIndex::new().await;
        let ids = seed_vectors(tvi.vi(), tvi.store(), 100).await;
        for id in &ids {
            assert!(tvi.vi().contains(*id));
            assert!(tvi.store().get_vector_mapping(*id).await.unwrap().is_some());
        }
    }

    #[tokio::test]
    async fn test_reembed_idmap_updated() {
        let tvi = TestVectorIndex::new().await;
        let emb_a = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb_a).await.unwrap();
        let old_data_id = tvi.store().get_vector_mapping(1).await.unwrap().unwrap();

        let emb_b = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb_b).await.unwrap();
        let new_data_id = tvi.store().get_vector_mapping(1).await.unwrap().unwrap();

        assert_ne!(old_data_id, new_data_id);
        assert!(tvi.vi().contains(1));
    }

    // -- Priority 3: R-03 Filtered Search --

    #[tokio::test]
    async fn test_filtered_search_restricts_results() {
        let tvi = TestVectorIndex::new().await;
        let ids = seed_vectors(tvi.vi(), tvi.store(), 10).await;
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

    #[tokio::test]
    async fn test_filtered_search_empty_allow_list() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 5).await;
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search_filtered(&query, 10, 32, &[]).unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_filtered_search_unknown_ids() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 5).await;
        let query = random_normalized_embedding(384);
        let results = tvi
            .vi()
            .search_filtered(&query, 10, 32, &[9999, 9998])
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_filtered_search_mixed_known_unknown() {
        let tvi = TestVectorIndex::new().await;
        let ids = seed_vectors(tvi.vi(), tvi.store(), 5).await;
        let query = random_normalized_embedding(384);
        let allowed = vec![ids[0], 9999];
        let results = tvi.vi().search_filtered(&query, 10, 32, &allowed).unwrap();
        assert!(results.len() <= 1);
        if !results.is_empty() {
            assert_eq!(results[0].entry_id, ids[0]);
        }
    }

    #[tokio::test]
    async fn test_filtered_search_exclusion() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).await.unwrap();
        tvi.vi().insert(2, &emb).await.unwrap(); // same embedding

        // Filter to exclude entry 2
        let results = tvi.vi().search_filtered(&emb, 10, 32, &[1]).unwrap();
        assert_search_contains(&results, 1);
        assert_search_excludes(&results, 2);
    }

    // -- Priority 4: R-06 Re-Embedding --

    #[tokio::test]
    async fn test_reembed_search_finds_latest() {
        let tvi = TestVectorIndex::new().await;
        let emb_a = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb_a).await.unwrap();

        let emb_b = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb_b).await.unwrap(); // re-embed

        let results = tvi.vi().search(&emb_b, 1, 32).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].entry_id, 1);
    }

    #[tokio::test]
    async fn test_reembed_contains_still_true() {
        let tvi = TestVectorIndex::new().await;
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        assert!(tvi.vi().contains(1));
    }

    #[tokio::test]
    async fn test_reembed_stale_count() {
        let tvi = TestVectorIndex::new().await;
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        assert_eq!(tvi.vi().stale_count(), 0);
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        assert_eq!(tvi.vi().stale_count(), 1);
    }

    #[tokio::test]
    async fn test_reembed_point_count_increases() {
        let tvi = TestVectorIndex::new().await;
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        assert_eq!(tvi.vi().point_count(), 1);
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        assert_eq!(tvi.vi().point_count(), 2);
    }

    #[tokio::test]
    async fn test_reembed_5_times() {
        let tvi = TestVectorIndex::new().await;
        for _ in 0..5 {
            tvi.vi()
                .insert(1, &random_normalized_embedding(384))
                .await
                .unwrap();
        }
        assert_eq!(tvi.vi().stale_count(), 4);
        assert_eq!(tvi.vi().point_count(), 5);
        assert!(tvi.vi().contains(1));
        assert!(tvi.store().get_vector_mapping(1).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_reembed_vector_map_updated() {
        let tvi = TestVectorIndex::new().await;
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        let first = tvi.store().get_vector_mapping(1).await.unwrap().unwrap();
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        let second = tvi.store().get_vector_mapping(1).await.unwrap().unwrap();
        assert_ne!(first, second);
    }

    // -- Priority 6: R-07 Empty Index --

    #[tokio::test]
    async fn test_search_empty_index() {
        let tvi = TestVectorIndex::new().await;
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 10, 32).unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_filtered_empty_index() {
        let tvi = TestVectorIndex::new().await;
        let query = random_normalized_embedding(384);
        let results = tvi
            .vi()
            .search_filtered(&query, 10, 32, &[1, 2, 3])
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_point_count_empty() {
        let tvi = TestVectorIndex::new().await;
        assert_eq!(tvi.vi().point_count(), 0);
    }

    #[tokio::test]
    async fn test_contains_empty() {
        let tvi = TestVectorIndex::new().await;
        assert!(!tvi.vi().contains(42));
    }

    #[tokio::test]
    async fn test_stale_count_empty() {
        let tvi = TestVectorIndex::new().await;
        assert_eq!(tvi.vi().stale_count(), 0);
    }

    // -- Priority 7: R-08 Similarity Scores --

    #[tokio::test]
    async fn test_self_similarity() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).await.unwrap();
        let results = tvi.vi().search(&emb, 1, 32).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_id, 1);
        assert!(
            (results[0].similarity - 1.0).abs() < 0.01,
            "self-similarity should be ~1.0, got {}",
            results[0].similarity
        );
    }

    #[tokio::test]
    async fn test_orthogonal_similarity() {
        let tvi = TestVectorIndex::new().await;
        let mut emb_a = vec![0.0f32; 384];
        emb_a[0] = 1.0;
        let mut emb_b = vec![0.0f32; 384];
        emb_b[1] = 1.0;

        tvi.vi().insert(1, &emb_a).await.unwrap();
        tvi.vi().insert(2, &emb_b).await.unwrap();

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

    #[tokio::test]
    async fn test_results_sorted_descending() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 20).await;
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 10, 32).unwrap();
        assert_results_sorted(&results);
    }

    // -- Priority 9: R-10 Self-Search Validation (AC-13) --

    #[tokio::test]
    async fn test_self_search_50_entries() {
        let tvi = TestVectorIndex::new().await;
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
            let eid = tvi.store().insert(entry).await.unwrap();
            tvi.vi().insert(eid, &emb).await.unwrap();
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

    #[tokio::test]
    async fn test_new_index_empty() {
        let tvi = TestVectorIndex::new().await;
        assert_eq!(tvi.vi().point_count(), 0);
        assert!(!tvi.vi().contains(1));
        assert_eq!(tvi.vi().stale_count(), 0);
    }

    #[tokio::test]
    async fn test_new_index_config() {
        let tvi = TestVectorIndex::new().await;
        assert_eq!(tvi.vi().config().dimension, 384);
        assert_eq!(tvi.vi().config().max_nb_connection, 16);
    }

    // -- AC-15: Send + Sync --

    #[tokio::test]
    async fn test_vector_index_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<VectorIndex>();
    }

    // -- Edge Cases --

    #[tokio::test]
    async fn test_search_top_k_zero() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 5).await;
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 0, 32).unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_search_top_k_larger_than_index() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 3).await;
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 100, 32).unwrap();
        assert!(results.len() <= 3);
    }

    #[tokio::test]
    async fn test_search_ef_less_than_top_k() {
        let tvi = TestVectorIndex::new().await;
        seed_vectors(tvi.vi(), tvi.store(), 10).await;
        let query = random_normalized_embedding(384);
        let results = tvi.vi().search(&query, 10, 1).unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_data_id_uniqueness() {
        let tvi = TestVectorIndex::new().await;
        let ids = seed_vectors(tvi.vi(), tvi.store(), 100).await;
        let mut data_ids: HashSet<u64> = HashSet::new();
        for &id in &ids {
            let data_id = tvi.store().get_vector_mapping(id).await.unwrap().unwrap();
            data_ids.insert(data_id);
        }
        assert_eq!(data_ids.len(), 100);
    }

    #[tokio::test]
    async fn test_usize_at_least_8_bytes() {
        assert!(std::mem::size_of::<usize>() >= 8);
    }

    // -- C5 vnc-003: allocate_data_id + insert_hnsw_only --

    #[tokio::test]
    async fn test_allocate_data_id_monotonic() {
        let tvi = TestVectorIndex::new().await;
        let mut prev = tvi.vi().allocate_data_id();
        for _ in 0..9 {
            let next = tvi.vi().allocate_data_id();
            assert!(next > prev, "allocate_data_id must be strictly increasing");
            prev = next;
        }
    }

    #[tokio::test]
    async fn test_allocate_data_id_starts_at_zero() {
        let tvi = TestVectorIndex::new().await;
        assert_eq!(tvi.vi().allocate_data_id(), 0);
        assert_eq!(tvi.vi().allocate_data_id(), 1);
    }

    #[tokio::test]
    async fn test_insert_hnsw_only_searchable() {
        let tvi = TestVectorIndex::new().await;
        let data_id = tvi.vi().allocate_data_id();
        let emb = random_normalized_embedding(384);

        // Manually write VECTOR_MAP (simulating server combined txn)
        tvi.store().put_vector_mapping(1, data_id).await.unwrap();

        // Insert into HNSW only
        tvi.vi().insert_hnsw_only(1, data_id, &emb).unwrap();

        // Should be searchable
        let results = tvi.vi().search(&emb, 1, 32).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_id, 1);
    }

    #[tokio::test]
    async fn test_insert_hnsw_only_validates_dimension() {
        let tvi = TestVectorIndex::new().await;
        let data_id = tvi.vi().allocate_data_id();
        let emb = vec![0.0f32; 128]; // wrong dimension
        let result = tvi.vi().insert_hnsw_only(1, data_id, &emb);
        assert!(matches!(result, Err(VectorError::DimensionMismatch { .. })));
    }

    #[tokio::test]
    async fn test_insert_hnsw_only_validates_nan() {
        let tvi = TestVectorIndex::new().await;
        let data_id = tvi.vi().allocate_data_id();
        let mut emb = random_normalized_embedding(384);
        emb[0] = f32::NAN;
        let result = tvi.vi().insert_hnsw_only(1, data_id, &emb);
        assert!(matches!(result, Err(VectorError::InvalidEmbedding(_))));
    }

    #[tokio::test]
    async fn test_insert_hnsw_only_no_vector_map_write() {
        let tvi = TestVectorIndex::new().await;
        let data_id = tvi.vi().allocate_data_id();
        let emb = random_normalized_embedding(384);

        // Do NOT write VECTOR_MAP
        tvi.vi().insert_hnsw_only(1, data_id, &emb).unwrap();

        // VECTOR_MAP should NOT have a mapping (insert_hnsw_only skips it)
        assert!(tvi.store().get_vector_mapping(1).await.unwrap().is_none());
        // But HNSW point count increased
        assert_eq!(tvi.vi().point_count(), 1);
        // And IdMap knows about the entry
        assert!(tvi.vi().contains(1));
    }

    #[tokio::test]
    async fn test_insert_hnsw_only_idmap_updated() {
        let tvi = TestVectorIndex::new().await;
        let data_id = tvi.vi().allocate_data_id();
        let emb = random_normalized_embedding(384);
        tvi.vi().insert_hnsw_only(1, data_id, &emb).unwrap();
        assert!(tvi.vi().contains(1));
    }

    #[tokio::test]
    async fn test_existing_insert_still_works() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);
        tvi.vi().insert(1, &emb).await.unwrap();
        // VECTOR_MAP written
        assert!(tvi.store().get_vector_mapping(1).await.unwrap().is_some());
        // Searchable
        let results = tvi.vi().search(&emb, 1, 32).unwrap();
        assert_eq!(results[0].entry_id, 1);
    }

    #[tokio::test]
    async fn test_allocate_then_insert_hnsw_sequence() {
        let tvi = TestVectorIndex::new().await;
        let emb = random_normalized_embedding(384);

        // Full GH #14 fix sequence:
        // 1. Allocate data_id
        let data_id = tvi.vi().allocate_data_id();
        // 2. Write VECTOR_MAP externally (server's combined txn)
        tvi.store().put_vector_mapping(42, data_id).await.unwrap();
        // 3. Insert into HNSW only
        tvi.vi().insert_hnsw_only(42, data_id, &emb).unwrap();

        // Verify end-to-end
        assert!(tvi.vi().contains(42));
        assert_eq!(
            tvi.store().get_vector_mapping(42).await.unwrap(),
            Some(data_id)
        );
        let results = tvi.vi().search(&emb, 1, 32).unwrap();
        assert_eq!(results[0].entry_id, 42);
    }

    #[tokio::test]
    async fn test_insert_point_count() {
        let tvi = TestVectorIndex::new().await;
        assert_eq!(tvi.vi().point_count(), 0);
        tvi.vi()
            .insert(1, &random_normalized_embedding(384))
            .await
            .unwrap();
        assert_eq!(tvi.vi().point_count(), 1);
        seed_vectors(tvi.vi(), tvi.store(), 10).await;
        assert_eq!(tvi.vi().point_count(), 11);
    }

    // -- crt-005: Vector Compaction Tests --

    // IT-C3-01: Compact eliminates stale nodes
    #[tokio::test]
    async fn test_compact_eliminates_stale_nodes() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        // Insert 10 entries via normal path (creates store entries + vectors)
        let ids = seed_vectors(tvi.vi(), tvi.store(), 10).await;

        // Create stale nodes: re-embed 3 entries (old data_ids become stale)
        for &id in &ids[0..3] {
            let new_emb = random_normalized_embedding(dim);
            tvi.vi().insert(id, &new_emb).await.unwrap();
        }
        assert!(
            tvi.vi().stale_count() > 0,
            "should have stale nodes after reembedding"
        );

        // Collect current embeddings for all 10 entries
        let embeddings: Vec<(u64, Vec<f32>)> = ids
            .iter()
            .map(|&id| (id, random_normalized_embedding(dim)))
            .collect();

        // Compact
        tvi.vi().compact(embeddings).await.unwrap();

        assert_eq!(
            tvi.vi().stale_count(),
            0,
            "stale_count should be 0 after compact"
        );
        assert_eq!(
            tvi.vi().point_count(),
            10,
            "point_count should equal active entries"
        );
    }

    // IT-C3-02: Search results consistent before and after compaction
    // Pre-existing: GH#288 — flaky with 5-point dataset; HNSW non-determinism
    // after compact() causes different result sets ~1/3 of runs.
    #[tokio::test]
    #[ignore = "Pre-existing: GH#288 — flaky, HNSW non-determinism with 5-point dataset"]
    async fn test_compact_search_consistency() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        // Insert entries with known embeddings
        let mut embeddings: Vec<(u64, Vec<f32>)> = Vec::new();
        for i in 0..5 {
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
            let entry_id = tvi.store().insert(entry).await.unwrap();
            let emb = random_normalized_embedding(dim);
            tvi.vi().insert(entry_id, &emb).await.unwrap();
            embeddings.push((entry_id, emb));
        }

        // Search before compaction
        let query = embeddings[0].1.clone();
        let results_before = tvi.vi().search(&query, 5, 32).unwrap();
        let ids_before: HashSet<u64> = results_before.iter().map(|r| r.entry_id).collect();

        // Compact with same embeddings
        tvi.vi().compact(embeddings.clone()).await.unwrap();

        // Search after compaction
        let results_after = tvi.vi().search(&query, 5, 32).unwrap();
        let ids_after: HashSet<u64> = results_after.iter().map(|r| r.entry_id).collect();

        assert_eq!(
            ids_before, ids_after,
            "search results should return same entry_ids after compaction"
        );
    }

    // IT-C3-03: VECTOR_MAP updated after compaction
    #[tokio::test]
    async fn test_compact_vector_map_updated() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        let ids = seed_vectors(tvi.vi(), tvi.store(), 5).await;

        // Compact
        let embeddings: Vec<(u64, Vec<f32>)> = ids
            .iter()
            .map(|&id| (id, random_normalized_embedding(dim)))
            .collect();
        tvi.vi().compact(embeddings).await.unwrap();

        // Verify VECTOR_MAP has sequential data_ids
        for (idx, &entry_id) in ids.iter().enumerate() {
            let data_id = tvi.store().get_vector_mapping(entry_id).await.unwrap();
            assert_eq!(
                data_id,
                Some(idx as u64),
                "data_id should be sequential starting from 0"
            );
        }
    }

    // IT-C3-04: point_count equals active entries after compaction
    #[tokio::test]
    async fn test_compact_point_count() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        // Insert 10, create 5 stale
        let ids = seed_vectors(tvi.vi(), tvi.store(), 10).await;
        for &id in &ids[0..5] {
            tvi.vi()
                .insert(id, &random_normalized_embedding(dim))
                .await
                .unwrap();
        }
        // point_count includes stale nodes
        assert!(tvi.vi().point_count() > 10);

        // Compact with 10 active entries
        let embeddings: Vec<(u64, Vec<f32>)> = ids
            .iter()
            .map(|&id| (id, random_normalized_embedding(dim)))
            .collect();
        tvi.vi().compact(embeddings).await.unwrap();

        assert_eq!(tvi.vi().point_count(), 10);
    }

    // IT-C3-05: Compact failure leaves old index intact
    #[tokio::test]
    async fn test_compact_failure_preserves_old_index() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        let ids = seed_vectors(tvi.vi(), tvi.store(), 5).await;
        let old_stale = tvi.vi().stale_count();

        // Attempt compact with wrong dimension -- should fail
        let bad_embeddings: Vec<(u64, Vec<f32>)> = ids
            .iter()
            .map(|&id| (id, vec![0.0f32; 128])) // wrong dimension
            .collect();
        let result = tvi.vi().compact(bad_embeddings).await;
        assert!(result.is_err(), "compact with wrong dimension should fail");

        // Old index should still work
        assert_eq!(
            tvi.vi().stale_count(),
            old_stale,
            "stale_count unchanged after failed compact"
        );
        let query = random_normalized_embedding(dim);
        let results = tvi.vi().search(&query, 5, 32).unwrap();
        assert!(
            !results.is_empty(),
            "search should still work after failed compact"
        );
    }

    // IT-C3-06: Insert after compact works correctly
    #[tokio::test]
    async fn test_insert_after_compact() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        let ids = seed_vectors(tvi.vi(), tvi.store(), 5).await;

        // Compact
        let embeddings: Vec<(u64, Vec<f32>)> = ids
            .iter()
            .map(|&id| (id, random_normalized_embedding(dim)))
            .collect();
        tvi.vi().compact(embeddings).await.unwrap();

        // Insert 3 new entries
        let new_ids = seed_vectors(tvi.vi(), tvi.store(), 3).await;

        assert_eq!(
            tvi.vi().point_count(),
            8,
            "should have 5 + 3 = 8 after compact then insert"
        );

        // All entries findable
        for &id in &ids {
            assert!(
                tvi.vi().contains(id),
                "original entry {id} should still be present"
            );
        }
        for &id in &new_ids {
            assert!(tvi.vi().contains(id), "new entry {id} should be present");
        }
    }

    // IT-C3-07: Compact with empty embeddings
    #[tokio::test]
    async fn test_compact_empty_embeddings() {
        let tvi = TestVectorIndex::new().await;

        seed_vectors(tvi.vi(), tvi.store(), 5).await;

        // Compact with empty vec
        tvi.vi().compact(Vec::new()).await.unwrap();

        assert_eq!(tvi.vi().stale_count(), 0);
        assert_eq!(tvi.vi().point_count(), 0);
    }

    // IT-C3-08: Compact with zero stale nodes (harmless rebuild)
    #[tokio::test]
    async fn test_compact_no_stale_nodes() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        // Insert entries with known embeddings
        let mut embeddings: Vec<(u64, Vec<f32>)> = Vec::new();
        for i in 0..5 {
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
            let entry_id = tvi.store().insert(entry).await.unwrap();
            let emb = random_normalized_embedding(dim);
            tvi.vi().insert(entry_id, &emb).await.unwrap();
            embeddings.push((entry_id, emb.clone()));
        }

        assert_eq!(tvi.vi().stale_count(), 0, "no stale nodes before compact");

        // Compact with same embeddings
        tvi.vi().compact(embeddings.clone()).await.unwrap();

        assert_eq!(tvi.vi().stale_count(), 0);
        assert_eq!(tvi.vi().point_count(), 5);

        // Search still works
        let results = tvi.vi().search(&embeddings[0].1, 5, 32).unwrap();
        assert!(!results.is_empty());
    }

    // IT-C3-12: Similarity scores within epsilon after compaction
    #[tokio::test]
    async fn test_compact_similarity_scores_stable() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        // Insert entries with known embeddings
        let mut embeddings: Vec<(u64, Vec<f32>)> = Vec::new();
        for i in 0..5 {
            let entry = unimatrix_store::NewEntry {
                title: format!("Sim entry {i}"),
                content: format!("Sim content {i}"),
                topic: "test".to_string(),
                category: "vector".to_string(),
                tags: vec![],
                source: "test".to_string(),
                status: unimatrix_store::Status::Active,
                created_by: String::new(),
                feature_cycle: String::new(),
                trust_source: String::new(),
            };
            let entry_id = tvi.store().insert(entry).await.unwrap();
            let emb = random_normalized_embedding(dim);
            tvi.vi().insert(entry_id, &emb).await.unwrap();
            embeddings.push((entry_id, emb));
        }

        // Search before
        let query = embeddings[0].1.clone();
        let results_before = tvi.vi().search(&query, 5, 32).unwrap();

        // Compact with same embeddings
        tvi.vi().compact(embeddings.clone()).await.unwrap();

        // Search after
        let results_after = tvi.vi().search(&query, 5, 32).unwrap();

        // Similarity scores should be very close (HNSW is approximate)
        // Compare top result similarity -- with same embeddings, should be nearly identical
        if !results_before.is_empty() && !results_after.is_empty() {
            let top_before = results_before[0].similarity;
            let top_after = results_after[0].similarity;
            assert!(
                (top_before - top_after).abs() < 0.01,
                "top similarity should be stable: before={top_before}, after={top_after}"
            );
        }
    }

    // -- GH#286: get_embedding must find points at any layer, not only layer 0 --

    /// Insert enough points that the HNSW layer-assignment RNG almost certainly
    /// places at least one above layer 0 (probability ~1-(15/16)^200 ≈ 1.0).
    /// Then verify get_embedding returns Some(_) for ALL inserted points.
    ///
    /// This is a deterministic regression guard: if get_embedding still used
    /// get_layer_iterator(0) it would return None for points assigned level >= 1,
    /// causing this assertion to fail.
    #[tokio::test]
    async fn test_get_embedding_returns_some_for_all_points_regardless_of_layer() {
        let tvi = TestVectorIndex::new().await;

        // 200 points: probability that ALL land on layer 0 is (15/16)^200 < 10^-6.
        // In practice hnsw_rs uses max_nb_connection=16 => P(level>=1) ~= 1/16 per point.
        let ids = seed_vectors(tvi.vi(), tvi.store(), 200).await;

        let mut missing = Vec::new();
        for &id in &ids {
            if tvi.vi().get_embedding(id).is_none() {
                missing.push(id);
            }
        }

        assert!(
            missing.is_empty(),
            "get_embedding returned None for {} entries: {:?}",
            missing.len(),
            &missing[..missing.len().min(10)]
        );
    }

    /// Verify that get_embedding returns the correct vector (round-trips).
    /// Uses a single known embedding stored via insert_hnsw_only so the
    /// data_id is predictable and the layer assignment is random.
    #[tokio::test]
    async fn test_get_embedding_value_matches_inserted_vector() {
        let tvi = TestVectorIndex::new().await;
        let dim = 384;

        // Insert 50 entries; for each, verify the retrieved embedding is
        // close to the original (dot product ~1.0 for unit vectors).
        let mut embeddings: Vec<(u64, Vec<f32>)> = Vec::new();
        for i in 0..50 {
            let entry = unimatrix_store::NewEntry {
                title: format!("Emb entry {i}"),
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
            let entry_id = tvi.store().insert(entry).await.unwrap();
            let emb = random_normalized_embedding(dim);
            tvi.vi().insert(entry_id, &emb).await.unwrap();
            embeddings.push((entry_id, emb));
        }

        for (entry_id, original) in &embeddings {
            let retrieved = tvi
                .vi()
                .get_embedding(*entry_id)
                .unwrap_or_else(|| panic!("get_embedding returned None for entry {entry_id}"));

            assert_eq!(
                retrieved.len(),
                dim,
                "retrieved embedding has wrong dimension for entry {entry_id}"
            );

            // Dot product of two unit vectors equals cosine similarity; should be ~1.0
            let dot: f32 = original
                .iter()
                .zip(retrieved.iter())
                .map(|(a, b)| a * b)
                .sum();
            assert!(
                dot > 0.99,
                "embedding round-trip mismatch for entry {entry_id}: dot={dot}"
            );
        }
    }
}
