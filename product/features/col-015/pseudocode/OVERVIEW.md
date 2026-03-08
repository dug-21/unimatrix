# Pseudocode Overview: col-015

## Components

| Component | Crate | Files |
|-----------|-------|-------|
| shared-fixtures | unimatrix-engine | src/test_scenarios.rs, Cargo.toml, src/lib.rs |
| calibration-tests | unimatrix-engine | tests/pipeline_calibration.rs, tests/pipeline_retrieval.rs |
| regression-tests | unimatrix-engine | tests/pipeline_regression.rs |
| extraction-tests | unimatrix-observe | tests/extraction_pipeline.rs |
| server-e2e-tests | unimatrix-server | src/test_support.rs, tests/pipeline_e2e.rs, Cargo.toml, src/lib.rs |

## Data Flow

```
shared-fixtures (test_scenarios.rs)
  |-- EntryProfile, CalibrationScenario, RetrievalScenario
  |-- kendall_tau(), assertion helpers
  |-- Standard profiles + scenarios
  |
  +-> calibration-tests (import via unimatrix_engine::test_scenarios)
  |     Uses profiles + scenarios + assertion helpers
  |
  +-> regression-tests (import via unimatrix_engine::test_scenarios)
  |     Uses profiles + CANONICAL_NOW for golden values
  |
  +-> server-e2e-tests (import via unimatrix_engine::test_scenarios)
        Uses profiles for entry seeding

extraction-tests (standalone)
  |-- Uses unimatrix_observe::extraction directly
  |-- Uses unimatrix_store::test_helpers::TestDb for store seeding
  |-- No dependency on shared-fixtures
```

## Shared Types

- `EntryProfile`: Deterministic entry signal description
- `CalibrationScenario`: Entries + expected confidence ordering
- `RetrievalScenario` / `RetrievalEntry`: Entries with content + optional embedding
- `CANONICAL_NOW: u64 = 1_700_000_000`

## Build Order

1. shared-fixtures (foundation)
2. calibration-tests + regression-tests (depend on shared-fixtures)
3. extraction-tests (independent)
4. server-e2e-tests (depends on shared-fixtures + server internals)

## Integration Harness Plan

col-015 creates its own Rust-native test infrastructure. It does NOT add Python integration tests to infra-001. The feature validates pipeline internals via unit and integration tests in Rust. Existing infra-001 smoke tests should continue passing since no production code changes.
