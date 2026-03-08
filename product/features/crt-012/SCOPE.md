# crt-012: Neural Pipeline Cleanup

## Problem Statement

The neural and adaptive pipelines contain two instances of structural duplication and one hardcoded constant that limits training reproducibility control.

### Duplication 1: TrainingReservoir exists in two crates

`TrainingReservoir` is implemented independently in both `unimatrix-learn` and `unimatrix-adapt`:

- **`crates/unimatrix-learn/src/reservoir.rs`** — Generic `TrainingReservoir<T: Clone>` with `add(&[T])`, `sample_batch(usize) -> Vec<&T>`, `len()`, `is_empty()`, `total_seen()`. Used by `TrainingService` in `service.rs` for `TrainingSample` buffering.

- **`crates/unimatrix-adapt/src/training.rs`** — Concrete `TrainingReservoir` (not generic) specialized for `TrainingPair`. Same reservoir sampling algorithm, same `StdRng` seeded RNG, same `add`/`sample_batch`/`len`/`total_seen` API. Used by `AdaptationService` in `service.rs` for co-access pair buffering.

Both implementations use identical reservoir sampling logic: fill-until-capacity, then replace with probability `capacity / total_seen`. Both use `rand::rngs::StdRng` seeded via `seed_from_u64`. The adapt-side version is a specialization of the learn-side generic.

### Duplication 2: EwcState exists in two crates

`EwcState` (EWC++ online regularization) is implemented identically in both:

- **`crates/unimatrix-learn/src/ewc.rs`** — Full EWC++ state with `update()` (from grad matrices), `update_from_flat()` (from flat vectors), `penalty()`, `gradient_contribution()`, `to_vecs()`/`from_vecs()` serialization.

- **`crates/unimatrix-adapt/src/regularization.rs`** — Identical implementation, same module-level docs, same struct fields (`fisher`, `reference_params`, `alpha`, `lambda`, `initialized`), same methods with identical signatures and logic.

Both compute the running exponential average: `F_new = alpha * F_old + (1 - alpha) * F_batch` for Fisher diagonal, and the same formula for reference parameters. The duplication is exact copy-paste.

### Hardcoded RNG Seed

The RNG seed `42` is hardcoded in multiple locations:

1. **`unimatrix-learn/src/config.rs:59`** — `reservoir_seed: 42` as the `LearnConfig` default.
2. **`unimatrix-adapt/src/service.rs:54`** — `TrainingReservoir::new(config.reservoir_capacity, 42)` inline constant, not from config.
3. **`unimatrix-adapt/src/lora.rs:41`** — `MicroLoRA::new()` calls `Self::with_seed(config, 42)`.
4. **`unimatrix-learn/src/models/classifier.rs:72`** — `StdRng::seed_from_u64(42)` for Xavier init.
5. **`unimatrix-learn/src/models/scorer.rs:27`** — `StdRng::seed_from_u64(123)` for Xavier init.

The `LearnConfig` already has a `reservoir_seed` field, but `AdaptConfig` does not. The adapt-side hardcodes `42` in the service constructor rather than reading from config. MicroLoRA has a `with_seed()` constructor but the primary `new()` always uses `42`.

This means every training epoch samples identically (same RNG sequence), reducing sampling diversity across restarts and potentially biasing which training examples the models see.

## Goals

1. **Extract shared `RunningAverage` trait or struct.** Identify the common running-average state management pattern used by both EwcState implementations. Consolidate into a single implementation. The consolidated EwcState lives in one crate and is imported by the other.

2. **Unify TrainingReservoir.** The generic `TrainingReservoir<T: Clone>` in `unimatrix-learn` already subsumes the concrete version in `unimatrix-adapt`. Make `unimatrix-adapt` import from `unimatrix-learn` (or extract to a shared crate if the dependency direction is wrong).

