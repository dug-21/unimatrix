# nxs-002: Vector Index -- Test Plan Overview

## Test Strategy

Testing follows the risk-based strategy from `RISK-TEST-STRATEGY.md`. Tests are written in priority order of risk severity. The test-first priority list:

1. **R-02** (Dimension mismatch) -- write FIRST. If dimension validation fails, all other data is corrupt.
2. **R-01** (IdMap desync) -- core data integrity invariant.
3. **R-03** (Filtered search correctness) -- correctness of the primary user-facing feature.
4. **R-06** (Re-embedding stale points) -- common operation with subtle behavior.
5. **R-04** (Persistence round-trip) -- data survival across restarts.
6. **R-07** (Empty index) -- first thing users encounter.
7. **R-08** (Similarity computation) -- downstream consumers depend on score quality.
8. **R-05** (Concurrency) -- code review + functional validation.
9. **R-10** (API misuse) -- self-search validation.
10. **R-09, R-11, R-12** -- lower priority, basic coverage.

## Risk-to-Test Mapping

| Risk | Severity | Test Plan File | Key Tests |
|------|----------|---------------|-----------|
| R-01 | Critical | index.md | IdMap consistency after insert, re-embed, dump/load |
| R-02 | Critical | index.md | Dimension validation on insert + search (FIRST) |
| R-03 | High | index.md | Filtered search inclusion/exclusion |
| R-04 | High | persistence.md | Dump/load round-trip, missing files |
| R-05 | High | index.md | Concurrent insert/search (code review) |
| R-06 | High | index.md | Re-embedding, stale count, search correctness |
| R-07 | Medium | index.md | Empty index for all methods |
| R-08 | Medium | index.md | Known-value similarity scores |
| R-09 | Medium | index.md | Data ID uniqueness |
| R-10 | Medium | index.md | Self-search validation |
| R-11 | Medium | persistence.md | Load with missing/corrupt files |
| R-12 | Low | index.md | Compile-time usize >= 8 bytes |
| IR-01 | -- | index.md | Store write failure during insert |
| IR-02 | -- | store-extension.md | iter_vector_mappings at 0, 1, 100 entries |
| IR-03 | -- | persistence.md | New index with existing VECTOR_MAP |
| EC-01..10 | -- | index.md | Edge cases per risk strategy |

## Test Organization

Tests are organized as standard Rust `#[cfg(test)] mod tests` blocks within each source file. Integration tests that span multiple components are placed in `index.rs` tests since that module orchestrates cross-component behavior.

### Per-Component Test Locations

| Component | Test Location | Type |
|-----------|--------------|------|
| C1 (crate-setup) | Verified by `cargo build --workspace` | Build |
| C2 (error) | `error.rs::tests` | Unit |
| C3 (config) | `config.rs::tests` | Unit |
| C4 (index) | `index.rs::tests` | Unit + Integration |
| C5 (filter) | `filter.rs::tests` | Unit |
| C6 (persistence) | `persistence.rs::tests` | Integration |
| C7 (store-extension) | `unimatrix-store/src/read.rs::tests` | Unit |
| C8 (lib) | Compile-time assertions | Build |
| C9 (test-infra) | Used by all test modules | Infrastructure |

## Coverage Requirements

Per the Risk-Based Test Strategy:
- **Critical risks** (R-01, R-02): 12+ scenarios across insert, re-embed, and load paths.
- **High risks** (R-03 through R-06): 28+ scenarios covering filter correctness, persistence, concurrency, and re-embedding.
- **Medium risks** (R-07 through R-10): 18+ scenarios for empty index, similarity computation, data ID management, and API correctness.
- **Total target**: ~75 test scenarios across all risk levels.

## Test Infrastructure (C9)

All tests use these shared helpers from `test_helpers.rs`:
- `TestVectorIndex` -- creates temp store + VectorIndex, auto-cleanup on drop.
- `random_normalized_embedding(dim)` -- generates random L2-normalized vector.
- `assert_search_contains(results, entry_id)` -- verify entry in results.
- `assert_search_excludes(results, entry_id)` -- verify entry NOT in results.
- `assert_results_sorted(results)` -- verify descending similarity order.
- `seed_vectors(vi, store, count)` -- insert N random vectors with store entries.
