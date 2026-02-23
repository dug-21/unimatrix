# Risk Coverage Report: nxs-002 (Vector Index)

**Date**: 2026-02-22
**Test Framework**: cargo test (Rust built-in)
**Total Tests**: 170 (85 unimatrix-store + 85 unimatrix-vector)
**All Passing**: Yes

---

## Risk-to-Test Mapping

### R-01: IdMap Desync with VECTOR_MAP (CRITICAL)

| Status | Coverage |
|--------|----------|
| MITIGATED | 6 tests |

| Test | Scenario |
|------|----------|
| `test_insert_idmap_consistent_with_vector_map` | Insert single vector, verify IdMap matches VECTOR_MAP |
| `test_insert_100_vectors_all_consistent` | Insert 100 vectors, verify all IdMap entries match VECTOR_MAP |
| `test_reembed_idmap_updated` | Re-embed entry, verify IdMap updated correctly |
| `test_reembed_vector_map_updated` | Re-embed entry, verify VECTOR_MAP overwritten |
| `test_load_idmap_consistent` | Dump + load, verify rebuilt IdMap matches VECTOR_MAP |
| `test_idmap_consistency_full_lifecycle` | Full lifecycle: insert 100, dump, load, re-embed 10, verify all consistent |

### R-02: Dimension Mismatch Silent Corruption (CRITICAL)

| Status | Coverage |
|--------|----------|
| MITIGATED | 8 tests |

| Test | Scenario |
|------|----------|
| `test_insert_wrong_dimension_128` | Insert 128-d vector, verify DimensionMismatch error |
| `test_insert_wrong_dimension_512` | Insert 512-d vector, verify error |
| `test_insert_wrong_dimension_0` | Insert 0-d vector, verify error |
| `test_insert_wrong_dimension_383` | Off-by-one below (383-d) |
| `test_insert_wrong_dimension_385` | Off-by-one above (385-d) |
| `test_search_wrong_dimension` | Search with wrong-dimension query |
| `test_search_filtered_wrong_dimension` | Filtered search with wrong-dimension query |
| `test_insert_correct_dimension_succeeds` | Valid 384-d insert succeeds |

### R-03: Filtered Search Returns Wrong Results (HIGH)

| Status | Coverage |
|--------|----------|
| MITIGATED | 5 tests |

| Test | Scenario |
|------|----------|
| `test_filtered_search_restricts_results` | Insert 10, filter to 3, verify only those 3 appear |
| `test_filtered_search_empty_allow_list` | Empty allow-list returns empty |
| `test_filtered_search_unknown_ids` | Filter with unmapped IDs returns empty |
| `test_filtered_search_mixed_known_unknown` | Mix of mapped/unmapped, only mapped appear |
| `test_filtered_search_exclusion` | Exclude most similar entry, verify it's gone |

### R-04: Persistence Round-Trip Data Loss (HIGH)

| Status | Coverage |
|--------|----------|
| MITIGATED | 10 tests |

| Test | Scenario |
|------|----------|
| `test_dump_creates_files` | Verify graph, data, and meta files created |
| `test_dump_metadata_content` | Verify metadata has correct basename, point_count, dimension |
| `test_dump_empty_index` | Empty index dump produces meta file only |
| `test_load_round_trip` | Insert 50, dump, load, verify same search results |
| `test_load_point_count_matches` | Point count preserved after dump/load |
| `test_load_idmap_consistent` | IdMap rebuilt correctly from VECTOR_MAP |
| `test_multi_cycle_dump_load` | Insert, dump, load, insert more, dump, load -- 20 points |
| `test_load_missing_meta_file` | Missing meta -> Persistence error |
| `test_load_missing_graph_file` | Missing graph -> Persistence error |
| `test_load_missing_data_file` | Missing data -> Persistence error |

### R-05: RwLock Deadlock or Starvation (HIGH)

| Status | Coverage |
|--------|----------|
| MITIGATED | Design + 1 test |

Lock ordering enforced by code structure: hnsw lock always acquired before id_map lock.
All public methods follow the same pattern (insert: hnsw write -> id_map write; search: hnsw read -> id_map read).

| Test | Scenario |
|------|----------|
| `test_vector_index_send_sync` | Compile-time proof that VectorIndex is Send + Sync |

### R-06: Re-Embedding Stale Point Corruption (HIGH)

| Status | Coverage |
|--------|----------|
| MITIGATED | 6 tests |

| Test | Scenario |
|------|----------|
| `test_reembed_stale_count` | Stale count increments on re-embed |
| `test_reembed_point_count_increases` | point_count = 2 after re-embed (old + new) |
| `test_reembed_search_finds_latest` | Search finds re-embedded entry with new embedding |
| `test_reembed_contains_still_true` | contains() still true after re-embed |
| `test_reembed_idmap_updated` | IdMap updated: old reverse mapping removed |
| `test_reembed_5_times` | 5 consecutive re-embeds, stale_count = 5 |

### R-07: Empty Index Edge Cases (MEDIUM)

| Status | Coverage |
|--------|----------|
| MITIGATED | 5 tests |

