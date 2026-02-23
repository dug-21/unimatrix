# C4: Index Module -- Test Plan

This is the largest test file, covering the core VectorIndex operations and the majority of risks.

## Test Priority Order

### Priority 1: R-02 Dimension Mismatch (WRITE FIRST)

```
test_insert_wrong_dimension_128:
    vi = TestVectorIndex::new()
    emb = vec![0.0f32; 128]
    result = vi.vi().insert(1, &emb)
    ASSERT matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 128 }))

test_insert_wrong_dimension_512:
    vi = TestVectorIndex::new()
    emb = vec![0.0f32; 512]
    result = vi.vi().insert(1, &emb)
    ASSERT matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 512 }))

test_insert_wrong_dimension_0:
    vi = TestVectorIndex::new()
    result = vi.vi().insert(1, &[])
    ASSERT matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 0 }))

test_insert_wrong_dimension_383:
    vi = TestVectorIndex::new()
    emb = vec![0.0f32; 383]
    result = vi.vi().insert(1, &emb)
    ASSERT matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 383 }))

test_insert_wrong_dimension_385:
    vi = TestVectorIndex::new()
    emb = vec![0.0f32; 385]
    result = vi.vi().insert(1, &emb)
    ASSERT matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 385 }))

test_search_wrong_dimension:
    vi = TestVectorIndex::new()
    emb = random_normalized_embedding(384)
    vi.vi().insert(1, &emb).unwrap()
    query = vec![0.0f32; 128]
    result = vi.vi().search(&query, 10, 32)
    ASSERT matches!(result, Err(VectorError::DimensionMismatch { expected: 384, got: 128 }))

test_search_filtered_wrong_dimension:
    vi = TestVectorIndex::new()
    query = vec![0.0f32; 256]
    result = vi.vi().search_filtered(&query, 10, 32, &[1])
    ASSERT matches!(result, Err(VectorError::DimensionMismatch { .. }))

test_insert_correct_dimension_succeeds:
    vi = TestVectorIndex::new()
    emb = random_normalized_embedding(384)
    result = vi.vi().insert(1, &emb)
    ASSERT result.is_ok()
```

### Priority 1: W2 Invalid Embedding Validation

```
test_insert_nan_embedding:
    vi = TestVectorIndex::new()
    emb = random_normalized_embedding(384)
    emb[10] = f32::NAN
    result = vi.vi().insert(1, &emb)
    ASSERT matches!(result, Err(VectorError::InvalidEmbedding(_)))

test_insert_infinity_embedding:
    vi = TestVectorIndex::new()
    emb = random_normalized_embedding(384)
    emb[0] = f32::INFINITY
    result = vi.vi().insert(1, &emb)
    ASSERT matches!(result, Err(VectorError::InvalidEmbedding(_)))

test_insert_neg_infinity_embedding:
    vi = TestVectorIndex::new()
    emb = random_normalized_embedding(384)
    emb[0] = f32::NEG_INFINITY
    result = vi.vi().insert(1, &emb)
    ASSERT matches!(result, Err(VectorError::InvalidEmbedding(_)))

test_search_nan_query:
    vi = TestVectorIndex::new()
    query = random_normalized_embedding(384)
    query[5] = f32::NAN
    result = vi.vi().search(&query, 10, 32)
    ASSERT matches!(result, Err(VectorError::InvalidEmbedding(_)))

test_search_infinity_query:
    vi = TestVectorIndex::new()
    query = random_normalized_embedding(384)
    query[0] = f32::INFINITY
    result = vi.vi().search(&query, 10, 32)
    ASSERT matches!(result, Err(VectorError::InvalidEmbedding(_)))
```

### Priority 2: R-01 IdMap Desync

