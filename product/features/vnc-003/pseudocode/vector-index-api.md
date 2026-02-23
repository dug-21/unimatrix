# Pseudocode: C5 VectorIndex API Extension

## File: `crates/unimatrix-vector/src/index.rs`

### New Method: `allocate_data_id`

```
/// Allocate the next HNSW data ID without performing any insertion.
///
/// Used by server's combined write transaction to write VECTOR_MAP
/// in the same transaction as entry insert + audit (GH #14 fix).
/// The returned data_id is unique and monotonically increasing.
pub fn allocate_data_id(&self) -> u64:
    return self.next_data_id.fetch_add(1, Ordering::Relaxed)
```

This is an atomic operation -- no locks, no I/O, no fallibility.

### New Method: `insert_hnsw_only`

```
/// Insert into HNSW index and update IdMap only.
///
/// Skips the VECTOR_MAP write (caller already wrote it in a combined
/// transaction). The data_id must have been allocated via allocate_data_id().
pub fn insert_hnsw_only(&self, entry_id: u64, data_id: u64, embedding: &[f32]) -> Result<()>:
    // 1. Validate embedding dimension
    self.validate_dimension(embedding)?

    // 2. Validate no NaN/Inf
    self.validate_embedding(embedding)?

    // 3. Insert into HNSW (write lock on self.hnsw)
    {
        let hnsw = self.hnsw.write().unwrap_or_else(|e| e.into_inner())
        let data_vec = embedding.to_vec()
        hnsw.insert_slice((&data_vec, data_id as usize))
    }

    // 4. Update IdMap (write lock on self.id_map)
    //    Handle re-embedding case: remove old reverse mapping
    {
        let mut id_map = self.id_map.write().unwrap_or_else(|e| e.into_inner())
        if let Some(old_data_id) = id_map.entry_to_data.insert(entry_id, data_id):
            id_map.data_to_entry.remove(&old_data_id)
        id_map.data_to_entry.insert(data_id, entry_id)
    }

    // NOTE: No VECTOR_MAP write -- caller handles this in their transaction
    return Ok(())
```

### Unchanged: Existing `insert()` method

The existing `VectorIndex::insert()` remains unchanged. It continues to:
1. validate_dimension
2. validate_embedding
3. Allocate data_id via next_data_id.fetch_add
4. Insert into HNSW
5. Write VECTOR_MAP via self.store.put_vector_mapping
6. Update IdMap

Callers that do NOT need combined-transaction control (e.g., non-server callers)
still use `insert()` as before.

### Key Design Notes

- `allocate_data_id` and `insert_hnsw_only` are the decoupled pair that enables
  the server to write VECTOR_MAP inside its own redb write transaction.
- Sequence: allocate_data_id() -> begin_write() -> write VECTOR_MAP -> commit -> insert_hnsw_only()
- If the transaction rolls back after allocate_data_id, the data_id is "leaked"
  (sparse HNSW ID space). This is acceptable per R-13 (low risk).
- If insert_hnsw_only fails after commit, VECTOR_MAP entry exists but HNSW does
  not have the point. Entry is not searchable until server restart. Acceptable per R-14.
