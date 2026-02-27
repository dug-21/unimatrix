# Pseudocode: C3 Vector Compaction

## Purpose

Add `VectorIndex::compact` that rebuilds the HNSW graph from active entries only, eliminating stale routing nodes. Add `VectorStore::compact` to the trait. Add `Store::rewrite_vector_map` for atomic VECTOR_MAP replacement.

## Files Modified

- `crates/unimatrix-vector/src/index.rs` -- VectorIndex::compact
- `crates/unimatrix-core/src/traits.rs` -- VectorStore::compact trait method
- `crates/unimatrix-store/src/write.rs` -- Store::rewrite_vector_map

## VectorIndex::compact (index.rs)

```
pub fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<()>:
    // Step 1: Build new HNSW graph
    new_hnsw = Hnsw::<f32, DistDot>::new(
        self.config.max_nb_connection,
        self.config.max_elements,
        self.config.max_layer,
        self.config.ef_construction,
        DistDot,
    )

    // Step 2: Insert all embeddings with sequential data_ids from 0
    new_data_to_entry = HashMap::new()
    new_entry_to_data = HashMap::new()
    vector_map_entries = Vec::new()

    for (idx, (entry_id, embedding)) in embeddings.iter().enumerate():
        data_id = idx as u64
        self.validate_dimension(embedding)?
        self.validate_embedding(embedding)?
        new_hnsw.insert_slice((embedding, data_id as usize))
        new_data_to_entry.insert(data_id, entry_id)
        new_entry_to_data.insert(entry_id, data_id)
        vector_map_entries.push((entry_id, data_id))

    // Step 3: Write VECTOR_MAP first (ADR-004)
    // If this fails, return error -- old graph untouched
    self.store.rewrite_vector_map(&vector_map_entries)?

    // Step 4: Atomic in-memory swap
    {
        hnsw = self.hnsw.write().unwrap_or_else(|e| e.into_inner())
        id_map = self.id_map.write().unwrap_or_else(|e| e.into_inner())
        *hnsw = new_hnsw
        *id_map = IdMap {
            data_to_entry: new_data_to_entry,
            entry_to_data: new_entry_to_data,
        }
    }

    // Step 5: Reset next_data_id
    self.next_data_id.store(embeddings.len() as u64, Ordering::Relaxed)

    Ok(())
```

If embeddings is empty, the compact still works: builds empty HNSW, clears VECTOR_MAP, swaps to empty index. This is a valid edge case (EC-02).

## Store::rewrite_vector_map (write.rs)

```
pub fn rewrite_vector_map(&self, mappings: &[(u64, u64)]) -> Result<()>:
    txn = self.db.begin_write()?

    {
        table = txn.open_table(VECTOR_MAP)?  // mut
        // Remove all existing entries
        let existing_keys: Vec<u64> = table.iter()?
            .map(|r| r.map(|(k, _)| k.value()))
            .collect::<StdResult<Vec<_>, _>>()?

        for key in existing_keys:
            table.remove(key)?

        // Insert new mappings
        for (entry_id, data_id) in mappings:
            table.insert(*entry_id, *data_id)?
    }

    txn.commit()?
    Ok(())
```

Single transaction: all-or-nothing. If any step fails, old VECTOR_MAP remains intact.

## VectorStore::compact Trait Method (traits.rs)

```
pub trait VectorStore: Send + Sync {
    // ... existing methods ...
    fn compact(&self, embeddings: Vec<(u64, Vec<f32>)>) -> Result<(), CoreError>;
}
```

Object-safe: `&self`, no generics, concrete return type.

The implementation delegates to VectorIndex::compact with error mapping.

## Error Handling

- HNSW build failure: error returned, old graph untouched (build-new-then-swap)
- Embedding validation: error before any writes
- VECTOR_MAP write failure: transaction rolls back, no in-memory changes (VECTOR_MAP-first)
- Lock poisoned: panic propagation (bug, not runtime)
- Empty embeddings: valid -- produces empty index

## Key Test Scenarios

1. compact(10 active + 3 stale): stale_count() == 0 after (R-03, AC-13)
2. Same entry_ids in search before/after compact (R-15, AC-20)
3. VECTOR_MAP updated with new data_ids (R-06, AC-13)
4. point_count() == embeddings.len() after (AC-13)
5. Empty embeddings: produces empty index (R-18, EC-02)
6. Compact then insert new entry: new entry searchable (R-03)
7. Compact with zero stale: harmless rebuild (R-19)
8. rewrite_vector_map is single transaction (R-06)
