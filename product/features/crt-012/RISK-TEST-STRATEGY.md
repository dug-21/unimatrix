# crt-012: Risk-Test Strategy

## Risk Registry

### R-01: Bincode Deserialization Failure with New AdaptConfig Fields (SR-03)

**Severity:** High | **Likelihood:** Medium | **Category:** Data Integrity

Adding `reservoir_seed` and `init_seed` to `AdaptConfig` which is embedded in `AdaptationState` and serialized via bincode. Bincode v2 with serde path has strict field matching. If `#[serde(default)]` is not correctly applied or bincode does not honor it, existing `adaptation.state` files will fail to deserialize, causing the adaptation pipeline to start from scratch.

**Mitigation:**
- Use `#[serde(default = "default_reservoir_seed")]` with explicit default functions.
- Add a regression test: serialize `AdaptConfig` without new fields, deserialize with them.
- The existing `load_state()` already handles deserialization failure gracefully (logs warning, returns None, renames to `.corrupt`). Worst case: cold restart, no data loss.

**Test:** T-COMPAT-01: Deserialize old-format AdaptConfig with new fields present.

### R-02: Import Path Breakage in Adapt-Side Code (SR-02)

**Severity:** Medium | **Likelihood:** Low | **Category:** Build

Changing `regularization.rs` from a full module to a re-export could break internal imports if any code uses `crate::regularization::EwcState` with glob imports or references internal items.

**Mitigation:**
- The re-export preserves `crate::regularization::EwcState` path.
- `persistence.rs` is the only internal consumer: `use crate::regularization::EwcState`. This continues to work.
- Compiler will catch any missed paths immediately.

**Test:** Compilation success is the test. No specific test needed.

### R-03: TrainingReservoir API Mismatch at Call Sites (SR-01)

**Severity:** Medium | **Likelihood:** Medium | **Category:** Build

The adapt-side `TrainingReservoir::add()` accepts `&[(u64, u64, u32)]` tuples. After unification, `TrainingReservoir<TrainingPair>::add()` accepts `&[TrainingPair]`. Call sites must convert.

**Mitigation:**
- Two call sites to update: `AdaptationService::record_training_pairs()` and `execute_training_step()` (indirectly, via the reservoir being pre-filled).
- `record_training_pairs()` currently passes raw tuples. After change: map tuples to `TrainingPair` before calling `add()`.
- The conversion is trivial: `TrainingPair { entry_id_a, entry_id_b, count }`.

**Test:** Existing tests T-SVC-03, T-SVC-06, T-SVC-07 cover `record_training_pairs` and training step execution. They will validate the conversion works.

### R-04: Model Init Seed Constructor Regression (SR-05)

**Severity:** High | **Likelihood:** Low | **Category:** Correctness

If `new_with_baseline_seed(42)` does not produce identical weights to current `new_with_baseline()`, all baseline tests (T-CS-01, T-CS-04) will fail and the production model behavior changes.

**Mitigation:**
- Implementation: Extract the body of `new_with_baseline()` into `new_with_baseline_seed()`, then `new_with_baseline()` calls `new_with_baseline_seed(42)`. This is a pure mechanical refactoring.
- The RNG sequence is deterministic for a given seed. Same seed = same weights.

**Test:** T-SEED-01: Verify `new_with_baseline()` and `new_with_baseline_seed(42)` produce identical `flat_parameters()`.

### R-05: Unused Dependency Activation Side Effects

**Severity:** Low | **Likelihood:** Low | **Category:** Build

Activating the `unimatrix-learn` dependency in `unimatrix-adapt` (currently declared but unused) could cause compile-time issues if the crate has feature flags or conditional compilation that interacts unexpectedly.

**Mitigation:**
- `unimatrix-learn` has no feature flags. No conditional compilation. Simple dependency.
- Verify `cargo check -p unimatrix-adapt` succeeds with the import added.

**Test:** Compilation success.

## Scope Risk Traceability

| Scope Risk | Architecture Decision | Implementation Risk | Test Coverage |
|-----------|----------------------|--------------------|----|
| SR-01 (API mismatch) | ADR-001: Re-export + type alias | R-03 | Existing T-SVC-03, T-SVC-06, T-SVC-07 |
| SR-02 (test coupling) | ADR-001: Re-export preserves paths | R-02 | Compilation |
| SR-03 (serde compat) | ADR-002: `#[serde(default)]` | R-01 | T-COMPAT-01 |
| SR-04 (re-export leakage) | ADR-001: Thin re-exports | None (non-issue) | N/A |
| SR-05 (baseline constructors) | ADR-002: Seed constructor pattern | R-04 | T-SEED-01 |

## Test Strategy

### New Tests Required

| Test ID | Description | Validates |
|---------|-------------|-----------|
| T-SEED-01 | `SignalClassifier::new_with_baseline()` produces same params as `new_with_baseline_seed(42)` | R-04, AC-06 |
| T-SEED-02 | `ConventionScorer::new_with_baseline()` produces same params as `new_with_baseline_seed(123)` | R-04, AC-07 |
| T-SEED-03 | Different seeds produce different weights (classifier) | AC-11 |
| T-SEED-04 | Different seeds produce different weights (scorer) | AC-11 |
| T-COMPAT-01 | Bincode round-trip with old AdaptConfig format (without new fields) | R-01, AC-08 |

### Existing Tests Providing Coverage

| Test ID(s) | Coverage |
|------------|----------|
| T-CS-01 (baseline zero-digest noise) | Classifier baseline determinism (AC-06) |
| T-CS-04 (scorer baseline low score) | Scorer baseline determinism (AC-07) |
| T-SVC-03 (record pairs accumulation) | Reservoir API compatibility (AC-02) |
| T-SVC-07 (train step fires) | End-to-end training with unified types (AC-02) |
| T-SVC-10 (persistence roundtrip) | Persistence with new config fields (AC-08) |
| T-PER-01 (save/load roundtrip) | State persistence compatibility (AC-08) |
| T-FR-CONFIG-01 (default config values) | LearnConfig default verification (AC-05) |

### Tests Removed (Duplicates)

| Removed Test(s) | Canonical Coverage |
|-----------------|-------------------|
| T-REG-01 through T-REG-09 (adapt regularization.rs) | T-LC-04, T-LC-05 + ewc.rs tests in unimatrix-learn |
| T-TRN-01 through T-TRN-03, T-TRN-10 (adapt training.rs reservoir) | T-LC-01 through T-LC-03, reservoir_overflow_no_growth in unimatrix-learn |

## Risk Severity Summary

| Severity | Count | Items |
|----------|-------|-------|
| High | 2 | R-01 (bincode compat), R-04 (baseline regression) |
| Medium | 2 | R-02 (import paths), R-03 (API mismatch) |
| Low | 1 | R-05 (unused dep activation) |

**Top 3 risks by severity:**
1. R-01: Bincode deserialization with new fields (mitigated by `#[serde(default)]` + graceful fallback)
2. R-04: Model init seed constructor regression (mitigated by mechanical refactoring + T-SEED-01)
3. R-03: TrainingReservoir API mismatch at call sites (mitigated by trivial conversion, caught by compiler)
