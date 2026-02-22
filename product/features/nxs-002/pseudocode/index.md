# C4: Index Module -- Pseudocode

## Purpose

Core `VectorIndex` struct: hnsw_rs wrapper with insert, search, filtered search, inspection methods.

## File: `crates/unimatrix-vector/src/index.rs`

### Types

```
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicU64, Ordering};
use hnsw_rs::prelude::*;
use anndists::dist::DistDot;
use unimatrix_store::Store;
use crate::config::VectorConfig;
use crate::error::{VectorError, Result};
use crate::filter::EntryIdFilter;

#[derive(Debug, Clone, Copy, PartialEq)]
pub STRUCT SearchResult:
    pub entry_id: u64
    pub similarity: f32    // 1.0 - distance, higher = more similar

STRUCT IdMap:
    data_to_entry: HashMap<u64, u64>   // hnsw data_id -> entry_id
    entry_to_data: HashMap<u64, u64>   // entry_id -> hnsw data_id

IMPL IdMap:
    fn new() -> Self:
        IdMap {
            data_to_entry: HashMap::new(),
            entry_to_data: HashMap::new(),
        }

    fn len(&self) -> usize:
        // entry_to_data has one entry per unique entry_id
        self.entry_to_data.len()

pub STRUCT VectorIndex:
    hnsw: RwLock<Hnsw<'static, f32, DistDot>>
    store: Arc<Store>
    config: VectorConfig
    next_data_id: AtomicU64
    id_map: RwLock<IdMap>
```

### Constructor

```
IMPL VectorIndex:
    pub fn new(store: Arc<Store>, config: VectorConfig) -> Result<Self>:
        hnsw = Hnsw::new(
            config.max_nb_connection,
            config.max_elements,
            config.max_layer,
            config.ef_construction,
            DistDot,
        )

        Ok(VectorIndex {
            hnsw: RwLock::new(hnsw),
            store,
            config,
            next_data_id: AtomicU64::new(0),
            id_map: RwLock::new(IdMap::new()),
        })
```

### Validation Helpers

```
    fn validate_dimension(&self, embedding: &[f32]) -> Result<()>:
        if embedding.len() != self.config.dimension:
            return Err(VectorError::DimensionMismatch {
                expected: self.config.dimension,
                got: embedding.len(),
            })
        Ok(())

    fn validate_embedding(&self, embedding: &[f32]) -> Result<()>:
        for (i, &val) in embedding.iter().enumerate():
            if val.is_nan():
                return Err(VectorError::InvalidEmbedding(
                    format!("NaN at index {i}")
                ))
            if val.is_infinite():
                return Err(VectorError::InvalidEmbedding(
                    format!("infinity at index {i}")
                ))
        Ok(())
```

### Insert

```
    pub fn insert(&self, entry_id: u64, embedding: &[f32]) -> Result<()>:
        // Step 1: Validate
        self.validate_dimension(embedding)?
        self.validate_embedding(embedding)?

        // Step 2: Generate data ID
        data_id = self.next_data_id.fetch_add(1, Ordering::Relaxed)

        // Step 3: Insert into hnsw_rs (write lock)
        {
            hnsw = self.hnsw.write().unwrap_or_else(|e| e.into_inner())
            hnsw.insert_slice((&embedding, data_id as usize))
        }
        // hnsw write lock released here

        // Step 4: Write to VECTOR_MAP
        self.store.put_vector_mapping(entry_id, data_id)?

        // Step 5: Update IdMap (write lock)
        {
            id_map = self.id_map.write().unwrap_or_else(|e| e.into_inner())

            // Handle re-embedding: remove old reverse mapping
            if let Some(old_data_id) = id_map.entry_to_data.insert(entry_id, data_id):
                id_map.data_to_entry.remove(&old_data_id)

            id_map.data_to_entry.insert(data_id, entry_id)
        }

        Ok(())
```

### Search (Unfiltered)

```
    pub fn search(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
    ) -> Result<Vec<SearchResult>>:
        // Step 1: Validate
        self.validate_dimension(query)?
        self.validate_embedding(query)?

        // Step 2: Handle edge cases
        if top_k == 0:
            return Ok(vec![])

        let effective_ef = ef_search.max(top_k)

        // Step 3: Search hnsw_rs (read lock)
        let neighbours;
        {
            hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner())
            if hnsw.get_nb_point() == 0:
                return Ok(vec![])
            neighbours = hnsw.search(&query, top_k, effective_ef)
        }

        // Step 4: Map results (read lock on id_map)
        self.map_neighbours_to_results(&neighbours)

    fn map_neighbours_to_results(
        &self,
        neighbours: &[Neighbour],
    ) -> Result<Vec<SearchResult>>:
        id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner())

        results = Vec::with_capacity(neighbours.len())
        for n in neighbours:
            data_id = n.d_id as u64
            if let Some(&entry_id) = id_map.data_to_entry.get(&data_id):
                results.push(SearchResult {
                    entry_id,
                    similarity: 1.0 - n.distance,
                })
            // else: stale point (re-embedded), skip silently

        results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity).unwrap_or(Ordering::Equal))
        Ok(results)
```