3. **Make RNG seed configurable in AdaptConfig.** Add a `reservoir_seed: u64` field to `AdaptConfig` (matching `LearnConfig`'s existing field). Wire it through `AdaptationService::new()` to replace the hardcoded `42`.

4. **Add `init_seed` to AdaptConfig for MicroLoRA.** Make the LoRA initialization seed configurable instead of hardcoded `42` in `MicroLoRA::new()`.

5. **Preserve all existing tests.** All 1025+ unit tests and 174+ integration tests must pass. Refactoring must not change observable behavior (same seed defaults mean same deterministic sequences).

## Non-Goals

- **No new functionality.** This is pure structural cleanup. No new ML algorithms, no new training strategies, no new MCP tools.
- **No changing default seed values.** Defaults remain `42` (and `123` for scorer Xavier init) to preserve deterministic test behavior. The goal is configurability, not different defaults.
- **No new crate.** Prefer dependency between existing crates over creating a `unimatrix-common` or `unimatrix-ml-core` crate. If dependency direction allows, `unimatrix-adapt` depends on `unimatrix-learn` for shared types. If not, evaluate alternatives.
- **No changes to the neural extraction pipeline.** SignalClassifier and ConventionScorer initialization seeds (42/123) become configurable but defaults stay the same.
- **No persistence format changes.** EwcState serialization (`to_vecs`/`from_vecs`) and AdaptConfig serde format remain backward-compatible. New config fields use `#[serde(default)]` for deserialization of old data.
- **No changes to MCP tool interfaces.** This is internal refactoring only.

## Scope Boundaries

### Crates Modified

| Crate | Changes |
|-------|---------|
| `unimatrix-learn` | Canonical home for `TrainingReservoir<T>` and `EwcState`. No structural changes, just becomes the "owner". |
| `unimatrix-adapt` | Remove duplicated `TrainingReservoir` and `EwcState`. Import from `unimatrix-learn`. Add `reservoir_seed` and `init_seed` to `AdaptConfig`. Wire seed through service/lora constructors. |

### Dependency Direction

`unimatrix-adapt` already depends on `unimatrix-learn` in Cargo.toml (declared but unused). No new dependency needed. `unimatrix-learn` has no workspace dependencies, so no circular dependency risk. The refactoring activates an existing declared dependency.

### Files Affected

**unimatrix-learn:**
- `src/reservoir.rs` — No changes (already generic, already canonical).
- `src/ewc.rs` — No changes (already complete implementation).
- `src/lib.rs` — Ensure `pub mod reservoir` and `pub mod ewc` are publicly exported.

**unimatrix-adapt:**
- `src/regularization.rs` — Remove entire module, replace with `pub use unimatrix_learn::ewc::EwcState;`.
- `src/training.rs` — Remove `TrainingReservoir` struct, replace with `use unimatrix_learn::reservoir::TrainingReservoir;`. Adapt `TrainingPair` to work with generic reservoir.
- `src/config.rs` — Add `reservoir_seed: u64` and `init_seed: u64` fields with `#[serde(default)]`.
- `src/service.rs` — Wire `config.reservoir_seed` into `TrainingReservoir::new()`. Wire `config.init_seed` into `MicroLoRA::with_seed()`.
- `src/lora.rs` — Optionally keep `with_seed()` as-is; the change is at the call site.
- `src/lib.rs` — Update module declarations.
- `Cargo.toml` — Add `unimatrix-learn` dependency.

### Test Impact

- All existing tests in both crates must pass without modification (same defaults).
- Tests in `unimatrix-adapt` that directly construct `EwcState` or `TrainingReservoir` will use the imported types transparently (same API).
- No new test files needed; existing tests cover the shared code.

## Resolved Questions

1. **Dependency direction: CONFIRMED.** `unimatrix-adapt/Cargo.toml` already declares `unimatrix-learn = { path = "../unimatrix-learn" }` as a dependency (line 16). No circular dependency exists. `unimatrix-learn` has zero workspace dependencies (only external: ndarray, rand, serde, bincode, tracing). The dependency exists but is currently unused (no `use unimatrix_learn::` in adapt source). This is the ideal refactoring path.

2. **Model init seeds: YES, include in scope.** Per human decision: classifier seed (42) and scorer seed (123) will become configurable via `LearnConfig` fields `classifier_init_seed` and `scorer_init_seed`, with current values as defaults. This is consistent with the RNG configurability goal.

3. **Shared types stay in `unimatrix-learn`.** Per human decision: ML primitives do not belong in `unimatrix-core` (which holds domain traits: entry types, search results). `unimatrix-learn` is already positioned as "shared ML infrastructure" per its crate description.

## Related Issues

- #113: TrainingReservoir/EwcState deduplication
- #51: Reservoir RNG seed hardcoded
- #144: Parent issue for this feature
