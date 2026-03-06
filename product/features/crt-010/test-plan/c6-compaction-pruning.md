# C6: Vector Index Pruning During Compaction — Test Plan

## Status: Verification test only (no new code)

## Location
Integration test in `crates/unimatrix-server/` or `crates/unimatrix-vector/` test module

## Tests

### T-CP-01: Compaction excludes deprecated entries from HNSW (AC-12)
- Insert: Active entry A with embedding, Deprecated entry B with embedding
- Run compaction (StatusService::run_maintenance or VectorIndex::compact with Active-only entries)
- Verify: B's entry_id absent from VECTOR_MAP after compaction
- Verify: A's entry_id present in VECTOR_MAP after compaction
- Verify: search for B's embedding returns no results (or does not return B)

### T-CP-02: Active successor remains findable post-compaction (R-08 verification)
- Insert: Deprecated X (superseded_by=Y), Active Y — both with embeddings
- Run compaction with Active-only entries (simulates background tick)
- Search for content similar to X's embedding
- Expected: Y appears in results via its own embedding (not via injection — X is gone from HNSW)

### T-CP-03: Compaction with all entries deprecated — empty HNSW (edge case)
- Insert: only Deprecated entries
- Run compaction with empty Active entries list
- Expected: HNSW graph is empty, search returns empty vec, no panic

## Risk Coverage

| Risk | Scenarios | Tests |
|------|-----------|-------|
| R-08 (post-compaction injection) | Successor findable post-compaction | T-CP-02 |
| AC-12 | Deprecated absent from VECTOR_MAP | T-CP-01 |