### Search (Filtered)

```
    pub fn search_filtered(
        &self,
        query: &[f32],
        top_k: usize,
        ef_search: usize,
        allowed_entry_ids: &[u64],
    ) -> Result<Vec<SearchResult>>:
        // Step 1: Validate
        self.validate_dimension(query)?
        self.validate_embedding(query)?

        // Step 2: Handle edge cases
        if top_k == 0 || allowed_entry_ids.is_empty():
            return Ok(vec![])

        // Step 3: Translate entry IDs to data IDs (read lock on id_map)
        let allowed_data_ids;
        {
            id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner())
            allowed_data_ids = allowed_entry_ids.iter()
                .filter_map(|&eid| id_map.entry_to_data.get(&eid).map(|&did| did as usize))
                .collect::<Vec<usize>>()
        }

        if allowed_data_ids.is_empty():
            return Ok(vec![])

        // Step 4: Build filter
        filter = EntryIdFilter::new(allowed_data_ids)

        // Step 5: Search hnsw_rs (read lock)
        let effective_ef = ef_search.max(top_k)
        let neighbours;
        {
            hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner())
            if hnsw.get_nb_point() == 0:
                return Ok(vec![])
            neighbours = hnsw.search_filter(&query, top_k, effective_ef, Some(&filter))
        }

        // Step 6: Map results
        self.map_neighbours_to_results(&neighbours)
```

### Inspection Methods

```
    pub fn point_count(&self) -> usize:
        hnsw = self.hnsw.read().unwrap_or_else(|e| e.into_inner())
        hnsw.get_nb_point()

    pub fn contains(&self, entry_id: u64) -> bool:
        id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner())
        id_map.entry_to_data.contains_key(&entry_id)

    pub fn stale_count(&self) -> usize:
        point_count = self.point_count()
        id_map = self.id_map.read().unwrap_or_else(|e| e.into_inner())
        // Stale points = total hnsw points - active mappings
        point_count.saturating_sub(id_map.len())

    // Accessor for persistence module
    pub(crate) fn config(&self) -> &VectorConfig:
        &self.config

    pub(crate) fn store(&self) -> &Arc<Store>:
        &self.store
```

## Design Notes

- **Lock ordering**: hnsw lock is always acquired before id_map lock when both are needed. In `insert`, the hnsw write lock is released before id_map write lock is acquired (no nested locks).
- **Poisoned lock handling**: `unwrap_or_else(|e| e.into_inner())` recovers from poisoned mutexes. If a thread panics while holding a lock, the lock becomes "poisoned". This is acceptable because hnsw_rs operations should not panic, and if they do, the data is likely still valid.
- **Re-embedding**: When `entry_to_data.insert(entry_id, data_id)` returns `Some(old_data_id)`, we remove the old reverse mapping. The old point remains in hnsw_rs (no deletion API) but is invisible to result mapping.
- **Stale point handling in search**: If a `Neighbour.d_id` is not found in `data_to_entry`, it is a stale point from re-embedding. We silently skip it. The entry's current vector will be found via its current data_id.
- **`search` vs `search_filter`**: The unfiltered `search` method uses `hnsw.search()` (no filter parameter). The filtered version uses `hnsw.search_filter()` with `Some(&filter)`.
- **ef_search clamping**: If `ef_search < top_k`, we use `top_k` as the effective ef. This is a correctness requirement from hnsw_rs.

## hnsw_rs API Notes

- `Hnsw::new(max_nb_connection, max_elements, max_layer, ef_construction, dist)` -- order matters!
- `insert_slice((&[f32], usize))` -- takes a tuple of (data slice, id)
- `search(&[f32], knbn, ef_arg)` -- returns `Vec<Neighbour>`
- `search_filter(&[f32], knbn, ef_arg, Option<&dyn FilterT>)` -- filtered search
- `get_nb_point() -> usize` -- number of inserted points
- `Neighbour { d_id: usize, distance: f32, p_id: PointId }` -- d_id is our data_id
