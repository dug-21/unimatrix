# nxs-002: Vector Index -- Acceptance Map

**Date**: 2026-02-22

Maps each acceptance criterion to implementing component(s), test type, risk mitigation, and verification method.

---

## AC-01: Crate Compiles in Workspace

| Attribute | Value |
|-----------|-------|
| Component | C1 (crate-setup) |
| Test type | Build verification |
| Risks mitigated | None (structural) |
| Verification | `cargo build --workspace` succeeds with zero errors. `crates/unimatrix-vector/Cargo.toml` exists. Crate is `edition = "2024"`, `#![forbid(unsafe_code)]` present. |

## AC-02: VectorIndex::new Creates Empty Index

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Unit |
| Risks mitigated | R-10 (API misuse) |
| Verification | Create `VectorIndex::new(store, VectorConfig::default())`. Assert `point_count() == 0`. Assert `contains(1) == false`. Assert config values match (dimension=384, M=16, ef_construction=200). |

## AC-03: Insert Writes hnsw_rs + VECTOR_MAP + IdMap

| Attribute | Value |
|-----------|-------|
| Component | C4 (index), C7 (store-extension) |
| Test type | Integration |
| Risks mitigated | R-01 (IdMap desync) |
| Verification | Insert a 384-d vector for entry_id=1. Verify: `point_count() == 1`, `contains(1) == true`, `Store::get_vector_mapping(1).unwrap()` returns `Some(data_id)`, IdMap forward lookup `entry_to_data[1]` matches VECTOR_MAP. Insert 100 vectors and verify consistency for all. |

## AC-04: Search Returns entry_id + similarity, Sorted Descending

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Integration |
| Risks mitigated | R-08 (similarity computation), R-10 (API misuse) |
| Verification | Insert 10 vectors with known embeddings. Search with a query. Verify results contain `entry_id` and `similarity` fields. Verify similarity values are in descending order. Verify similarity is in range [0.0, 1.0] for normalized vectors. Self-search: insert vector V for entry E, search for V, verify E is top result with similarity ~= 1.0. |

## AC-05: Filtered Search Restricts to Allow-List

| Attribute | Value |
|-----------|-------|
| Component | C4 (index), C5 (filter) |
| Test type | Integration |
| Risks mitigated | R-03 (filtered search correctness) |
| Verification | Insert 10 vectors. Call `search_filtered` with 3 allowed entry IDs. Verify only those 3 appear in results. Test with empty allow-list (returns empty). Test with IDs that have no vector mappings (returns empty). Test with all IDs (same as unfiltered). Test exclusion: insert entries A and B with similar embeddings, filter to allow only A, verify B is excluded. |

## AC-06: Dimension Mismatch Returns Error

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Unit |
| Risks mitigated | R-02 (dimension mismatch) |
| Verification | **Write this test FIRST.** Attempt insert with 128-d vector: verify `VectorError::DimensionMismatch { expected: 384, got: 128 }`. Same for 512-d, 0-d, 383-d, 385-d. Attempt search with wrong-dimension query: verify same error. Insert valid 384-d vector succeeds. |

## AC-07: Empty Index Search Returns Empty Vec

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Unit |
| Risks mitigated | R-07 (empty index) |
| Verification | Create new VectorIndex. Call `search(query, 10, 32)`. Verify `Ok(Vec::new())`. Call `search_filtered(query, 10, 32, &[1,2,3])`. Verify `Ok(Vec::new())`. No panic. |

## AC-08: Re-Embedding Updates VECTOR_MAP, Old Point Benign

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Integration |
| Risks mitigated | R-06 (re-embedding stale points) |
| Verification | Insert entry E with embedding A. Re-insert E with embedding B (different). Verify: `contains(E) == true`, VECTOR_MAP has new data_id, `stale_count() == 1`, `point_count() == 2`. Search for B: verify E appears as top result. Search for A: verify E still appears (old point maps to E). |

## AC-09: Dump Produces Index Files

| Attribute | Value |
|-----------|-------|
| Component | C6 (persistence) |
| Test type | Integration |
| Risks mitigated | R-04 (persistence) |
| Verification | Insert 50 vectors. Call `dump(dir)`. Verify files exist: `*.hnsw.graph`, `*.hnsw.data`, `unimatrix-vector.meta`. Verify metadata file contains correct point_count and dimension. |

## AC-10: Load Restores Index, Search Works After Reload

| Attribute | Value |
|-----------|-------|
| Component | C6 (persistence) |
| Test type | Integration |
| Risks mitigated | R-04 (persistence), R-01 (IdMap desync) |
| Verification | Insert 50 vectors. Record search results for 5 queries. Dump. Load into new VectorIndex. Run same 5 queries. Verify results match (same entry_ids, similar scores within f32 tolerance). Verify `point_count()` matches. Verify IdMap consistent with VECTOR_MAP. |