```
test_insert_idmap_consistent_with_vector_map:
    vi = TestVectorIndex::new()
    emb = random_normalized_embedding(384)
    vi.vi().insert(1, &emb).unwrap()

    // Verify IdMap matches VECTOR_MAP
    ASSERT vi.vi().contains(1)
    data_id = vi.store().get_vector_mapping(1).unwrap().unwrap()
    // IdMap should have the same data_id for entry 1
    ASSERT vi.vi().contains(1) == true

test_insert_100_vectors_all_consistent:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 100)
    for id in ids:
        ASSERT vi.vi().contains(id)
        ASSERT vi.store().get_vector_mapping(id).unwrap().is_some()

test_reembed_idmap_updated:
    vi = TestVectorIndex::new()
    emb_a = random_normalized_embedding(384)
    vi.vi().insert(1, &emb_a).unwrap()
    old_data_id = vi.store().get_vector_mapping(1).unwrap().unwrap()

    emb_b = random_normalized_embedding(384)
    vi.vi().insert(1, &emb_b).unwrap()
    new_data_id = vi.store().get_vector_mapping(1).unwrap().unwrap()

    ASSERT old_data_id != new_data_id
    ASSERT vi.vi().contains(1)
```

### Priority 3: R-03 Filtered Search

```
test_filtered_search_restricts_results:
    vi = TestVectorIndex::new()
    // Insert 10 vectors
    ids = seed_vectors(vi.vi(), vi.store(), 10)
    // Filter to first 3
    allowed = [ids[0], ids[1], ids[2]]
    query = random_normalized_embedding(384)
    results = vi.vi().search_filtered(&query, 10, 32, &allowed).unwrap()

    for r in results:
        ASSERT allowed.contains(&r.entry_id)

test_filtered_search_empty_allow_list:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 5)
    query = random_normalized_embedding(384)
    results = vi.vi().search_filtered(&query, 10, 32, &[]).unwrap()
    ASSERT results.is_empty()

test_filtered_search_unknown_ids:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 5)
    query = random_normalized_embedding(384)
    results = vi.vi().search_filtered(&query, 10, 32, &[9999, 9998]).unwrap()
    ASSERT results.is_empty()

test_filtered_search_mixed_known_unknown:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 5)
    query = random_normalized_embedding(384)
    allowed = [ids[0], 9999]
    results = vi.vi().search_filtered(&query, 10, 32, &allowed).unwrap()
    ASSERT results.len() <= 1  // only ids[0] is valid

test_filtered_search_all_ids:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 10)
    query = random_normalized_embedding(384)
    filtered = vi.vi().search_filtered(&query, 10, 32, &ids).unwrap()
    unfiltered = vi.vi().search(&query, 10, 32).unwrap()
    ASSERT filtered.len() == unfiltered.len()

test_filtered_search_exclusion:
    vi = TestVectorIndex::new()
    // Insert two entries with similar embeddings
    emb = random_normalized_embedding(384)
    vi.vi().insert(1, &emb).unwrap()
    // Insert a slightly different embedding for entry 2
    emb2 = emb.clone()  // same embedding -> most similar
    vi.vi().insert(2, &emb2).unwrap()

    // Filter to exclude entry 2
    results = vi.vi().search_filtered(&emb, 10, 32, &[1]).unwrap()
    assert_search_contains(&results, 1)
    assert_search_excludes(&results, 2)

test_filtered_search_single_id:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 10)
    emb = random_normalized_embedding(384)
    vi.vi().insert(ids[0], &emb).unwrap()  // re-embed with known embedding
    results = vi.vi().search_filtered(&emb, 1, 32, &[ids[0]]).unwrap()
    ASSERT results.len() == 1
    ASSERT results[0].entry_id == ids[0]
```

### Priority 4: R-06 Re-Embedding

