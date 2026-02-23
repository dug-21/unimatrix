# Gate 3b Report: Code Review -- nxs-002 Vector Index

**Date**: 2026-02-22
**Result**: PASS

---

## Validation Checklist

### 1. Code Matches Architecture

| Architecture Element | Implementation | Status |
|---------------------|----------------|--------|
| `VectorIndex` struct (RwLock<Hnsw>, Arc<Store>, VectorConfig, AtomicU64, RwLock<IdMap>) | `index.rs:44-54` | PASS |
| `IdMap` bidirectional HashMap | `index.rs:22-28` (data_to_entry + entry_to_data) | PASS |
| `SearchResult` struct (entry_id, similarity) | `index.rs:14-20` | PASS |
| `VectorConfig` struct (dimension, max_nb_connection, ef_construction, max_elements, max_layer, ef_search) | `config.rs:6-23` | PASS |
| `VectorError` enum (7 variants) | `error.rs:6-26` | PASS |
| `EntryIdFilter` implementing FilterT | `filter.rs:1-40` | PASS |
| Persistence: dump/load with metadata | `persistence.rs:16-128` | PASS |
| `Store::iter_vector_mappings()` extension | `read.rs:193-203` (unimatrix-store) | PASS |
| `#![forbid(unsafe_code)]` | `lib.rs:1` | PASS |
| Re-exports: VectorConfig, VectorError, Result, SearchResult, VectorIndex | `lib.rs:12-14` | PASS |
| test_helpers behind `test-support` feature | `lib.rs:9-10` | PASS |

### 2. Code Matches Pseudocode

| Component | Pseudocode File | Deviations | Status |
|-----------|----------------|------------|--------|
| C1: crate-setup | crate-setup.md | Cargo.toml matches (hnsw_rs 0.3 + simdeez_f, anndists 0.1, tempfile 3, rand 0.9) | PASS |
| C2: error | error.md | All 7 variants match pseudocode. Display/Error/From impls present. | PASS |
| C3: config | config.md | Struct fields and defaults match (384, 16, 200, 10000, 16, 32) | PASS |
| C4: index | index.md | Insert flow matches: validate_dimension, validate_embedding, fetch_add, hnsw.insert_slice, put_vector_mapping, update IdMap. Search flow matches: validate, hnsw.search_with_filter(knn_neighbours), map_neighbours_to_results. | PASS |
| C5: filter | filter.md | EntryIdFilter with sorted Vec<usize>, binary_search in hnsw_filter method | PASS |
| C6: persistence | persistence.md | dump: create_dir_all, file_dump, write metadata. load: parse metadata, validate dimension, HnswIo::load_hnsw, rebuild IdMap from iter_vector_mappings. Empty index handling added. | PASS |
| C7: store-extension | store-extension.md | iter_vector_mappings uses ReadableTable::iter on VECTOR_MAP | PASS |
| C8: lib | lib.md | Module structure and re-exports match | PASS |
| C9: test-infra | test-infra.md | TestVectorIndex, random_normalized_embedding, seed_vectors, assertion helpers match | PASS |

### 3. Upstream Dependency Fixes

Two upstream issues were encountered and resolved:

1. **anndists v0.1.4 edition 2024 compilation bug**: The upstream crate declares `edition = "2024"` but has `cfg_if!` + `#[cfg]` block patterns that produce `()` return types. Fixed via local patch in `patches/anndists/`:
   - Replaced `cfg_if!` blocks with explicit `#[cfg]` attributes on separate `fn eval` impls for DistL1, DistL2, DistDot
   - Fixed `scalar_dot_f32` assertion (`assert!(dot >= 0.)` -> `dot.max(0.)`) for floating-point tolerance
   - Workspace `Cargo.toml` uses `[patch.crates-io]` to apply the fix

2. **hnsw_rs `datamap_opt` quirk**: `load_hnsw` unconditionally sets `datamap_opt = true`, causing `file_dump` to generate unique basenames instead of overwriting. Fixed by capturing the actual basename returned by `file_dump` and writing it to metadata.

### 4. Acceptance Criteria Coverage

| AC | Verified By | Status |
|----|------------|--------|
| AC-01 | `cargo build --workspace` succeeds, Cargo.toml exists, edition 2024, forbid(unsafe_code) | PASS |
| AC-02 | test_new_index_empty, test_new_index_config | PASS |
| AC-03 | test_insert_correct_dimension_succeeds, test_insert_idmap_consistent_with_vector_map, test_insert_100_vectors_all_consistent | PASS |
| AC-04 | test_results_sorted_descending, test_self_similarity | PASS |
| AC-05 | test_filtered_search_restricts_results, test_filtered_search_exclusion, test_filtered_search_empty_allow_list, test_filtered_search_unknown_ids, test_filtered_search_mixed_known_unknown | PASS |
| AC-06 | test_insert_wrong_dimension_{0,128,383,385,512}, test_search_wrong_dimension, test_search_filtered_wrong_dimension | PASS |
| AC-07 | test_search_empty_index, test_search_filtered_empty_index | PASS |
| AC-08 | test_reembed_vector_map_updated, test_reembed_stale_count, test_reembed_search_finds_latest, test_reembed_idmap_updated, test_reembed_5_times | PASS |
| AC-09 | test_dump_creates_files, test_dump_metadata_content, test_dump_empty_index | PASS |
| AC-10 | test_load_round_trip, test_load_point_count_matches, test_load_idmap_consistent | PASS |
| AC-11 | test_point_count_empty, test_insert_point_count, test_reembed_point_count_increases | PASS |
| AC-12 | test_self_similarity, test_orthogonal_similarity | PASS |
| AC-13 | test_self_search_50_entries | PASS |
| AC-14 | 11 error variant tests + persistence error tests | PASS |
| AC-15 | test_vector_index_send_sync (compile-time) | PASS |
| AC-16 | TestVectorIndex, helpers, test_random_embedding_* | PASS |
| AC-17 | `#![forbid(unsafe_code)]` in lib.rs, grep confirms no unsafe | PASS |
| AC-18 | test_idmap_consistency_full_lifecycle | PASS |

### 5. Code Quality

- No TODO, FIXME, unimplemented!(), or stub functions
- No unsafe code (forbid enforced)
- Lock ordering: hnsw lock always acquired before id_map lock
- All error paths use typed VectorError variants
- No panics in production code paths (only in tests via unwrap)
- Workspace builds with zero errors, only upstream anndists warning

### 6. Test Results

- **unimatrix-store**: 85 passed, 0 failed
- **unimatrix-vector**: 85 passed, 0 failed
- **Total**: 170 passed, 0 failed

### 7. Files Produced

| File | Lines |
|------|-------|
| `crates/unimatrix-vector/Cargo.toml` | 18 |
| `crates/unimatrix-vector/src/lib.rs` | 15 |
| `crates/unimatrix-vector/src/error.rs` | ~180 |
| `crates/unimatrix-vector/src/config.rs` | ~90 |
| `crates/unimatrix-vector/src/filter.rs` | ~70 |
| `crates/unimatrix-vector/src/index.rs` | ~900 |
| `crates/unimatrix-vector/src/persistence.rs` | ~500 |
| `crates/unimatrix-vector/src/test_helpers.rs` | ~180 |
| `crates/unimatrix-store/src/read.rs` (modified) | +25 lines |
| `Cargo.toml` (workspace, modified) | +5 lines |
| `patches/anndists/` (local patch) | upstream fix |

---

**Gate 3b: PASS**
