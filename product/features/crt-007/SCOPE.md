# crt-007: Neural Extraction Pipeline

## Problem Statement

col-013 (Extraction Rule Engine) provides deterministic, rule-based knowledge extraction from observation data. These rules are effective for high-confidence patterns (knowledge gaps, implicit conventions, dead knowledge) but are inherently limited: they cannot classify ambiguous signals, score cross-feature pattern strength, or adapt to a project's evolving domain. A signal that looks like noise in one codebase may be a convention in another. Rules are static; the domain is not.

Additionally, the training infrastructure built in unimatrix-adapt (crt-006) for MicroLoRA embedding adaptation — reservoir sampling, EWC++ regularization, fire-and-forget training, versioned persistence — is currently coupled to the MicroLoRA use case. This infrastructure is generic and directly applicable to training extraction models, but cannot be consumed without refactoring.

crt-007 bridges both gaps: it extracts shared training primitives from unimatrix-adapt into a reusable `unimatrix-learn` module, then builds two purpose-built neural models (Signal Classifier and Convention Scorer) that enhance the extraction pipeline with learned, domain-adaptive classification. Shadow mode validation ensures models prove themselves before influencing the knowledge base.

## Goals

1. Extract shared training infrastructure (`TrainingReservoir`, `EwcState`, `ModelRegistry`, persistence helpers) from `unimatrix-adapt` into a new `unimatrix-learn` crate consumed by both `unimatrix-adapt` (MicroLoRA) and the new extraction models
2. Refactor `unimatrix-adapt` to consume `unimatrix-learn` primitives instead of its own copies (~200 lines moved, not rewritten; all existing adapt tests pass)
3. Implement a Signal Classifier MLP (~5MB) via burn that classifies signal digests into `convention | pattern | gap | dead | noise` categories
4. Implement a Convention Scorer MLP (~2MB) via burn that scores cross-feature pattern confidence (0.0-1.0)
5. Integrate neural models into the col-013 extraction pipeline as an enhancement layer (rules produce signal digests, models classify/score them)
6. Build shadow mode infrastructure: models run alongside rules, predictions logged but not stored, with precision/recall comparison against rule-only extraction
7. Implement model versioning (production/shadow/previous) with auto-rollback on accuracy regression (>5% drop)
8. Ship with hand-tuned baseline weights biased toward conservative extraction (classifier biases toward `noise`, scorer requires strong evidence)
9. Add `"neural"` trust_source value to crt-002 confidence scoring (~5 lines)

## Non-Goals

- **Duplicate Detector, Pattern Merger, Entry Writer Scorer** -- deferred to crt-009 (Advanced Models + Optional LLM)
- **Continuous self-retraining from utilization feedback** -- deferred to crt-008 (Continuous Self-Retraining); crt-007 ships with hand-tuned baseline weights only
- **LLM API integration** -- deferred to crt-009
- **Lesson extraction from failure traces** -- permanently agent-driven (requires multi-hop causal reasoning)
- **New MCP tools** (e.g., `context_review`) -- deferred to crt-009
- **GPU acceleration** -- CPU-only; models are small enough (<5s inference, <5s training)
- **Multi-repository model sharing** -- per-repo scope; models live in `~/.unimatrix/{project_hash}/models/`
- **Candle framework** -- burn is the selected framework; Candle evaluated but burn's `TrainStep` trait and autodiff better fit the self-learning roadmap
- **Daemon mode** -- models run within the session-scoped server process

## Background Research

### Existing Infrastructure (unimatrix-adapt, crt-006)

The `unimatrix-adapt` crate implements the full continuous learning pipeline for MicroLoRA embedding adaptation:

| Component | File | Lines | Extraction Target |
|-----------|------|-------|-------------------|
| `TrainingReservoir` | `training.rs` | ~80 | Generic reservoir sampling with configurable capacity |
| `EwcState` | `regularization.rs` | ~80 | EWC++ online Fisher regularization |
| `AdaptationState` (persistence) | `persistence.rs` | ~100 | Atomic save/load with version tracking |
| `AdaptConfig` | `config.rs` | ~40 | Configuration pattern |
| `AdaptationService` (orchestrator) | `service.rs` | ~200 | Fire-and-forget training dispatch (MicroLoRA-specific) |