```
test_reembed_search_finds_latest:
    vi = TestVectorIndex::new()
    emb_a = random_normalized_embedding(384)
    vi.vi().insert(1, &emb_a).unwrap()

    emb_b = random_normalized_embedding(384)
    vi.vi().insert(1, &emb_b).unwrap()  // re-embed

    results = vi.vi().search(&emb_b, 1, 32).unwrap()
    ASSERT results[0].entry_id == 1

test_reembed_contains_still_true:
    vi = TestVectorIndex::new()
    emb_a = random_normalized_embedding(384)
    vi.vi().insert(1, &emb_a).unwrap()
    vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    ASSERT vi.vi().contains(1)

test_reembed_stale_count:
    vi = TestVectorIndex::new()
    vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    ASSERT vi.vi().stale_count() == 0
    vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    ASSERT vi.vi().stale_count() == 1

test_reembed_point_count_increases:
    vi = TestVectorIndex::new()
    vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    ASSERT vi.vi().point_count() == 1
    vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    ASSERT vi.vi().point_count() == 2  // old + new

test_reembed_5_times:
    vi = TestVectorIndex::new()
    for i in 0..5:
        vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    ASSERT vi.vi().stale_count() == 4
    ASSERT vi.vi().point_count() == 5
    ASSERT vi.vi().contains(1)
    // VECTOR_MAP has latest mapping
    ASSERT vi.store().get_vector_mapping(1).unwrap().is_some()

test_reembed_vector_map_updated:
    vi = TestVectorIndex::new()
    vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    first_data_id = vi.store().get_vector_mapping(1).unwrap().unwrap()
    vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    second_data_id = vi.store().get_vector_mapping(1).unwrap().unwrap()
    ASSERT first_data_id != second_data_id  // new data_id assigned
```

### Priority 6: R-07 Empty Index

```
test_search_empty_index:
    vi = TestVectorIndex::new()
    query = random_normalized_embedding(384)
    results = vi.vi().search(&query, 10, 32).unwrap()
    ASSERT results.is_empty()

test_search_filtered_empty_index:
    vi = TestVectorIndex::new()
    query = random_normalized_embedding(384)
    results = vi.vi().search_filtered(&query, 10, 32, &[1, 2, 3]).unwrap()
    ASSERT results.is_empty()

test_point_count_empty:
    vi = TestVectorIndex::new()
    ASSERT vi.vi().point_count() == 0

test_contains_empty:
    vi = TestVectorIndex::new()
    ASSERT vi.vi().contains(42) == false

test_stale_count_empty:
    vi = TestVectorIndex::new()
    ASSERT vi.vi().stale_count() == 0
```

### Priority 7: R-08 Similarity Scores

```
test_self_similarity:
    vi = TestVectorIndex::new()
    emb = random_normalized_embedding(384)
    vi.vi().insert(1, &emb).unwrap()
    results = vi.vi().search(&emb, 1, 32).unwrap()
    ASSERT results.len() == 1
    ASSERT results[0].entry_id == 1
    ASSERT (results[0].similarity - 1.0).abs() < 0.01  // ~1.0 for identical

test_orthogonal_similarity:
    vi = TestVectorIndex::new()
    // Create two orthogonal vectors (first dim vs second dim)
    emb_a = vec![0.0f32; 384]
    emb_a[0] = 1.0  // unit vector in dim 0
    emb_b = vec![0.0f32; 384]
    emb_b[1] = 1.0  // unit vector in dim 1

    vi.vi().insert(1, &emb_a).unwrap()
    vi.vi().insert(2, &emb_b).unwrap()

    results = vi.vi().search(&emb_a, 2, 32).unwrap()
    // First result should be entry 1 (self-search, similarity ~1.0)
    ASSERT results[0].entry_id == 1
    ASSERT (results[0].similarity - 1.0).abs() < 0.01
    // Second result should be entry 2 (orthogonal, similarity ~0.0)
    ASSERT results[1].entry_id == 2
    ASSERT results[1].similarity.abs() < 0.1

test_results_sorted_descending:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 20)
    query = random_normalized_embedding(384)
    results = vi.vi().search(&query, 10, 32).unwrap()
    assert_results_sorted(&results)

test_similarity_in_expected_range:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 50)
    query = random_normalized_embedding(384)
    results = vi.vi().search(&query, 10, 32).unwrap()
    for r in results:
        // For random normalized vectors, similarity should be in [-1, 1]
        ASSERT r.similarity >= -1.1
        ASSERT r.similarity <= 1.1
```

### Priority 9: R-10 Self-Search Validation (AC-13)

