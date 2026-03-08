# crt-012: Acceptance Map

## Acceptance Criteria to Implementation Mapping

| AC | Description | Implementation Step | Test Coverage | Risk |
|----|-------------|--------------------|----|------|
| AC-01 | EwcState deduplication | Step 3: Replace regularization.rs with re-export | Compilation + existing T-LC-04, T-LC-05 | R-02 |
| AC-02 | TrainingReservoir deduplication | Step 4: Remove struct, import from learn | T-SVC-03, T-SVC-07, T-TRN-14/15/16 | R-03 |
| AC-03 | AdaptConfig reservoir_seed | Step 2 + Step 4: Add field, wire in service | T-COMPAT-01, existing config tests | R-01 |
| AC-04 | AdaptConfig init_seed | Step 2 + Step 4: Add field, wire in service | T-COMPAT-01, existing config tests | R-01 |
| AC-05 | LearnConfig model init seeds | Step 1: Add fields, wire in service | T-FR-CONFIG-01 (updated) | None |
| AC-06 | Classifier seed constructor | Step 1: Add new_with_baseline_seed() | T-SEED-01, T-CS-01 | R-04 |
| AC-07 | Scorer seed constructor | Step 1: Add new_with_baseline_seed() | T-SEED-02, T-CS-04 | R-04 |
| AC-08 | Persistence backward compat | Step 2: #[serde(default)] on new fields | T-COMPAT-01, T-PER-01, T-SVC-10 | R-01 |
| AC-09 | Test suite passes | Step 5: Full workspace validation | cargo test --workspace | All |
| AC-10 | Duplicate tests removed | Steps 3-4: Remove from adapt | Canonical tests in learn | None |
| AC-11 | Different seeds = different results | Step 1: Seed constructors | T-SEED-03, T-SEED-04 | None |
| AC-12 | No public API changes | All steps: Verify no external changes | cargo check for server/observe/engine | None |

## New Test Summary

| Test ID | File | Validates |
|---------|------|-----------|
| T-SEED-01 | unimatrix-learn/src/models/classifier.rs | AC-06: baseline_seed(42) == baseline() |
| T-SEED-02 | unimatrix-learn/src/models/scorer.rs | AC-07: baseline_seed(123) == baseline() |
| T-SEED-03 | unimatrix-learn/src/models/classifier.rs | AC-11: seed(42) != seed(99) |
| T-SEED-04 | unimatrix-learn/src/models/scorer.rs | AC-11: seed(123) != seed(99) |
| T-COMPAT-01 | unimatrix-adapt/src/config.rs | AC-08: old format deserializes with defaults |

## Removed Tests

| Test ID(s) | File | Reason |
|------------|------|--------|
| T-REG-01 to T-REG-09 | unimatrix-adapt/src/regularization.rs | Duplicates of unimatrix-learn/src/ewc.rs tests |
| T-TRN-01 to T-TRN-03, T-TRN-10 | unimatrix-adapt/src/training.rs | Duplicates of unimatrix-learn/src/reservoir.rs tests |

## Exit Checklist

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace` clean
- [ ] No TODO/unimplemented!() in changed files
- [ ] AC-01 through AC-12 verified
- [ ] #113 and #51 closable
- [ ] No changes to unimatrix-server, unimatrix-observe, unimatrix-engine, unimatrix-core
