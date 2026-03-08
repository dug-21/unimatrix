# crt-012: Architecture — Neural Pipeline Cleanup

## Overview

Pure refactoring: consolidate duplicated ML primitives (`EwcState`, `TrainingReservoir`) into their canonical home in `unimatrix-learn`, and make all RNG seeds configurable via config structs. No new functionality, no new crates, no persistence format changes.

## Crate Dependency Context

```
unimatrix-learn (no workspace deps)
    ^
    | (already declared in Cargo.toml, currently unused)
    |
unimatrix-adapt
    ^
    |
unimatrix-server (depends on both directly)
```

`unimatrix-learn` is a leaf crate with zero workspace dependencies. `unimatrix-adapt` already declares it as a dependency. This refactoring activates that unused dependency.

## Architecture Decision 1: Re-export Pattern for Backward Compatibility

**Decision:** Thin re-export modules in `unimatrix-adapt` preserve existing import paths.

**Context:** Removing `unimatrix-adapt/src/regularization.rs` entirely would break any code doing `use unimatrix_adapt::regularization::EwcState` (including internal tests and `persistence.rs` which does `use crate::regularization::EwcState`).

**Choice:** Replace `regularization.rs` contents with a single re-export:
```rust
pub use unimatrix_learn::ewc::EwcState;
```

Tests that were inside `regularization.rs` (T-REG-01 through T-REG-09) are removed because identical tests already exist in `unimatrix-learn/src/ewc.rs`. The `crate::regularization::EwcState` import path continues to work.

**Alternatives considered:**
- Remove `regularization.rs` entirely and update all imports: Higher risk, more churn, no benefit.
- Re-export at `lib.rs` level only: Breaks `use crate::regularization::EwcState` in internal modules.

## Architecture Decision 2: TrainingReservoir Unification via Type Alias

**Decision:** The adapt-side `TrainingReservoir` struct is replaced with a type alias to the generic learn-side version.

**Context:** The adapt-side `TrainingReservoir` in `training.rs` is not generic -- it is hardcoded to `TrainingPair`. The learn-side version is `TrainingReservoir<T: Clone>`. The adapt-side `add()` method accepts `&[(u64, u64, u32)]` tuples and converts them to `TrainingPair` internally. The learn-side `add()` accepts `&[T]` directly.

**Choice:**
1. Remove the adapt-side `TrainingReservoir` struct from `training.rs`.
2. Import the learn-side generic: `use unimatrix_learn::reservoir::TrainingReservoir;`
3. The reservoir type in adapt becomes `TrainingReservoir<TrainingPair>`.
4. Move tuple-to-`TrainingPair` conversion to call sites (`AdaptationService::record_training_pairs` and test code). This is 3 lines of mapping.

**Rationale:** The conversion belongs at the API boundary (where raw tuples arrive), not inside the generic data structure. This keeps the reservoir truly generic.

## Architecture Decision 3: Seed Configurability Strategy

**Decision:** All RNG seeds become config fields with defaults matching current hardcoded values.

### AdaptConfig Changes

| Field | Type | Default | Replaces |
|-------|------|---------|----------|
| `reservoir_seed` | `u64` | `42` | Hardcoded `42` in `AdaptationService::new()` |
| `init_seed` | `u64` | `42` | Hardcoded `42` in `MicroLoRA::new()` |

Both fields get `#[serde(default = "...")]` for bincode backward compatibility.

### LearnConfig Changes

| Field | Type | Default | Replaces |
|-------|------|---------|----------|
| `classifier_init_seed` | `u64` | `42` | Hardcoded `42` in `SignalClassifier::new_with_baseline()` |
| `scorer_init_seed` | `u64` | `123` | Hardcoded `123` in `ConventionScorer::new_with_baseline()` |

### Constructor Evolution

**Pattern:** Add `_with_seed(seed)` variant; existing no-arg constructor calls it with default.