## AC-11: point_count Returns Correct Count

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Unit |
| Risks mitigated | R-09 (data ID management) |
| Verification | New index: `point_count() == 0`. Insert 1: `point_count() == 1`. Insert 10 more: `point_count() == 11`. Re-embed 1: `point_count() == 12` (stale point counted). |

## AC-12: Similarity Scores for Known Vectors

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Unit |
| Risks mitigated | R-08 (similarity computation) |
| Verification | Insert normalized vector V. Search for V: similarity ~= 1.0 (within 0.001 tolerance). Create orthogonal normalized vector W. Insert W. Search for V: similarity of W ~= 0.0 (within 0.1 tolerance). Verify similarity is computed as `1.0 - distance`. |

## AC-13: Self-Search Returns Entry as Top Result

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Integration |
| Risks mitigated | R-10 (API misuse) |
| Verification | Insert 50 unique random normalized vectors. For each entry, search for that entry's embedding with top_k=1. Verify the entry itself is the top result. This validates correct hnsw_rs integration and ID mapping end-to-end. |

## AC-14: Typed Result Errors (No Panics)

| Attribute | Value |
|-----------|-------|
| Component | C2 (error) |
| Test type | Unit |
| Risks mitigated | R-11 (load failures) |
| Verification | Verify each `VectorError` variant is constructible and has meaningful `Display` output. Verify `From<StoreError>` conversion works. Verify `DimensionMismatch`, `Persistence`, `InvalidEmbedding` errors are returned for the appropriate conditions. Load from non-existent path returns `Persistence`. |

## AC-15: VectorIndex is Send + Sync

| Attribute | Value |
|-----------|-------|
| Component | C4 (index) |
| Test type | Compile-time |
| Risks mitigated | R-05 (concurrency) |
| Verification | `fn assert_send_sync<T: Send + Sync>() {} assert_send_sync::<VectorIndex>();` compiles. |

## AC-16: Test Infrastructure

| Attribute | Value |
|-----------|-------|
| Component | C9 (test-infra) |
| Test type | Structural |
| Risks mitigated | None (quality) |
| Verification | `TestVectorIndex` exists with `new()` and `vi()` methods. `random_normalized_embedding(384)` returns 384-d L2-normalized vector. `assert_search_contains(results, entry_id)` helper exists. `seed_vectors(vi, 50)` helper exists. Accessible via `test-support` feature. |

## AC-17: forbid(unsafe_code)

| Attribute | Value |
|-----------|-------|
| Component | C1 (crate-setup), C8 (lib) |
| Test type | Compile-time |
| Risks mitigated | None (safety) |
| Verification | `#![forbid(unsafe_code)]` present in `lib.rs`. Any `unsafe` block causes compilation failure. Verified by `grep -r "unsafe" crates/unimatrix-vector/src/` returning only the forbid attribute. |

## AC-18: IdMap Consistent After Insert, Dump, Load

| Attribute | Value |
|-----------|-------|
| Component | C4 (index), C6 (persistence) |
| Test type | Integration |
| Risks mitigated | R-01 (IdMap desync) |
| Verification | Insert 100 vectors. For each entry_id, verify `IdMap.entry_to_data[entry_id]` matches `Store::get_vector_mapping(entry_id)`. Dump and load. Repeat verification on loaded index. Re-embed 10 entries. Repeat verification. All checks must pass. |

---

## Summary Matrix

| AC | Component(s) | Test Type | Risk(s) |
|----|-------------|-----------|---------|
| AC-01 | C1 | Build | -- |
| AC-02 | C4 | Unit | R-10 |
| AC-03 | C4, C7 | Integration | R-01 |
| AC-04 | C4 | Integration | R-08, R-10 |
| AC-05 | C4, C5 | Integration | R-03 |
| AC-06 | C4 | Unit | R-02 |
| AC-07 | C4 | Unit | R-07 |
| AC-08 | C4 | Integration | R-06 |
| AC-09 | C6 | Integration | R-04 |
| AC-10 | C6 | Integration | R-04, R-01 |
| AC-11 | C4 | Unit | R-09 |
| AC-12 | C4 | Unit | R-08 |
| AC-13 | C4 | Integration | R-10 |
| AC-14 | C2 | Unit | R-11 |
| AC-15 | C4 | Compile-time | R-05 |
| AC-16 | C9 | Structural | -- |
| AC-17 | C1, C8 | Compile-time | -- |
| AC-18 | C4, C6 | Integration | R-01 |
