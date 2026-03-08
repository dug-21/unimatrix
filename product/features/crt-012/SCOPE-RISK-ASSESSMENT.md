# crt-012: Scope Risk Assessment

## SR-01: API Surface Mismatch Between Generic and Concrete Reservoirs

**Severity:** Medium
**Likelihood:** Medium

The learn-side `TrainingReservoir<T: Clone>` uses `add(&[T])` and `sample_batch(usize) -> Vec<&T>`. The adapt-side concrete `TrainingReservoir` uses `add(&[(u64, u64, u32)])` which internally constructs `TrainingPair` structs before adding. The `add` method signature differs: the generic version accepts `&[T]` directly, while the adapt-side accepts raw tuples and converts.

**Impact:** The adapt-side `add` method performs a tuple-to-struct conversion that the generic version does not. Callers in `unimatrix-adapt/src/training.rs` (`execute_training_step`) and `unimatrix-adapt/src/service.rs` (`record_training_pairs`) pass `&[(u64, u64, u32)]` tuples. After unification, these call sites must either: (a) construct `TrainingPair` before calling `add`, or (b) the reservoir type becomes `TrainingReservoir<TrainingPair>` and callers convert tuples to `TrainingPair` at the call site.

**Mitigation:** The conversion is trivial (3 fields). Move tuple-to-struct conversion to call sites or add a convenience method on `TrainingPair`. Architect should specify the exact call-site pattern.

## SR-02: Test Coupling to Module Paths

**Severity:** Low
**Likelihood:** Medium

Tests in `unimatrix-adapt` that use `use crate::regularization::EwcState` or `use crate::training::TrainingReservoir` will need import path updates. Tests within the removed modules will need to move or be deleted (they test the same code now living in `unimatrix-learn`).

**Impact:** Test files that import directly from removed modules will fail to compile. Tests within `regularization.rs` (T-REG-01 through T-REG-09) duplicate tests already in `unimatrix-learn/src/ewc.rs`. Tests within `training.rs` for `TrainingReservoir` (T-TRN-01 through T-TRN-03, T-TRN-10) duplicate tests in `unimatrix-learn/src/reservoir.rs`.

**Mitigation:** Re-export types from `unimatrix-adapt` at the same module path (e.g., `pub use unimatrix_learn::ewc::EwcState;` in a thin `regularization.rs`). Duplicated unit tests can be removed since the canonical tests live in `unimatrix-learn`. Integration tests in `unimatrix-adapt` that exercise the combined pipeline remain unchanged.

## SR-03: AdaptConfig Serde Backward Compatibility

**Severity:** Medium
**Likelihood:** Low

Adding `reservoir_seed` and `init_seed` fields to `AdaptConfig` changes its serialized format. `AdaptConfig` derives `Serialize, Deserialize` and is persisted via bincode in `unimatrix-adapt/src/persistence.rs`. Existing persisted state files will not contain the new fields.

**Impact:** Loading old adaptation state files will fail deserialization if the new fields are not marked with `#[serde(default)]`. The persistence format uses bincode, which has strict field matching by default.

**Mitigation:** Use `#[serde(default)]` on new fields with default values matching current hardcoded constants (42 for both). Verify bincode compatibility in tests. The persistence module's `AdaptState` struct may also need updating if it embeds `AdaptConfig`.

## SR-04: Re-export Leakage and Public API Surface

**Severity:** Low
**Likelihood:** Low

Making `unimatrix-adapt` re-export types from `unimatrix-learn` means downstream crates (like `unimatrix-server`) could theoretically access `unimatrix-learn` types through `unimatrix-adapt`. This is not a functional problem but muddies the API surface.

**Impact:** Minimal. The server already depends on both crates directly. No new transitive exposure.

**Mitigation:** Use thin re-export modules (`pub use`) to maintain the existing import paths. Document in `lib.rs` that these types originate from `unimatrix-learn`.

## SR-05: Model Init Seed Configurability Breaks Deterministic Baselines

**Severity:** Medium
**Likelihood:** Low

Making `SignalClassifier::new_with_baseline()` and `ConventionScorer::new_with_baseline()` accept a seed parameter changes their function signatures. All call sites (in `service.rs`, tests, and potentially `unimatrix-observe`) must be updated.

**Impact:** If the `new_with_baseline()` signature changes, all callers break. The function is used in `TrainingService::new()` (service.rs), `TrainingService::try_train_step()` closure, and numerous tests. The baseline weights are deterministic and relied upon by tests like T-CS-01 (baseline zero-digest noise classification).

**Mitigation:** Add `new_with_baseline_seed(seed: u64)` alongside the existing `new_with_baseline()` (which calls the new one with the default seed). Alternatively, keep `new_with_baseline()` as-is and add a parallel constructor. The architect should decide which pattern.

## Top 3 Risks for Architect Attention

1. **SR-01 (API mismatch):** The adapt-side reservoir wraps tuple-to-struct conversion in `add()`. The architect must specify how call sites adapt to the generic `TrainingReservoir<TrainingPair>`.

2. **SR-03 (serde compat):** Bincode deserialization of `AdaptConfig` with new fields. Must verify `#[serde(default)]` works with bincode v2's strict mode.

3. **SR-05 (baseline constructors):** Model init seed configurability affects a widely-called constructor. Architect should specify the constructor evolution pattern.
