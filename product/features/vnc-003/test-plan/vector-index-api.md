# Test Plan: C5 VectorIndex API Extension

## File: `crates/unimatrix-vector/src/index.rs`

### New Tests

1. **test_allocate_data_id_monotonic** (NEW)
   - Call `allocate_data_id()` 10 times
   - Verify each returned value is strictly increasing
   - Covers R-02 scenario 3

2. **test_allocate_data_id_starts_at_zero** (NEW)
   - New VectorIndex, first `allocate_data_id()` returns 0
   - Second returns 1

3. **test_insert_hnsw_only_searchable** (NEW)
   - Allocate data_id, manually write VECTOR_MAP, call insert_hnsw_only
   - Search for the embedding, verify it is found
   - Covers R-02 scenario 4, R-14 scenario 3

4. **test_insert_hnsw_only_validates_dimension** (NEW)
   - Call insert_hnsw_only with wrong dimension
   - Verify DimensionMismatch error
   - Covers R-14 scenario 1

5. **test_insert_hnsw_only_validates_nan** (NEW)
   - Call insert_hnsw_only with NaN in embedding
   - Verify InvalidEmbedding error

6. **test_insert_hnsw_only_no_vector_map_write** (NEW)
   - Call insert_hnsw_only (without writing VECTOR_MAP)
   - Verify VECTOR_MAP does NOT contain the mapping
   - Verify HNSW point count increased
   - Confirms insert_hnsw_only skips VECTOR_MAP

7. **test_insert_hnsw_only_idmap_updated** (NEW)
   - Call insert_hnsw_only, verify contains() returns true

8. **test_existing_insert_still_works** (NEW)
   - Call the original `insert()` method
   - Verify VECTOR_MAP written and HNSW searchable
   - Covers R-02 scenario 5 (backward compat)

9. **test_allocate_then_insert_hnsw_sequence** (NEW)
   - Allocate data_id, write VECTOR_MAP externally, call insert_hnsw_only
   - Verify end-to-end: entry searchable, VECTOR_MAP present, IdMap consistent
   - Covers R-02 scenarios 1-2

### AC Coverage

| AC | Test |
|----|------|
| AC-30 | test_allocate_then_insert_hnsw_sequence (VECTOR_MAP in txn) |
| AC-32 | test_insert_hnsw_only_searchable (HNSW after commit) |