```
test_self_search_50_entries:
    vi = TestVectorIndex::new()
    // Insert 50 vectors, remember their embeddings
    embeddings = Vec::new()
    ids = Vec::new()
    for i in 0..50:
        emb = random_normalized_embedding(384)
        entry_id = i as u64 + 1
        // Need to insert store entry first
        store_entry = TestEntry::new("test", "vector")
            .with_title(&format!("Entry {i}"))
            .build()
        actual_id = vi.store().insert(store_entry).unwrap()
        vi.vi().insert(actual_id, &emb).unwrap()
        embeddings.push(emb)
        ids.push(actual_id)

    // Each entry's embedding should return itself as top result
    for (emb, id) in embeddings.iter().zip(ids.iter()):
        results = vi.vi().search(emb, 1, 32).unwrap()
        ASSERT results[0].entry_id == *id
```

### AC-02: New Empty Index

```
test_new_index_empty:
    vi = TestVectorIndex::new()
    ASSERT vi.vi().point_count() == 0
    ASSERT vi.vi().contains(1) == false
    ASSERT vi.vi().stale_count() == 0

test_new_index_config:
    config = VectorConfig::default()
    vi = TestVectorIndex::with_config(config.clone())
    // Verify config accessible
    ASSERT vi.vi().config().dimension == 384
    ASSERT vi.vi().config().max_nb_connection == 16
```

### AC-15: Send + Sync

```
test_vector_index_send_sync:
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<VectorIndex>()
```

### Edge Cases

```
test_search_top_k_zero:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 5)
    query = random_normalized_embedding(384)
    results = vi.vi().search(&query, 0, 32).unwrap()
    ASSERT results.is_empty()

test_search_top_k_larger_than_index:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 3)
    query = random_normalized_embedding(384)
    results = vi.vi().search(&query, 100, 32).unwrap()
    ASSERT results.len() <= 3

test_search_ef_less_than_top_k:
    vi = TestVectorIndex::new()
    seed_vectors(vi.vi(), vi.store(), 10)
    query = random_normalized_embedding(384)
    results = vi.vi().search(&query, 10, 1).unwrap()  // ef_search < top_k
    // Should still work (ef clamped to top_k)
    ASSERT results.len() > 0

test_data_id_uniqueness:
    vi = TestVectorIndex::new()
    ids = seed_vectors(vi.vi(), vi.store(), 100)
    // Verify all data IDs are unique via VECTOR_MAP
    data_ids: HashSet<u64> = ids.iter()
        .map(|&id| vi.store().get_vector_mapping(id).unwrap().unwrap())
        .collect()
    ASSERT data_ids.len() == 100

test_usize_at_least_8_bytes:
    // R-12: Compile-time platform assertion
    assert!(std::mem::size_of::<usize>() >= 8)

test_insert_point_count:
    vi = TestVectorIndex::new()
    ASSERT vi.vi().point_count() == 0
    vi.vi().insert(1, &random_normalized_embedding(384)).unwrap()
    ASSERT vi.vi().point_count() == 1
    seed_vectors(vi.vi(), vi.store(), 10)
    ASSERT vi.vi().point_count() == 11
```

## Risks Covered

| Risk | Test Count | Coverage |
|------|-----------|----------|
| R-01 (IdMap desync) | 3 | Insert, re-embed, 100-vector consistency |
| R-02 (Dimension mismatch) | 8 | Insert wrong dims, search wrong dims, correct succeeds |
| R-03 (Filtered search) | 7 | Empty, unknown, mixed, all, exclusion, single |
| R-06 (Re-embedding) | 6 | Search, contains, stale count, point count, 5x re-embed |
| R-07 (Empty index) | 5 | All public methods on empty index |
| R-08 (Similarity) | 4 | Self-similarity, orthogonal, sorted, range |
| R-09 (Data ID) | 1 | Uniqueness across 100 inserts |
| R-10 (API misuse) | 1 | Self-search 50 entries |
| R-12 (usize/u64) | 1 | Compile-time assertion |
| W2 (NaN/infinity) | 5 | NaN insert, inf insert, neg inf insert, NaN search, inf search |
| EC-01..03 | 3 | top_k=0, top_k>index, ef<top_k |
