# nxs-002: Vector Index -- Pseudocode Overview

## Component Interaction Map

```
                  lib.rs (C8)
                    |
        re-exports all public types
                    |
    +-------+-------+-------+-------+
    |       |       |       |       |
  config  error   index  persist  filter
  (C3)    (C2)    (C4)    (C6)    (C5)
    |       |       |       |       |
    |       |   VectorIndex |  EntryIdFilter
    |       |   insert/search   |
    |       |       |       |   |
    |       |       +---+---+---+
    |       |           |
    |       +-----+-----+
    |             |
    +------+------+
           |
    store-extension (C7)
    iter_vector_mappings
           |
    unimatrix-store (external)
    VECTOR_MAP table
```

## Data Flow: Insert

```
caller.insert(entry_id, embedding)
  |
  +--> validate: embedding.len() == config.dimension (384)
  +--> validate: no NaN/infinity in embedding
  +--> data_id = next_data_id.fetch_add(1, Relaxed)
  |
  +--> WRITE LOCK hnsw
  |      hnsw.insert_slice((&embedding, data_id as usize))
  |    RELEASE WRITE LOCK
  |
  +--> store.put_vector_mapping(entry_id, data_id)
  |
  +--> WRITE LOCK id_map
  |      if old_data_id = id_map.entry_to_data.insert(entry_id, data_id):
  |          id_map.data_to_entry.remove(old_data_id)  // stale
  |      id_map.data_to_entry.insert(data_id, entry_id)
  |    RELEASE WRITE LOCK
  |
  +--> Ok(())
```

## Data Flow: Search (Unfiltered)

```
caller.search(query, top_k, ef_search)
  |
  +--> validate: query.len() == config.dimension
  +--> validate: no NaN/infinity in query
  |
  +--> effective_ef = max(ef_search, top_k)
  |
  +--> READ LOCK hnsw
  |      if hnsw.get_nb_point() == 0: return Ok(vec![])
  |      neighbours = hnsw.search_filter(&query, top_k, effective_ef, None)
  |    RELEASE READ LOCK
  |
  +--> READ LOCK id_map
  |      results = []
  |      for n in neighbours:
  |          if entry_id = id_map.data_to_entry.get(n.d_id as u64):
  |              results.push(SearchResult { entry_id, similarity: 1.0 - n.distance })
  |    RELEASE READ LOCK
  |
  +--> results.sort_by(|a, b| b.similarity.partial_cmp(&a.similarity))
  +--> Ok(results)
```

## Data Flow: Filtered Search

```
caller.search_filtered(query, top_k, ef_search, allowed_entry_ids)
  |
  +--> validate: query.len() == config.dimension
  +--> validate: no NaN/infinity in query
  |
  +--> READ LOCK id_map
  |      allowed_data_ids = []
  |      for entry_id in allowed_entry_ids:
  |          if data_id = id_map.entry_to_data.get(entry_id):
  |              allowed_data_ids.push(data_id as usize)
  |    RELEASE READ LOCK
  |
  +--> if allowed_data_ids.is_empty(): return Ok(vec![])
  +--> allowed_data_ids.sort()
  +--> filter = EntryIdFilter { allowed_data_ids }
  |
  +--> effective_ef = max(ef_search, top_k)
  |
  +--> READ LOCK hnsw
  |      if hnsw.get_nb_point() == 0: return Ok(vec![])
  |      neighbours = hnsw.search_filter(&query, top_k, effective_ef, Some(&filter))
  |    RELEASE READ LOCK
  |
  +--> READ LOCK id_map
  |      results = map neighbours to SearchResult via data_to_entry
  |    RELEASE READ LOCK
  |
  +--> results.sort_by similarity descending
  +--> Ok(results)
```

## Data Flow: Dump

```
caller.dump(dir)
  |
  +--> READ LOCK hnsw
  |      basename = hnsw.file_dump(dir, "unimatrix")
  |      point_count = hnsw.get_nb_point()
  |    RELEASE READ LOCK
  |
  +--> next = next_data_id.load(Relaxed)
  |
  +--> write metadata file: dir/unimatrix-vector.meta
  |      basename={basename}
  |      point_count={point_count}
  |      dimension={config.dimension}
  |      next_data_id={next}
  |
  +--> Ok(())
```

## Data Flow: Load

```
VectorIndex::load(store, config, dir)
  |
  +--> read metadata file: dir/unimatrix-vector.meta
  |      parse basename, point_count, dimension, next_data_id
  |
  +--> validate: dimension == config.dimension
  |
  +--> hnsw_io = HnswIo::load(dir, basename)
  +--> hnsw = hnsw_io.load_hnsw_with_dist(DistDot)
  |
  +--> mappings = store.iter_vector_mappings()
  +--> id_map = IdMap::new()
  +--> for (entry_id, data_id) in mappings:
  |      id_map.data_to_entry.insert(data_id, entry_id)
  |      id_map.entry_to_data.insert(entry_id, data_id)
  |
  +--> VectorIndex {
  |      hnsw: RwLock::new(hnsw),
  |      store,
  |      config,
  |      next_data_id: AtomicU64::new(next_data_id),
  |      id_map: RwLock::new(id_map),
  |    }
```

## Shared Types

```rust
// SearchResult -- returned by search and search_filtered
pub struct SearchResult {
    pub entry_id: u64,
    pub similarity: f32,
}

// IdMap -- internal bidirectional map
struct IdMap {
    data_to_entry: HashMap<u64, u64>,
    entry_to_data: HashMap<u64, u64>,
}

// VectorConfig -- index construction parameters
pub struct VectorConfig {
    pub dimension: usize,         // 384
    pub max_nb_connection: usize, // 16
    pub ef_construction: usize,   // 200
    pub max_elements: usize,      // 10_000
    pub max_layer: usize,         // 16
    pub default_ef_search: usize, // 32
}
```

## Lock Ordering Convention

To prevent deadlock, locks are always acquired in this order when multiple are needed:
1. hnsw (RwLock)
2. id_map (RwLock)

No method acquires both locks simultaneously except `insert`, which acquires hnsw write lock first, releases it, then acquires id_map write lock. This ensures no deadlock.

## Error Propagation Pattern

All public methods return `Result<T, VectorError>`. Errors from unimatrix-store propagate via `From<StoreError>`. hnsw_rs errors are caught and wrapped in `VectorError::Index(String)`. File I/O errors are wrapped in `VectorError::Persistence(String)`.

## Implementation Order

1. C1 (crate-setup): Cargo.toml + empty lib.rs
2. C2 (error): VectorError enum
3. C3 (config): VectorConfig struct
4. C7 (store-extension): Store::iter_vector_mappings
5. C5 (filter): EntryIdFilter
6. C4 (index): VectorIndex (the big one)
7. C6 (persistence): dump/load
8. C8 (lib): re-exports
9. C9 (test-infra): TestVectorIndex, helpers
