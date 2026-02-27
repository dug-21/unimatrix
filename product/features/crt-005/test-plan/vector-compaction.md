# Test Plan: C3 Vector Compaction

## Component

C3: Vector Compaction (`crates/unimatrix-vector/src/index.rs`, `crates/unimatrix-core/src/traits.rs`, `crates/unimatrix-store/src/write.rs`)

## Risks Covered

| Risk | Description | Priority |
|------|-------------|----------|
| R-03 | HNSW graph compaction corrupts the active index | High |
| R-06 | VECTOR_MAP-first compaction ordering | High |
| R-15 | Compaction search result drift | Med |
| R-17 | Trait object safety after signature change | High |
| R-18 | Empty KB edge cases (compaction-specific) | Med |
| R-19 | Concurrent compaction safety | Low |

## Integration Tests (index.rs)

### IT-C3-01: Compact eliminates stale nodes
- Insert 10 entries into VectorIndex
- Mark 3 as stale (insert replacement embeddings, leaving old data_ids orphaned)
- Call compact with the 10 active (entry_id, embedding) pairs
- Assert stale_count() == 0
- Assert point_count() == 10
- Covers: R-03 scenario 1, AC-13

### IT-C3-02: Search results consistent before and after compaction
- Insert entries with known embeddings
- Run search query, record returned entry_ids
- Run compact
- Run same search query
- Assert same set of entry_ids returned (order may differ)
- Covers: R-03 scenario 2, R-15 scenario 1, AC-20

### IT-C3-03: VECTOR_MAP updated after compaction
- Insert entries, compact
- Read VECTOR_MAP from store
- Assert every active entry_id has a corresponding data_id
- Assert data_ids are sequential (0..n)
- Covers: R-06 scenario 3

### IT-C3-04: point_count equals active entries after compaction
- Insert 10 entries, create 5 stale nodes
- Compact with 10 active entries
- Assert point_count() == 10
- Covers: R-03 scenario 4

### IT-C3-05: Compact failure leaves old index intact
- Insert entries with valid embeddings
- Attempt compact with invalid embeddings (e.g., wrong dimension)
- Assert error returned
- Assert stale_count() unchanged (old graph untouched)
- Assert search still works with original results
- Covers: R-03 scenario 5

### IT-C3-06: Insert after compact works correctly
- Insert 5 entries, compact
- Insert 3 new entries
- Search for new entries: assert they are found
- Search for original entries: assert they are still found
- Assert point_count() == 8
- Covers: R-03 scenario 6

### IT-C3-07: Compact with empty embeddings
- Insert entries, then compact with empty Vec
- Assert stale_count() == 0
- Assert point_count() == 0
- Assert VECTOR_MAP is empty
- Covers: R-18, EC-02

### IT-C3-08: Compact with zero stale nodes (harmless rebuild)
- Insert 5 entries (no stale nodes)
- Compact with the same 5 entries
- Assert stale_count() == 0
- Assert point_count() == 5
- Assert search results unchanged
- Covers: R-19 scenario 1

## Integration Tests (write.rs)

### IT-C3-09: rewrite_vector_map single transaction
- Call rewrite_vector_map with 5 mappings
- Read back all 5 mappings: assert correct
- Call rewrite_vector_map with 3 different mappings
- Read back: assert exactly 3 entries (old ones cleared)
- Covers: R-06 scenario 4

### IT-C3-10: rewrite_vector_map with empty mappings
- Call rewrite_vector_map with empty Vec
- Read VECTOR_MAP: assert empty
- Covers: R-06

### IT-C3-11: VECTOR_MAP write failure leaves old data intact
- Write initial VECTOR_MAP entries
- Simulate write failure (if possible at test level) or verify via code review
- Verify old VECTOR_MAP entries still present on failure
- Covers: R-06 scenario 2

## Unit Tests (traits.rs)

### UT-C3-01: VectorStore::compact trait method is object-safe
- Construct Box<dyn VectorStore>
- Verify compact method is callable on trait object
- Compile-time test: if it compiles, it passes
- Covers: R-17 scenario 1

## Similarity Score Verification

### IT-C3-12: Similarity scores within epsilon after compaction
- Insert entries with known embeddings
- Search, record similarity scores
- Compact
- Search again, record similarity scores
- Assert similarity scores within reasonable epsilon (HNSW non-determinism)
- Covers: R-15 scenario 3

## Code Review Checks

### CR-C3-01: VECTOR_MAP write before in-memory swap
- Verify in compact implementation: store.rewrite_vector_map() call precedes hnsw/id_map swap
- Covers: R-06 scenario 1

### CR-C3-02: No direct HNSW mutation during compact
- Verify compact builds a NEW Hnsw instance, does not mutate the old one
- Covers: R-03

## Dependencies

- C2 (f64 scoring): SearchResult.similarity must be f64
- Store::rewrite_vector_map (new method in write.rs)

## Estimated Test Count

- 1 unit test (trait safety)
- 12 integration tests
- ~13 total new tests
