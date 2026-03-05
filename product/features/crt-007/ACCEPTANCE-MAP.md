# Acceptance Map: crt-007 Neural Extraction Pipeline

## Acceptance Criteria to Implementation Wave Mapping

| AC | Description | Wave | Test IDs | Verification |
|----|-------------|------|----------|-------------|
| AC-01 | unimatrix-learn crate exists with TrainingReservoir<T>, EwcState, ModelRegistry, persistence helpers | 1, 4 | T-R02-1..5 | Crate compiles, types exported |
| AC-02 | unimatrix-adapt uses shared TrainingReservoir<T> and EwcState from learn | 1 | T-R02-1..4 | No duplicated implementations; adapt imports from learn |
| AC-03 | All existing unimatrix-adapt tests pass after refactoring | 1 | T-R02-5 | `cargo test -p unimatrix-adapt` all green |
| AC-04 | Signal Classifier MLP with baseline weights via ruv-fann | 3 | T-R01-1..3, T-R07-1, T-R07-3 | Forward pass produces valid 5-class distribution |
| AC-05 | Convention Scorer MLP with baseline weights via ruv-fann | 3 | T-R01-1..3, T-R07-2 | Forward pass produces [0,1] score |
| AC-06 | SignalDigest struct defined with all input features | 2 | T-R03-1..4 | 32-slot array, normalization verified |
| AC-07 | Classifier inference < 50ms | 3 | T-R09-1 | Benchmarked |
| AC-08 | Scorer inference < 10ms | 3 | T-R09-2 | Benchmarked |
| AC-09 | Shadow mode runs without affecting stored entries | 5, 6 | T-R04-1, T-R05-3 | Integration test: entries unchanged after shadow prediction |
| AC-10 | Shadow evaluation logs persist predictions | 5 | T-R04-1..3 | SQLite records queryable |
| AC-11 | ModelRegistry manages production/shadow/previous slots | 4 | T-R06-1..5 | Promotion, rollback, retention tested |
| AC-12 | Auto-rollback on >5% accuracy drop | 4 | T-R04-4 | Synthetic accuracy degradation triggers rollback |
| AC-13 | Models stored in {project_hash}/models/{model_name}/ | 4 | T-R06-1..3 | File paths verified |
| AC-14 | Baseline weights bias toward noise/low scores | 3 | T-R07-1..3 | Strong signal classified correctly; weak signal biased to noise |
| AC-15 | trust_source "neural" -> 0.40 in confidence scoring | 6 | -- | Unit test in confidence.rs |
| AC-16 | Neural-enhanced entries use trust_source "neural" | 6 | T-R05-3 | Integration test verifies trust_source field |
| AC-17 | Unit tests for classifier, scorer, shadow eval, registry | 2-5 | T-R01..R07 | All unit tests pass |
| AC-18 | Integration test: end-to-end shadow mode pipeline | 6 | T-R05-3, T-R09-3 | Synthetic observations -> rules -> digest -> predict -> log |

## Wave to Acceptance Criteria Coverage

| Wave | Acceptance Criteria Covered | Gate Criteria |
|------|---------------------------|--------------|
| 1 | AC-01 (partial), AC-02, AC-03 | All adapt tests pass |
| 2 | AC-06 | SignalDigest normalization verified |
| 3 | AC-04, AC-05, AC-07, AC-08, AC-14 | Models produce valid output with correct bias |
| 4 | AC-01 (complete), AC-11, AC-12, AC-13 | Registry promotion/rollback works |
| 5 | AC-09 (partial), AC-10 | Shadow evaluations persist to SQLite |
| 6 | AC-09 (complete), AC-15, AC-16, AC-17, AC-18 | End-to-end integration test passes |

## Risk to Acceptance Criteria Traceability

| Risk | ACs Affected | Test Coverage |
|------|-------------|---------------|
| R-01 (ruv-fann) | AC-04, AC-05, AC-07, AC-08 | T-R01-1..5 (P0 gate) |
| R-02 (adapt refactoring) | AC-02, AC-03 | T-R02-1..5 (P0 gate) |
| R-03 (SignalDigest stability) | AC-06 | T-R03-1..4 |
| R-04 (shadow accuracy) | AC-10, AC-12 | T-R04-1..4 |
| R-05 (col-013 integration) | AC-09, AC-16, AC-18 | T-R05-1..3 |
| R-06 (disk footprint) | AC-11, AC-13 | T-R06-1..5 |
| R-07 (conservative bias) | AC-14 | T-R07-1..4 |
| R-08 (schema migration) | AC-10 | T-R08-1..4 |
| R-09 (performance) | AC-07, AC-08 | T-R09-1..4 |

## Unverified Criteria (Deferred to crt-008)

None. All 18 acceptance criteria are fully verifiable within crt-007 scope. Shadow mode evaluation quality (whether models actually improve extraction accuracy) is a crt-008 concern -- crt-007 only verifies the shadow infrastructure works, not that models produce superior predictions.