- `SignalClassifier::new_with_baseline()` unchanged (calls `new_with_baseline_seed(42)` internally).
- `SignalClassifier::new_with_baseline_seed(seed: u64)` new public constructor.
- Same pattern for `ConventionScorer`.
- `MicroLoRA::new()` already has `with_seed()` -- no change to the type, only the call site in `AdaptationService::new()`.

**Call site changes in `TrainingService::new()`:**
```rust
// Before:
SignalClassifier::new_with_baseline()
ConventionScorer::new_with_baseline()

// After:
SignalClassifier::new_with_baseline_seed(config.classifier_init_seed)
ConventionScorer::new_with_baseline_seed(config.scorer_init_seed)
```

Also in `TrainingService::try_train_step()` closure where models are reconstructed.

## Architecture Decision 4: Persistence Backward Compatibility

**Decision:** New `AdaptConfig` fields use `#[serde(default)]` with explicit default functions. `AdaptationState` version remains 1 (no version bump needed).

**Context:** `AdaptConfig` is embedded in `AdaptationState` which is persisted via bincode. Bincode v2 with serde path supports `#[serde(default)]` for missing fields during deserialization.

**Rationale:** The new fields (`reservoir_seed`, `init_seed`) are metadata that do not affect the persisted adaptation weights. Old state files are still valid -- the defaults match the previously hardcoded values, so behavior is identical. No version bump is needed because the state's semantic meaning is unchanged.

## Architecture Decision 5: No EwcState Trait Extraction

**Decision:** Do not extract a `RunningAverage` trait from `EwcState`.

**Context:** SCOPE.md Goal 1 mentioned potentially extracting a shared running-average trait. After code analysis, `EwcState` is the only consumer of this pattern in the codebase. The running-average logic is 4 lines within `update()` and `update_from_flat()`. Extracting a trait for a single implementor adds abstraction without value.

**Choice:** Simply deduplicate by re-exporting. No new traits.

## File Change Summary

### unimatrix-learn (owner crate -- minimal changes)

| File | Change |
|------|--------|
| `src/lib.rs` | Already exports `EwcState` and `TrainingReservoir` publicly. No change. |
| `src/config.rs` | Add `classifier_init_seed: u64` (default 42), `scorer_init_seed: u64` (default 123). |
| `src/models/classifier.rs` | Add `new_with_baseline_seed(seed: u64)`. Existing `new_with_baseline()` delegates to it. |
| `src/models/scorer.rs` | Add `new_with_baseline_seed(seed: u64)`. Existing `new_with_baseline()` delegates to it. |
| `src/service.rs` | Wire `config.classifier_init_seed` / `config.scorer_init_seed` into model constructors. |

### unimatrix-adapt (consumer crate -- structural changes)

| File | Change |
|------|--------|
| `src/regularization.rs` | Replace body with `pub use unimatrix_learn::ewc::EwcState;` (remove duplicated tests). |
| `src/training.rs` | Remove `TrainingReservoir` struct. Import from `unimatrix_learn`. Callers convert tuples to `TrainingPair` before `add()`. |
| `src/config.rs` | Add `reservoir_seed: u64` (default 42), `init_seed: u64` (default 42) with `#[serde(default)]`. |
| `src/service.rs` | Wire `config.reservoir_seed` to `TrainingReservoir::new()`. Wire `config.init_seed` to `MicroLoRA::with_seed()`. |
| `src/lib.rs` | No structural change (module declarations unchanged). |

### No changes to

- `unimatrix-core`, `unimatrix-store`, `unimatrix-vector`, `unimatrix-embed`
- `unimatrix-engine`, `unimatrix-observe`, `unimatrix-server`
- Any MCP tool interfaces
- Any persistence formats (state version remains 1)

## Integration Surface

This refactoring has zero integration surface changes. All public APIs retain their signatures. All config defaults match current behavior. No downstream crates are affected.
