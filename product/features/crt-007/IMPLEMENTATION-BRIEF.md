# Implementation Brief: crt-007 Neural Extraction Pipeline

## Executive Summary

crt-007 introduces the neural layer of Unimatrix's self-learning pipeline. It extracts shared training infrastructure from unimatrix-adapt into a new unimatrix-learn crate, implements two purpose-built neural models (Signal Classifier and Convention Scorer) via ruv-fann, and wires them into the col-013 extraction pipeline with shadow mode validation. ~800 lines across 6 implementation waves.

## Implementation Waves

### Wave 1: unimatrix-learn Crate Scaffolding
**Scope**: Create crate, extract shared primitives from unimatrix-adapt
**Files**:
- NEW: `crates/unimatrix-learn/Cargo.toml`
- NEW: `crates/unimatrix-learn/src/lib.rs`
- NEW: `crates/unimatrix-learn/src/reservoir.rs` (from adapt/training.rs: TrainingReservoir<T>)
- NEW: `crates/unimatrix-learn/src/ewc.rs` (from adapt/regularization.rs: EwcState generalized)
- NEW: `crates/unimatrix-learn/src/persistence.rs` (from adapt/persistence.rs: atomic save/load helpers)
- NEW: `crates/unimatrix-learn/src/config.rs` (NeuralConfig)
- EDIT: `Cargo.toml` (workspace member)
- EDIT: `crates/unimatrix-adapt/Cargo.toml` (add unimatrix-learn dep)
- EDIT: `crates/unimatrix-adapt/src/training.rs` (use learn::reservoir::TrainingReservoir)
- EDIT: `crates/unimatrix-adapt/src/regularization.rs` (use learn::ewc::EwcState)
- EDIT: `crates/unimatrix-adapt/src/persistence.rs` (use learn::persistence helpers)
- EDIT: `crates/unimatrix-adapt/src/lib.rs` (re-export from learn)

**Estimated lines**: ~250 (moved + ~50 new generic interfaces)
**Gate**: `cargo test --workspace` passes. All 174+ adapt tests green.
**Risk gate**: R-02 (adapt refactoring regression)

### Wave 2: NeuralModel Trait + ruv-fann Validation
**Scope**: Define model trait, validate ruv-fann can construct and run crt-007 topologies
**Files**:
- NEW: `crates/unimatrix-learn/src/model.rs` (NeuralModel trait)
- NEW: `crates/unimatrix-learn/src/digest.rs` (SignalDigest, SIGNAL_SLOTS, normalization functions)

**Estimated lines**: ~80
**Gate**: ruv-fann constructs both MLP topologies, forward pass produces finite output
**Risk gate**: R-01 (ruv-fann validation). If this gate fails, implement Wave 2b (ndarray fallback) before proceeding.

### Wave 3: Signal Classifier + Convention Scorer
**Scope**: Implement both models with baseline weights
**Files**:
- NEW: `crates/unimatrix-learn/src/classifier.rs` (SignalClassifier implementing NeuralModel)
- NEW: `crates/unimatrix-learn/src/scorer.rs` (ConventionScorer implementing NeuralModel)

**Estimated lines**: ~200
**Gate**: Classifier produces 5-class distribution; scorer produces [0,1] score. Baseline bias verified (strong signal != noise, weak signal == noise).
**Risk gate**: R-07 (conservative bias calibration)

### Wave 4: ModelRegistry
**Scope**: Model versioning with production/shadow/previous slots
**Files**:
- NEW: `crates/unimatrix-learn/src/registry.rs` (ModelRegistry, ModelSlot, LoadedModel, RollingMetrics)

**Estimated lines**: ~150
**Gate**: Promotion, rollback, and retention policy work correctly. Registry persists as JSON.
**Risk gate**: R-06 (corruption resilience)

### Wave 5: Shadow Mode + SQLite Evaluation
**Scope**: ShadowEvaluator, schema migration, evaluation logging
**Files**:
- NEW: `crates/unimatrix-learn/src/shadow.rs` (ShadowEvaluator)
- EDIT: `crates/unimatrix-server/src/migrations.rs` (v7->v8 shadow_evaluations table)

**Estimated lines**: ~120
**Gate**: Shadow evaluations persist to SQLite. Accuracy queries return correct values. Per-class regression detectable.
**Risk gate**: R-04 (shadow accuracy), R-08 (schema migration)

### Wave 6: Pipeline Integration + trust_source
**Scope**: Wire neural models into extraction_tick, add trust_source "neural"
**Files**:
- EDIT: `crates/unimatrix-engine/src/confidence.rs` (trust_score: "neural" -> 0.40)
- EDIT: `crates/unimatrix-engine/Cargo.toml` (add unimatrix-learn dep)
- EDIT: `crates/unimatrix-server/src/background.rs` (neural enhancement in extraction_tick)
- EDIT: `crates/unimatrix-server/Cargo.toml` (add unimatrix-learn dep)

**Estimated lines**: ~100
**Gate**: End-to-end integration test: synthetic observations -> rules -> digest -> neural prediction -> shadow log. No entries stored (shadow mode). trust_source "neural" in confidence scoring.
**Risk gate**: R-05 (integration surface), R-09 (performance)

## Dependencies

| Dependency | Status | Required By |
|------------|--------|-------------|
| col-013 (extraction pipeline) | In progress (feature/col-013 branch) | Wave 6 |
| crt-006 (unimatrix-adapt) | Complete | Wave 1 |
| ruv-fann crate (crates.io) | v0.2.0 available | Wave 2 |

## Key Technical Decisions Pre-Resolved

1. **ruv-fann first, ndarray fallback** -- Wave 2 is the gate
2. **Models in unimatrix-learn** -- the ML crate owns all models
3. **Shadow evaluations in SQLite** -- JOINable, queryable
4. **Direct bias for cold start** -- no training data needed
5. **[f32; 32] SignalDigest** -- fixed-width, append-only slots
6. **Schema v7->v8** -- new shadow_evaluations table only

## Test Strategy Summary

- **38 risk-driven tests** across 9 risk categories
- **P0 gates**: adapt refactoring (Wave 1), ruv-fann validation (Wave 2)
- **Integration seam**: shadow_evaluations SQLite table (records both rule and neural predictions)
- **Performance**: classifier <50ms, scorer <10ms, total neural overhead <1s per tick

## Estimated Scope

| Wave | New Lines | Modified Lines | New Files | Modified Files |
|------|-----------|---------------|-----------|----------------|
| 1 | ~200 | ~50 | 5 | 5 |
| 2 | ~80 | 0 | 2 | 0 |
| 3 | ~200 | 0 | 2 | 0 |
| 4 | ~150 | 0 | 1 | 0 |
| 5 | ~120 | ~10 | 1 | 1 |
| 6 | ~50 | ~50 | 0 | 4 |
| **Total** | **~800** | **~110** | **11** | **10** |
