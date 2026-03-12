# nan-002: Test Plan -- embedding-reconstruction

## Component Scope

Post-commit embedding and vector index reconstruction in the import pipeline. Uses existing APIs: `OnnxProvider::new()`, `embed_entries()`, `VectorIndex::new()`, `VectorIndex::insert()`, `VectorIndex::dump()`. Not a new component -- tests validate correct orchestration of existing APIs within the import flow.

## Unit Tests

No unit tests for this component. Embedding and vector APIs are already unit-tested in `unimatrix-embed` and `unimatrix-vector`. The import pipeline's orchestration of these APIs is tested at the integration level.

## Integration Tests

### Vector Index Construction (AC-10)

#### test_import_creates_vector_index
- Import a database with N entries
- Assert vector index files exist in the `vector/` subdirectory of the project data directory
- Assert VectorIndex contains exactly N entries (one per imported entry)
- Risks: R-05

#### test_import_embeddings_are_384_dim
- Import entries, load VectorIndex
- Query a known entry's embedding
- Assert embedding dimension is 384 (all-MiniLM-L6-v2 output)

### Semantic Search Post-Import (AC-11)

#### test_semantic_search_after_import
- Import entries with known content (e.g., "Rust programming language" and "Python web framework")
- Perform semantic search for "Rust"
- Assert the Rust entry ranks higher than the Python entry
- Risks: R-05

### Embedding Batch Boundaries (R-05, R-12)

#### test_embedding_batch_boundary_64
- Import exactly 64 entries (one full batch)
- Assert all 64 entries have embeddings in the VectorIndex

#### test_embedding_batch_boundary_65
- Import exactly 65 entries (one full batch + 1 overflow)
- Assert all 65 entries have embeddings

#### test_embedding_batch_boundary_128
- Import 128 entries (two full batches)
- Assert all 128 entries have embeddings

### Embedding Failure Resilience (R-05, R-09)

#### test_db_valid_after_embedding_phase
- Import entries successfully
- Verify database entries are queryable by ID regardless of vector index state
- Confirms ADR-004 design: DB commit before embedding means DB is always in a valid state post-commit

### Performance (AC-17)

#### test_500_entry_import_under_60_seconds
- Create export with 500 entries (realistic content lengths)
- Time the full import including re-embedding
- Assert total time < 60 seconds
- Risks: R-12
- Note: This test may need to be marked `#[ignore]` for CI environments without GPU acceleration

### VectorIndex Persistence

#### test_vector_index_persisted_to_disk
- Import entries
- Assert `VectorIndex::dump()` was called (vector files exist on disk)
- Reload VectorIndex from disk, perform a search, assert results returned
- Confirms the index survives process restart

## Risk Coverage

| Risk | Tests | Coverage |
|------|-------|----------|
| R-05 (embedding after commit) | test_import_creates_vector_index, test_semantic_search_after_import, test_db_valid_after_embedding_phase | Full |
| R-09 (ONNX unavailable) | Partially covered by test_db_valid_after_embedding_phase; full ONNX-unavailable simulation is environment-dependent | Partial |
| R-12 (performance) | test_500_entry_import_under_60_seconds | Full |

## Notes

- ONNX model availability is an environment concern. Tests that require the model should skip gracefully (`#[ignore]` or conditional) in environments where the model is not cached.
- Embedding quality is not tested -- that is the domain of `unimatrix-embed` unit tests. Import tests only verify correct orchestration (right number of embeddings, right IDs, searchable results).
