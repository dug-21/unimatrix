# Risk-Test Strategy: crt-007 Neural Extraction Pipeline

## Risk Registry

### R-01: ruv-fann RPROP Implementation Defects
**Source**: SR-01 (Scope Risk Assessment)
**Severity**: HIGH | **Likelihood**: MEDIUM | **Residual**: LOW
**Architectural Mitigation**: ADR-002 (NeuralModel trait abstracts ML framework)

**Description**: ruv-fann v0.2.0 has ~4K downloads. RPROP training, softmax output, or model serialization may have undiscovered bugs. Numerical instability (NaN/Inf in weights) could produce garbage predictions.

**Test Strategy**:
- T-R01-1: Validate ruv-fann MLP construction with crt-007 topologies (32->64->32->5 and 32->32->1)
- T-R01-2: Forward pass on zero input produces finite output with correct shape
- T-R01-3: Forward pass on random input produces valid probability distribution (sums to 1.0 for classifier, [0,1] for scorer)
- T-R01-4: Model save/load round-trip produces identical predictions
- T-R01-5: NaN/Inf detection in params_flat() after load

**Acceptance Gate**: All T-R01 tests pass before proceeding to pipeline integration. If any fail, trigger fallback to ndarray implementation.

---

### R-02: unimatrix-adapt Refactoring Regression
**Source**: SR-02 (Scope Risk Assessment)
**Severity**: HIGH | **Likelihood**: LOW | **Residual**: VERY LOW
**Architectural Mitigation**: ADR-001 (extract-then-redirect pattern)

**Description**: Extracting TrainingReservoir and EwcState from unimatrix-adapt to unimatrix-learn risks breaking MicroLoRA adaptation. Serde compatibility, import paths, and type bounds could cause subtle regressions.

**Test Strategy**:
- T-R02-1: All existing unimatrix-adapt tests pass after refactoring (174+ tests)
- T-R02-2: `AdaptationState` persistence round-trip: save with old code, load with new code (serde compatibility)
- T-R02-3: `TrainingReservoir<TrainingPair>` behaves identically to pre-refactoring `TrainingReservoir` (same sampling distribution)
- T-R02-4: `EwcState` penalty and gradient_contribution produce identical results with flat vector vs Array2 interfaces
- T-R02-5: Full workspace test suite (`cargo test --workspace`) passes

**Acceptance Gate**: Zero test failures in unimatrix-adapt after refactoring. Persistence compatibility verified.

---

### R-03: SignalDigest Feature Semantic Drift
**Source**: SR-03 (Scope Risk Assessment)
**Severity**: MEDIUM | **Likelihood**: LOW | **Residual**: LOW
**Architectural Mitigation**: ADR-003 (fixed-width, append-only, versioned)

**Description**: If slot semantics change (e.g., normalization formula modified), models trained on old semantics produce incorrect predictions. The fixed-width format mitigates this but doesn't prevent all semantic drift.

**Test Strategy**:
- T-R03-1: SignalDigest construction with known input values produces expected normalized output per slot
- T-R03-2: Reserved slots (7-31) are zero after construction
- T-R03-3: Schema version mismatch detected by ModelRegistry (mock model with version 2 vs digest version 1)
- T-R03-4: Normalization functions are deterministic: same raw value -> same normalized value across runs

**Acceptance Gate**: All normalization functions tested with boundary values (0, 1, max, overflow).

---

### R-04: Shadow Mode False Confidence
**Source**: SR-04 (Scope Risk Assessment)
**Severity**: MEDIUM | **Likelihood**: MEDIUM | **Residual**: MEDIUM
**Architectural Mitigation**: ADR-004 (SQLite evaluation logs with side-by-side predictions)

**Description**: Shadow mode uses rule-only extraction as ground truth. If rules have systematic errors, neural models that learn to replicate those errors will appear accurate. The "accuracy" metric is relative to an imperfect baseline.

**Test Strategy**:
- T-R04-1: ShadowEvaluator correctly records both rule and neural predictions side-by-side
- T-R04-2: Per-class accuracy computation returns correct values for known evaluation sets
- T-R04-3: Divergence rate (rule != neural) is tracked and queryable
- T-R04-4: Promotion does NOT occur if any per-class accuracy drops >10% (even if aggregate improves)

**Acceptance Gate**: Shadow evaluation records are queryable and JOINable. Per-class regression check tested with synthetic data showing aggregate improvement but single-class regression.

**Residual Risk**: This risk cannot be fully mitigated until crt-008 introduces utilization feedback as an independent quality signal. Accepted for crt-007.

---

### R-05: col-013 Integration Surface Instability
**Source**: SR-05 (Scope Risk Assessment)
**Severity**: MEDIUM | **Likelihood**: MEDIUM | **Residual**: LOW
**Architectural Mitigation**: ADR-003 (SignalDigest as boundary type)

**Description**: crt-007 depends on col-013's extraction pipeline exposing stable integration points. If col-013's ProposedEntry structure or extraction_tick() flow changes, the neural enhancement step needs rework.

**Test Strategy**:
- T-R05-1: SignalDigest can be constructed from a mock ProposedEntry without depending on col-013 internal types
- T-R05-2: Neural enhancement step is a standalone function callable with mock inputs (not tightly coupled to extraction_tick)
- T-R05-3: Integration test: synthetic observations -> extraction rules -> signal digest -> neural prediction -> shadow log

**Acceptance Gate**: Neural enhancement is injectable into the extraction pipeline via a trait or function pointer, not hardcoded into extraction_tick.

---

