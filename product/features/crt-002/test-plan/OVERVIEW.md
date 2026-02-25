# Test Plan Overview: crt-002 Confidence Evolution

## Risk-to-Test Mapping

| Risk ID | Risk | Test Component | Test Type |
|---------|------|---------------|-----------|
| R-01 | Wilson score numerical instability | confidence-module | Unit |
| R-02 | Confidence not updated on all mutation paths | server-mutation-integration | Integration |
| R-03 | Combined transaction failure | store-confidence | Integration |
| R-04 | Re-ranking inverts search results | search-reranking | Unit + Integration |
| R-05 | Weight sum invariant violation | confidence-module | Unit |
| R-06 | update_confidence triggers index diffs | store-confidence | Unit |
| R-07 | Freshness NaN/infinity edge cases | confidence-module | Unit |
| R-08 | Component function out-of-range | confidence-module | Unit |
| R-09 | Confidence function panic in transaction | store-confidence | Unit |
| R-10 | New Status variant not handled | confidence-module | Unit (exhaustive match) |
| R-11 | Existing crt-001 tests break | All | Regression (cargo test) |
| R-12 | f64-to-f32 cast boundary | confidence-module | Unit |

## Test Infrastructure

### Existing Infrastructure (Reused)
- `unimatrix-store` test fixtures: `make_test_record()`, `test_store()` helpers
- `unimatrix-server` integration test patterns from crt-001
- `tempfile::TempDir` for isolated store instances

### New Test Patterns
- Pure function unit tests (no setup, no teardown) for all confidence components
- EntryRecord construction helpers with specific field values for confidence testing

## Test Execution Strategy

1. Unit tests first: all confidence-module pure functions (C1)
2. Store tests: update_confidence and record_usage_with_confidence (C2)
3. Integration tests: mutation paths (C4), retrieval path (C3), search re-ranking (C5)
4. Regression: full `cargo test --workspace` to catch R-11 breakage
