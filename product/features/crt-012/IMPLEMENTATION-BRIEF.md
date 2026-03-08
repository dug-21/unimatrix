# crt-012: Implementation Brief

## Summary

Deduplicate EwcState and TrainingReservoir across unimatrix-learn and unimatrix-adapt. Make all RNG seeds configurable. Pure refactoring with zero public API changes.

## Implementation Order

### Step 1: LearnConfig seed fields + model constructors (unimatrix-learn)

1. Add `classifier_init_seed: u64` (default 42) and `scorer_init_seed: u64` (default 123) to `LearnConfig` in `src/config.rs`.

2. In `src/models/classifier.rs`:
   - Rename `new_with_baseline()` body to `new_with_baseline_seed(seed: u64)`.
   - Make `new_with_baseline()` call `Self::new_with_baseline_seed(42)`.

3. In `src/models/scorer.rs`:
   - Rename `new_with_baseline()` body to `new_with_baseline_seed(seed: u64)`.
   - Make `new_with_baseline()` call `Self::new_with_baseline_seed(123)`.

4. In `src/service.rs`:
   - Update `TrainingService::new()` to use `SignalClassifier::new_with_baseline_seed(config.classifier_init_seed)` and `ConventionScorer::new_with_baseline_seed(config.scorer_init_seed)`.
   - Update the `try_train_step()` closure similarly.

5. Add tests T-SEED-01 through T-SEED-04 in the respective model test modules.

6. Run `cargo test -p unimatrix-learn`. All existing tests must pass.

### Step 2: AdaptConfig seed fields (unimatrix-adapt)

1. In `src/config.rs`, add:
   ```rust
   #[serde(default = "default_reservoir_seed")]
   pub reservoir_seed: u64,
   #[serde(default = "default_init_seed")]
   pub init_seed: u64,
   ```
   With `fn default_reservoir_seed() -> u64 { 42 }` and `fn default_init_seed() -> u64 { 42 }`.
   Update `Default` impl.

2. Add T-COMPAT-01 in `src/config.rs` tests or `src/persistence.rs` tests.

3. Run `cargo test -p unimatrix-adapt`. All existing tests must pass.

### Step 3: EwcState deduplication (unimatrix-adapt)

1. Replace `src/regularization.rs` contents with:
   ```rust
   //! EWC++ regularization re-exported from unimatrix-learn.
   pub use unimatrix_learn::ewc::EwcState;
   ```

2. Remove all tests from the old `regularization.rs` (canonical tests are in unimatrix-learn).

3. Run `cargo test -p unimatrix-adapt`. Verify `persistence.rs` and `service.rs` compile (they use `crate::regularization::EwcState`).

### Step 4: TrainingReservoir deduplication (unimatrix-adapt)

1. In `src/training.rs`:
   - Remove the `TrainingReservoir` struct, `use rand::...` imports for it.
   - Add `use unimatrix_learn::reservoir::TrainingReservoir;`.
   - The reservoir type becomes `TrainingReservoir<TrainingPair>`.
   - Update `TrainingReservoir::new()` calls (same signature, just different source).
   - For `add()`: callers previously passed `&[(u64, u64, u32)]`. Now they must pass `&[TrainingPair]`. Update `add()` call sites to construct `TrainingPair` first.
   - Remove duplicate reservoir tests (T-TRN-01 through T-TRN-03, T-TRN-10).

2. In `src/service.rs`:
   - Update `record_training_pairs()` to convert `&[(u64, u64, u32)]` tuples to `Vec<TrainingPair>` before calling `reservoir.add()`.
   - Wire `config.reservoir_seed` into `TrainingReservoir::new(config.reservoir_capacity, config.reservoir_seed)`.
   - Wire `config.init_seed` into `MicroLoRA::with_seed(lora_config, config.init_seed)`.

3. Run `cargo test -p unimatrix-adapt`. All remaining tests must pass.

### Step 5: Full workspace validation

1. `cargo test --workspace` -- all tests pass.
2. `cargo clippy --workspace` -- no new warnings.
3. `cargo check --workspace` -- clean compilation.

## Key Implementation Notes

- **TrainingPair stays in unimatrix-adapt.** It is an adapt-specific type (co-access pairs). Do not move it to unimatrix-learn.
- **The adapt-side `training.rs` still contains:** `TrainingPair`, `infonce_loss()`, `infonce_gradients()`, `execute_training_step()`, `add_ewc_gradient()`, `dot()`. Only `TrainingReservoir` is removed.
- **No changes to `unimatrix-adapt/Cargo.toml`.** The `unimatrix-learn` dependency is already declared.
- **Persistence module (`persistence.rs`)** uses `use crate::regularization::EwcState` which continues to work via re-export.

## ADRs (for Unimatrix storage when available)

| ADR | Decision |
|-----|----------|
| ADR-001 | Re-export pattern for type deduplication |
| ADR-002 | RNG seed configurability strategy |
| ADR-003 | No RunningAverage trait extraction |

## Related Issues

- Closes #113 (TrainingReservoir/EwcState dedup)
- Closes #51 (reservoir RNG seed hardcoded)
- Parent: #144