### R-06: Model File Corruption or Loss
**Source**: SR-06 (Scope Risk Assessment)
**Severity**: LOW | **Likelihood**: LOW | **Residual**: VERY LOW
**Architectural Mitigation**: ADR-005 (retention policy, ADR-006 baseline weights)

**Description**: Model files could be corrupted, deleted, or unreadable. Server must handle this gracefully.

**Test Strategy**:
- T-R06-1: ModelRegistry loads successfully with missing models directory (creates it)
- T-R06-2: Model load with corrupt file (truncated bytes) falls back to baseline weights
- T-R06-3: Model load with empty file falls back to baseline weights
- T-R06-4: registry.json corruption: falls back to fresh registry with baseline models
- T-R06-5: Retention policy cleanup deletes old version files after promotion

**Acceptance Gate**: All corruption scenarios result in functional (baseline) models, never a crash.

---

### R-07: Conservative Bias Deadlock
**Source**: SR-07 (Scope Risk Assessment)
**Severity**: LOW | **Likelihood**: MEDIUM | **Residual**: LOW
**Architectural Mitigation**: ADR-006 (configurable bias values)

**Description**: Baseline weights biased too heavily toward noise/low scores could prevent shadow mode from ever accumulating enough positive predictions for promotion. The system would be stuck in shadow mode indefinitely.

**Test Strategy**:
- T-R07-1: Classifier with baseline weights classifies a "strong convention signal" (consistency=1.0, feature_count=10, rule_confidence=0.9) as convention (not noise)
- T-R07-2: Scorer with baseline weights produces score > 0.5 for a strong signal
- T-R07-3: Classifier with baseline weights classifies a "weak signal" (low feature count, low consistency) as noise
- T-R07-4: Bias values are configurable via NeuralConfig (not hardcoded constants)

**Acceptance Gate**: Baseline models produce non-trivial (non-all-noise) predictions for strong signals while still biasing toward noise for weak signals.

---

### R-08: Schema Migration Failure
**Severity**: MEDIUM | **Likelihood**: LOW | **Residual**: VERY LOW

**Description**: v7->v8 schema migration (adding shadow_evaluations table) could fail on corrupted databases or concurrent access.

**Test Strategy**:
- T-R08-1: Migration from v7 database creates shadow_evaluations table with correct schema
- T-R08-2: Migration is idempotent (running twice doesn't error)
- T-R08-3: v8 database opens correctly with shadow_evaluations table functional
- T-R08-4: Existing v7 data is unaffected by migration

**Acceptance Gate**: Migration tested against a v7 database snapshot.

---

### R-09: Extraction Pipeline Performance Regression
**Severity**: MEDIUM | **Likelihood**: LOW | **Residual**: LOW

**Description**: Adding neural model inference to each extraction tick could slow the background tick enough to affect the next tick interval (15 minutes).

**Test Strategy**:
- T-R09-1: Classifier inference latency < 50ms (benchmarked)
- T-R09-2: Scorer inference latency < 10ms (benchmarked)
- T-R09-3: End-to-end neural enhancement for 100 ProposedEntries < 10 seconds total
- T-R09-4: Neural enhancement step does not hold locks that block MCP request handling

**Acceptance Gate**: Neural enhancement adds < 1 second overhead per typical extraction tick (10 entries).

---

## Scope Risk Traceability

| Scope Risk | Architecture Mitigation | Test Coverage | Residual |
|------------|------------------------|---------------|----------|
| SR-01 (ruv-fann maturity) | ADR-002 (trait abstraction) | T-R01-1..5 | LOW |
| SR-02 (adapt refactoring) | ADR-001 (extract-redirect) | T-R02-1..5 | VERY LOW |
| SR-03 (SignalDigest stability) | ADR-003 (fixed-width, versioned) | T-R03-1..4 | LOW |
| SR-04 (shadow accuracy) | ADR-004 (SQLite logs) | T-R04-1..4 | MEDIUM |
| SR-05 (col-013 integration) | ADR-003 (boundary type) | T-R05-1..3 | LOW |
| SR-06 (disk footprint) | ADR-005 (retention policy) | T-R06-1..5 | VERY LOW |
| SR-07 (conservative bias) | ADR-006 (configurable bias) | T-R07-1..4 | LOW |

## Test Summary

| Category | Test Count | Priority |
|----------|-----------|----------|
| ruv-fann validation (R-01) | 5 | P0 (gate: proceed or fallback) |
| Adapt refactoring (R-02) | 5 | P0 (gate: no regressions) |
| SignalDigest (R-03) | 4 | P1 |
| Shadow mode (R-04) | 4 | P1 |
| Integration surface (R-05) | 3 | P1 |
| Corruption resilience (R-06) | 5 | P2 |
| Bias calibration (R-07) | 4 | P1 |
| Schema migration (R-08) | 4 | P1 |
| Performance (R-09) | 4 | P2 |
| **Total** | **38** | |

## Risk-Ordered Implementation Sequence

1. **Phase 1**: unimatrix-learn crate + adapt refactoring (R-02 gate: all adapt tests pass)
2. **Phase 2**: ruv-fann integration + model construction (R-01 gate: forward pass works)
3. **Phase 3**: SignalDigest + normalization (R-03 tests)
4. **Phase 4**: Shadow mode + SQLite evaluation (R-04, R-08 tests)
5. **Phase 5**: Pipeline integration + trust_source (R-05, R-07, R-09 tests)
6. **Phase 6**: Corruption resilience + edge cases (R-06 tests)

Each phase has a clear gate. Failure at Phase 2 triggers the ndarray fallback path without invalidating work from Phase 1 (shared infrastructure is framework-agnostic).