The reservoir sampling, EWC++ regularization, and persistence helpers are generic. The `MicroLoRA`-specific code (lora.rs, prototypes.rs, episodic.rs, InfoNCE loss) stays in unimatrix-adapt. The service orchestrator stays but is refactored to use shared primitives.

### burn

Pure Rust deep learning framework providing:
- Declarative module system (`nn::Linear`, `nn::Relu`, etc.)
- Full training loop ownership via `TrainStep` trait — custom loss, gradient injection, optimizer control
- Autodiff backend — gradients flow through any tensor operation (including EWC penalty) natively
- Model save/load with versioning (`burn::record`)
- Multiple backends (NdArray for CPU, wgpu for GPU — CPU-only for Unimatrix)
- ONNX import/export
- Active ecosystem, well-maintained, MIT/Apache-2.0 licensed

**Why burn over ruv-fann**: ruv-fann's RPROP training is a black box — cannot inject EWC gradients (crt-008), cannot implement Siamese shared-weight architecture (crt-009). burn gives full training loop control while still providing topology-agnostic model definition. See Resolved Question #1.

### Signal Classifier Architecture

Input: structured signal features derived from extraction rule outputs:
- `search_miss_count`: number of zero-result searches for this pattern
- `co_access_density`: density of co-access relationships
- `consistency_score`: ratio of features showing this pattern
- `feature_count`: number of features observed
- `observation_count`: total observations matching this pattern
- `age_days`: age of the oldest observation
- Additional domain-specific features (TBD during architecture)

MLP topology: `input(N) -> hidden(64, sigmoid-symmetric) -> hidden(32) -> output(5, softmax)`
Output: probability distribution over `[convention, pattern, gap, dead, noise]`
Size: ~5MB

### Convention Scorer Architecture

Input features:
- `consistency_ratio`: how consistently the pattern appears across features
- `feature_count`: number of features exhibiting the pattern
- `deviation_count`: number of features deviating from the pattern
- `category_distribution`: spread of the pattern across entry categories
- `age_days`: pattern age

MLP topology: `input(M) -> hidden(32) -> output(1, sigmoid)`
Output: convention confidence 0.0-1.0
Size: ~2MB

### Shadow Mode Validation Protocol

1. **Features 1-5 (observation-only)**: rules extract, models observe but produce no output
2. **Features 6-N (shadow mode)**: models run on every extraction pipeline invocation, predictions logged alongside rule results, precision/recall computed against rule-only baseline
3. **Promotion**: shadow accuracy >= rule-only accuracy, minimum 20 evaluations, no regression per category
4. **Post-promotion monitoring**: rolling accuracy tracked, auto-rollback if >5% drop

### col-013 Integration Point

The extraction pipeline (col-013) produces `ProposedEntry` values from observation data. Neural models sit between rule evaluation and quality gates:

```
observations -> extraction rules -> signal digests -> neural classifier -> quality gates -> store
                                                    -> convention scorer -^
```

In shadow mode, the neural branch logs predictions but does not influence the quality gate decisions. After promotion, the classifier can reclassify rule outputs (e.g., a rule says "convention" but classifier says "noise" with high confidence -> suppress) and the convention scorer provides a learned confidence score that supplements the rule-based confidence.

### Constraints Discovered

- `unimatrix-adapt` uses `ndarray` for matrix operations; burn has its own tensor type — bridging at the `Vec<f32>` boundary
- The extraction pipeline (col-013) runs in the background maintenance tick; neural model inference must complete within the tick budget (~1 hour interval, but individual operations should be <100ms)
- Model files live in `~/.unimatrix/{project_hash}/models/` alongside existing `adaptation.state`
- The `TrainingReservoir` in unimatrix-adapt is currently typed to `TrainingPair` (co-access pairs); the shared version must be generic over the sample type
- `EwcState` currently takes `Array2<f32>` gradient matrices (MicroLoRA shape); the shared version needs to work with flat parameter vectors (burn models)
- Shadow mode requires persisting evaluation logs and metrics across sessions (simple SQLite table or flat file)

## Proposed Approach