| Test | Scenario |
|------|----------|
| `test_search_empty_index` | Search on empty index returns Ok(vec![]) |
| `test_search_filtered_empty_index` | Filtered search on empty returns Ok(vec![]) |
| `test_point_count_empty` | point_count() == 0 on new index |
| `test_contains_empty` | contains(1) == false on new index |
| `test_stale_count_empty` | stale_count() == 0 on new index |

### R-08: Similarity Score Computation Error (MEDIUM)

| Status | Coverage |
|--------|----------|
| MITIGATED | 3 tests |

| Test | Scenario |
|------|----------|
| `test_self_similarity` | Insert V, search for V, similarity ~= 1.0 (tolerance 0.01) |
| `test_orthogonal_similarity` | Orthogonal vectors have similarity ~= 0.0 |
| `test_results_sorted_descending` | Results sorted by similarity descending |

### R-09: Data ID Overflow or Collision (MEDIUM)

| Status | Coverage |
|--------|----------|
| MITIGATED | 2 tests |

| Test | Scenario |
|------|----------|
| `test_data_id_uniqueness` | 100 inserts, all data_ids unique in VECTOR_MAP |
| `test_usize_at_least_8_bytes` | Compile-time assert: `size_of::<usize>() >= 8` |

### R-10: hnsw_rs API Misuse (MEDIUM)

| Status | Coverage |
|--------|----------|
| MITIGATED | 4 tests |

| Test | Scenario |
|------|----------|
| `test_new_index_empty` | new() creates valid index |
| `test_new_index_config` | Config values propagated correctly |
| `test_self_search_50_entries` | 50-entry end-to-end insert + search |
| `test_search_top_k_zero` | top_k=0 returns empty (no panic) |

### R-11: Load Fails with Corrupt/Missing Files (MEDIUM)

| Status | Coverage |
|--------|----------|
| MITIGATED | 5 tests |

| Test | Scenario |
|------|----------|
| `test_load_missing_meta_file` | Persistence error on missing metadata |
| `test_load_missing_graph_file` | Persistence error on missing graph |
| `test_load_missing_data_file` | Persistence error on missing data |
| `test_load_nonexistent_directory` | Persistence error on nonexistent dir |
| `test_load_empty_directory` | Persistence error on empty dir |

### R-12: usize/u64 Cast Boundary (LOW)

| Status | Coverage |
|--------|----------|
| MITIGATED | 1 test |

| Test | Scenario |
|------|----------|
| `test_usize_at_least_8_bytes` | Compile-time size assertion |

---

## Additional Edge Case Tests

| Test | Category |
|------|----------|
| `test_insert_nan_embedding` | InvalidEmbedding error for NaN (W2) |
| `test_insert_infinity_embedding` | InvalidEmbedding error for infinity (W2) |
| `test_insert_neg_infinity_embedding` | InvalidEmbedding error for -infinity (W2) |
| `test_search_nan_query` | InvalidEmbedding error for NaN query |
| `test_search_infinity_query` | InvalidEmbedding error for infinity query |
| `test_search_top_k_larger_than_index` | Returns all available (no error) |
| `test_search_ef_less_than_top_k` | ef_search < top_k still returns results |
| `test_load_dimension_mismatch` | Dimension mismatch between metadata and config |
| `test_new_index_with_existing_vector_map` | Fresh index ignores existing VECTOR_MAP |

---

## Store Extension Tests (C7: iter_vector_mappings)

| Test | Scenario |
|------|----------|
| `test_iter_vector_mappings_empty` | Empty table returns empty vec |
| `test_iter_vector_mappings_populated` | 3 mappings, all returned |
| `test_iter_vector_mappings_after_overwrite` | Overwritten mapping returns latest |
| `test_iter_vector_mappings_consistency_with_get` | Bulk iter matches per-key get |
| `test_iter_vector_mappings_after_delete` | Deleted mapping excluded |

---

## Test Infrastructure Tests (C9)

| Test | Scenario |
|------|----------|
| `test_random_embedding_dimension` | Correct dimension (128, 384, 768) |
| `test_random_embedding_l2_normalized` | L2 norm ~= 1.0 |
| `test_random_embedding_not_all_zeros` | Not zero vector |
| `test_random_embeddings_different` | Two calls produce different vectors |

---

## Summary

| Risk | Priority | Tests | Status |
|------|----------|-------|--------|
| R-01 (IdMap desync) | Critical | 6 | MITIGATED |
| R-02 (dimension mismatch) | Critical | 8 | MITIGATED |
| R-03 (filtered search) | High | 5 | MITIGATED |
| R-04 (persistence) | High | 10 | MITIGATED |
| R-05 (deadlock) | High | 1 + design | MITIGATED |
| R-06 (re-embedding) | High | 6 | MITIGATED |
| R-07 (empty index) | Medium | 5 | MITIGATED |
| R-08 (similarity) | Medium | 3 | MITIGATED |
| R-09 (data ID) | Medium | 2 | MITIGATED |
| R-10 (API misuse) | Medium | 4 | MITIGATED |
| R-11 (load failures) | Medium | 5 | MITIGATED |
| R-12 (usize/u64) | Low | 1 | MITIGATED |

**All 12 risks mitigated. 85 vector tests + 5 store extension tests = 90 new tests, all passing.**
