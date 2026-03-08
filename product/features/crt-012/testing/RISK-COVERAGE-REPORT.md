# crt-012: Risk Coverage Report

## Test Results Summary

### Unit Tests
- unimatrix-learn: 73 passed (69 existing + 4 new seed tests)
- unimatrix-adapt: 52 passed (65 original - 13 removed duplicates + 1 new T-COMPAT-01)
- Total unit: 125 passed, 0 failed

### Integration Tests
- unimatrix-learn retraining_e2e: 1 passed
- Downstream crates (server, engine, observe): 1272 passed, 0 failed
- Full workspace: all crates compile and pass

### New Tests Added
| Test ID | Location | Validates | Result |
|---------|----------|-----------|--------|
| T-SEED-01 | unimatrix-learn/src/models/classifier.rs | AC-06: baseline_seed(42) == baseline() | PASS |
| T-SEED-02 | unimatrix-learn/src/models/scorer.rs | AC-07: baseline_seed(123) == baseline() | PASS |
| T-SEED-03 | unimatrix-learn/src/models/classifier.rs | AC-11: seed(42) != seed(99) | PASS |
| T-SEED-04 | unimatrix-learn/src/models/scorer.rs | AC-11: seed(123) != seed(99) | PASS |
| T-COMPAT-01 | unimatrix-adapt/src/config.rs | AC-08: old format deserializes with defaults | PASS |

### Tests Removed (Duplicates)
| Test IDs | Location | Canonical Coverage |
|----------|----------|--------------------|
| T-REG-01 to T-REG-09 | unimatrix-adapt/src/regularization.rs | unimatrix-learn/src/ewc.rs tests |
| T-TRN-01 to T-TRN-03, T-TRN-10 | unimatrix-adapt/src/training.rs | unimatrix-learn/src/reservoir.rs tests |

## Risk Coverage

| Risk | Severity | Mitigation | Test Coverage | Status |
|------|----------|-----------|---------------|--------|
| R-01: Bincode compat | High | `#[serde(default)]` on new fields | T-COMPAT-01, T-PER-01, T-SVC-10 | MITIGATED |
| R-02: Import path breakage | Medium | Re-export preserves `crate::regularization::EwcState` | Compilation success, persistence.rs compiles | MITIGATED |
| R-03: TrainingReservoir API mismatch | Medium | Tuple-to-TrainingPair conversion at call site | T-SVC-03, T-SVC-07, T-TRN-14/15/16 | MITIGATED |
| R-04: Model init seed regression | High | Mechanical refactoring: extract body to _seed variant | T-SEED-01, T-SEED-02, T-CS-01, T-CS-04 | MITIGATED |
| R-05: Unused dep activation | Low | No feature flags, clean compilation | cargo check -p unimatrix-adapt | MITIGATED |

## Acceptance Criteria Verification

| AC | Description | Status | Evidence |
|----|-------------|--------|----------|
| AC-01 | EwcState deduplication | PASS | regularization.rs is 2-line re-export |
| AC-02 | TrainingReservoir deduplication | PASS | No struct def in adapt/training.rs, imported from learn |
| AC-03 | AdaptConfig reservoir_seed | PASS | Field added, wired in service, T-COMPAT-01 |
| AC-04 | AdaptConfig init_seed | PASS | Field added, wired to MicroLoRA::with_seed() |
| AC-05 | LearnConfig model init seeds | PASS | Fields added, wired in service, T-FR-CONFIG-01 updated |
| AC-06 | Classifier seed constructor | PASS | new_with_baseline_seed() added, T-SEED-01 |
| AC-07 | Scorer seed constructor | PASS | new_with_baseline_seed() added, T-SEED-02 |
| AC-08 | Persistence backward compat | PASS | #[serde(default)], T-COMPAT-01, T-PER-01 |
| AC-09 | Test suite passes | PASS | All workspace tests pass |
| AC-10 | Duplicate tests removed | PASS | 13 tests removed (9 reg + 4 reservoir) |
| AC-11 | Different seeds different results | PASS | T-SEED-03, T-SEED-04 |
| AC-12 | No public API changes | PASS | No changes to server/observe/engine |

## Clippy
- unimatrix-learn: 0 warnings
- unimatrix-adapt: 0 warnings (6 pre-existing fixed as bonus)
- unimatrix-store: 0 warnings (1 pre-existing fixed)
- unimatrix-vector: 0 warnings (1 pre-existing fixed)
- unimatrix-embed: 0 warnings (3 pre-existing fixed)
- unimatrix-observe: 48 pre-existing warnings (not in scope, not modified)
