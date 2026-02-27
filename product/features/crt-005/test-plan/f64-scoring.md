# Test Plan: C2 f64 Scoring Constants

## Component

C2: f64 Scoring Upgrade (`crates/unimatrix-server/src/confidence.rs`, `crates/unimatrix-server/src/coaccess.rs`, `crates/unimatrix-vector/src/index.rs`, `crates/unimatrix-store/src/write.rs`)

## Risks Covered

| Risk | Description | Priority |
|------|-------------|----------|
| R-02 | Residual f32 constants after f64 sweep | Critical |
| R-04 | f64 precision loss at f32/f64 cast boundary | Med |
| R-11 | Existing test suite regression from f64 promotion | High |
| R-14 | Weight sum invariant after f64 promotion | High |
| R-17 | Trait object safety after signature change | High |

## Unit Tests (confidence.rs)

### UT-C2-01: Weight sum invariant (f64)
- Assert W_BASE + W_USAGE + W_FRESH + W_HELP + W_CORR + W_TRUST == 0.92 exactly
- Assert W_COAC == 0.08 exactly
- Assert (0.92 + 0.08) == 1.0 exactly
- Covers: R-14 scenarios 1-3

### UT-C2-02: compute_confidence returns f64 with full precision
- Call compute_confidence with known inputs
- Assert result has precision beyond 7 decimal digits
- Verify no truncation to f32 representation
- Covers: R-02 scenario 4

### UT-C2-03: compute_confidence all-max returns 1.0
- Construct EntryRecord where all 6 stored components score maximum
- Assert compute_confidence returns exactly 1.0 (within f64::EPSILON)
- Covers: R-14 scenario 5

### UT-C2-04: compute_confidence all-zero returns 0.0
- Construct EntryRecord where all components score 0
- Assert compute_confidence returns 0.0
- Covers: R-14 scenario 6

### UT-C2-05: rerank_score accepts and returns f64
- Call rerank_score(0.123456789012345_f64, 0.987654321098765_f64)
- Verify output preserves f64 precision (not truncated to f32)
- Covers: R-02 scenario 5, R-04 scenario 5

### UT-C2-06: co_access_affinity returns f64
- Call co_access_affinity with known inputs
- Verify return type is f64 and precision is preserved
- Covers: R-02

## Unit Tests (coaccess.rs)

### UT-C2-07: MAX_CO_ACCESS_BOOST is f64
- Assert MAX_CO_ACCESS_BOOST == 0.03_f64 exactly
- Assert MAX_BRIEFING_CO_ACCESS_BOOST == 0.01_f64 exactly
- Covers: R-02 scenario 3

### UT-C2-08: compute_search_boost returns HashMap<u64, f64>
- Call compute_search_boost with test data
- Verify all boost values are f64 with expected precision
- Covers: R-02

### UT-C2-09: compute_briefing_boost returns HashMap<u64, f64>
- Call compute_briefing_boost with test data
- Verify all boost values are f64
- Covers: R-02

## Unit Tests (index.rs)

### UT-C2-10: SearchResult.similarity is f64
- Insert embeddings into VectorIndex, search
- Assert SearchResult.similarity is f64 type
- Verify precision characteristic of f64 (not truncated f32)
- Covers: R-02 scenario 7, R-04

### UT-C2-11: Cast order verification -- distance 0.1
- HNSW returns distance=0.1 (f32)
- Verify similarity == (1.0_f64 - 0.1_f32 as f64)
- NOT (1.0_f32 - 0.1_f32) as f64
- Covers: R-04 scenario 1

### UT-C2-12: Cast order verification -- distance boundaries
- HNSW returns distance=0.0: verify similarity == 1.0_f64 exactly
- HNSW returns distance=1.0: verify similarity == 0.0_f64 exactly
- Covers: R-04 scenarios 2-3

## Unit Tests (write.rs)

### UT-C2-13: update_confidence roundtrip f64
- Store value 0.123456789012345_f64 via update_confidence
- Read entry back
- Assert confidence == 0.123456789012345_f64 exactly
- Covers: R-02 scenario 6

## Unit Tests (traits.rs)

### UT-C2-14: Trait object safety with compact
- Construct Box<dyn VectorStore> with a concrete type implementing compact
- Verify compile succeeds (compile-time test)
- Covers: R-17 scenarios 1, 4

## Regression Tests

### RT-C2-01: Full workspace test suite
- Run `cargo test --workspace` after all C2 changes
- Assert all 811+ tests pass
- No tests disabled (#[ignore] or #[cfg(skip)])
- Covers: R-11 scenarios 1-8

### RT-C2-02: No residual f32 in scoring pipeline
- Grep confidence.rs, coaccess.rs for `f32` type annotations or constants
- Only legitimate f32: none in these files after C2
- Grep index.rs for `as f32` in map_neighbours_to_results: should have none
- Covers: R-02 scenario 8

## Existing Test Updates (~60-80 tests)

The following existing tests require mechanical updates:

### confidence.rs test updates
- All weight constant assertions: f32 -> f64
- compute_confidence return value assertions: f32::EPSILON -> f64::EPSILON
- rerank_score test inputs and assertions: f32 -> f64
- co_access_affinity test inputs and assertions: f32 -> f64
- Hardcoded confidence values: remove `_f32` suffix or change to `_f64`

### coaccess.rs test updates
- MAX_CO_ACCESS_BOOST assertions: f32 -> f64
- compute_search_boost/compute_briefing_boost HashMap value assertions: f32 -> f64

### index.rs test updates
- SearchResult.similarity assertions: f32 -> f64
- Similarity comparison epsilon: f32::EPSILON -> f64::EPSILON

### schema.rs test updates
- EntryRecord construction: `confidence: 0.0_f32` -> `confidence: 0.0` (f64 default)
- Any f32 confidence comparison assertions

### tools.rs test updates
- StatusReport construction sites: all confidence-related fields become f64
- Any f32 assertions in scoring path tests

## Dependencies

- C1 (schema migration): EntryRecord.confidence must be f64 before C2 tests run

## Estimated Test Count

- 14 new unit tests
- 60-80 existing tests mechanically updated
- 1 regression verification (`cargo test --workspace`)