### Phase 1: Shared Training Infrastructure (unimatrix-learn)

New crate `crates/unimatrix-learn/` containing:
- `TrainingReservoir<T>` -- generic reservoir sampling (extracted from `unimatrix-adapt/src/training.rs`)
- `EwcState` -- EWC++ regularization generalized for flat parameter vectors (extracted from `unimatrix-adapt/src/regularization.rs`)
- `ModelRegistry` -- production/shadow/previous model slot management with promotion/rollback
- `ModelVersion` -- version metadata (generation, timestamp, accuracy metrics)
- Persistence helpers (atomic save/load with version tracking, extracted from `unimatrix-adapt/src/persistence.rs`)

Refactor `unimatrix-adapt` to depend on `unimatrix-learn` and use shared primitives. All 174+ existing adapt tests must pass.

### Phase 2: Neural Models (burn Integration)

New module in `unimatrix-learn` (resolved question #2):
- `SignalClassifier` as a burn module (`nn::Linear` layers) with hand-tuned baseline weights
- `ConventionScorer` as a burn module (`nn::Linear` layers) with hand-tuned baseline weights
- `SignalDigest` struct: the structured feature vector fed to models
- Input normalization (feature scaling to [0,1] range)
- Cold-start weight initialization biased toward conservative classification

### Phase 3: Shadow Mode + Model Versioning

- `ShadowEvaluator` that runs both rule-only and neural predictions, logs comparison metrics
- `ModelRegistry` tracking production/shadow/previous slots per model
- Promotion logic (accuracy threshold, minimum evaluations, per-category regression check)
- Auto-rollback on accuracy drop >5%
- Persisted evaluation log (simple append-only format)

### Phase 4: Pipeline Integration

- Wire neural models into col-013's extraction pipeline
- In shadow mode: log predictions alongside rule outputs
- After promotion: neural predictions influence quality gate decisions
- Add `"neural"` trust_source value to crt-002 (~5 lines)
- Entries extracted via neural enhancement use `trust_source: "neural"` (vs `"auto"` for rule-only)

## Acceptance Criteria

- AC-01: `unimatrix-learn` crate exists with `TrainingReservoir<T>`, `EwcState`, `ModelRegistry`, and persistence helpers
- AC-02: `unimatrix-adapt` depends on `unimatrix-learn` and uses shared `TrainingReservoir<T>` and `EwcState` (no duplicated implementations)
- AC-03: All existing unimatrix-adapt tests pass after refactoring
- AC-04: Signal Classifier MLP constructed with hand-tuned baseline weights via burn
- AC-05: Convention Scorer MLP constructed with hand-tuned baseline weights via burn
- AC-06: `SignalDigest` struct defined with all input features for both models
- AC-07: Classifier inference produces probability distribution over 5 categories in <50ms
- AC-08: Scorer inference produces convention confidence in <10ms
- AC-09: Shadow mode runs both models on extraction pipeline invocations without affecting stored entries
- AC-10: Shadow evaluation logs persist predictions with ground truth for later accuracy computation
- AC-11: ModelRegistry manages production/shadow/previous slots per model with promotion criteria
- AC-12: Auto-rollback triggers when rolling accuracy drops >5% below pre-promotion baseline
- AC-13: Models stored in `~/.unimatrix/{project_hash}/models/{model_name}/` with versioned filenames
- AC-14: Cold-start baseline weights bias classifier toward `noise` class and scorer toward low scores
- AC-15: crt-002 confidence scoring includes `"neural" -> 0.40` trust_source weight
- AC-16: Neural-enhanced extraction entries use `trust_source: "neural"` to distinguish from rule-only `"auto"` entries
- AC-17: Unit tests for classifier inference, scorer inference, shadow evaluation, model registry promotion/rollback
- AC-18: Integration test demonstrating end-to-end shadow mode (rules produce digest -> models classify -> predictions logged -> no entries stored)

## Constraints

- **col-013 dependency**: Extraction pipeline and background maintenance tick must be complete and merged
- **crt-006 dependency**: `unimatrix-adapt` must exist to be refactored
- **No breaking changes**: `unimatrix-adapt` public API unchanged; only internal implementation moves to shared crate
- **Binary size**: burn (NdArray backend, CPU-only) adds to binary size; model files are runtime artifacts (~7MB total for both models)
- **CPU only**: No GPU dependencies. All inference and (future) training runs on CPU
- **Per-repo isolation**: Model state is project-scoped via `{project_hash}` directory
- **~800 lines total**: ~250 shared infrastructure extraction, ~350 neural models, ~200 shadow mode (per ASS-015 scoping)

## Resolved Questions

1. **ML framework**: **Resolved: burn (not ruv-fann or ndarray).** Originally scoped as "ruv-fann first, with exit ramps." Exit ramp exercised mid-implementation when ruv-fann's black-box RPROP training was found to be fundamentally incompatible with the self-learning roadmap: (a) cannot access loss gradients before weight update — prevents EWC gradient injection required by crt-008, (b) cannot modify training loop — prevents custom optimizer strategies, (c) opaque network API blocks Siamese MLP shared-weight architecture required by crt-009 Duplicate Detector. **burn solves all three**: full training loop ownership via `TrainStep` trait, autodiff computes gradients through EWC penalty natively (no approximation), `nn::Linear` + shared module references support all crt-009 architectures. burn is pure Rust, active ecosystem, well-maintained, supports ONNX export. The topology-agnostic training value originally attributed to ruv-fann applies equally to burn — with the critical addition of training loop transparency. **ndarray remains** in unimatrix-learn for shared infra (reservoir, EWC state vectors) where full tensor/autodiff is unnecessary.

2. **Crate placement**: **Resolved: models live in `unimatrix-learn`.** Through the crt-008/009 lens, `unimatrix-learn` becomes "the ML crate" — shared training infra (reservoir, EWC++, model registry) plus all models (5+ across crt-007/008/009). The extraction pipeline in `unimatrix-engine` calls into `unimatrix-learn` for classification/scoring but doesn't own models. crt-009 adds models to `unimatrix-learn` without touching the extraction crate. Clean separation: domain logic (extraction rules) vs ML logic (model zoo + training).

3. **Shadow mode persistence**: **Resolved: SQLite.** Evaluation logs go into SQLite — JOINable with observations, sessions, signals. The platform will find more uses for queryable ML telemetry than we can predict today. Flexibility over simplicity.

4. **Baseline weight generation**: **Resolved: direct bias (option b).** Set weights to produce conservative output directly — classifier biases toward `noise` class, scorer biases toward low scores. No synthetic training data needed, no historical data dependency. Cold start is intentionally cautious.

5. **Feature vector stability**: **Resolved: fixed-width with reserved slots (32 floats).** A self-learning system that goes dumb on every schema change isn't self-learning. `SignalDigest` is defined as a fixed-width 32-slot `[f32; 32]` vector. crt-007 uses ~6-7 slots, remainder initialized to zero. New signals (crt-008/009 and beyond) fill empty slots additively — no model retraining required, no topology change, no cold restart. Model weights learned on earlier slots remain valid. Known roadmap needs ~15 features; 32 provides headroom for undiscovered signals without being so large that zeros dominate training dynamics. Power-of-2 aligns with SIMD/cache. The Duplicate Detector (crt-009) uses 768-dim embedding concatenation on a separate path — not counted against this budget. **Breaking change fallback** (removing/reordering features): ModelRegistry detects schema version mismatch, demotes old model to previous slot, cold-starts new model with conservative bias weights. Automatic, no restart, but temporarily loses learned accuracy until retraining catches up. This path should be rare.

## Open Questions

(All resolved — see above.)

## Scope Change Log

**2026-03-06 — Framework pivot (ruv-fann → burn):** Human identified ruv-fann's black-box RPROP as fundamentally incompatible with the self-learning roadmap during Stage 3b implementation. Exit ramp exercised as designed. All design artifacts beyond SCOPE.md removed; architecture, specification, pseudocode, and implementation to be re-derived from updated scope. 78% of implementation work (shared infra, digest, registry, shadow) is framework-agnostic and conceptually survives — only classifier/scorer/model-trait components change. Partial implementation stashed as `crt-007 partial implementation (pre-burn pivot)`.

## Tracking

GitHub Issue: #109
