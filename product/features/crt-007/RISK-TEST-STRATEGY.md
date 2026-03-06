# Risk-Based Test Strategy: crt-007

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Shared infra extraction breaks MicroLoRA training pipeline | High | Med | High |
| R-02 | EwcState flat parameter interface produces incorrect regularization due to parameter ordering mismatch | High | Low | Med |
| R-03 | Hand-rolled backpropagation contains gradient computation errors (silent correctness bug) | Med | Med | Med |
| R-04 | Baseline weights produce degenerate classifier output (all-noise or uniform distribution) | Med | Med | Med |
| R-05 | Shadow mode evaluation incorrectly promotes a model that degrades extraction quality | High | Low | Med |
| R-06 | Model deserialization fails after struct layout change (field added/removed) | Med | Med | Med |
| R-07 | Neural inference exceeds latency budget in extraction tick | Low | Low | Low |
| R-08 | shadow_evaluations table write contention with extraction pipeline | Low | Low | Low |
| R-09 | SignalDigest zero-padding dominates gradient flow, preventing meaningful learning | Med | Low | Low |
| R-10 | Auto-rollback triggers spuriously on small evaluation windows | Med | Med | Med |

## Risk-to-Scenario Mapping

### R-01: Shared Infra Extraction Breaks MicroLoRA
**Severity**: High
**Likelihood**: Med
**Impact**: MicroLoRA embedding adaptation stops working. crt-006 regresses.

**Test Scenarios**:
1. Run all 174+ existing unimatrix-adapt tests after refactoring -- zero failures
2. Verify TrainingReservoir<TrainingPair> has identical behavior to old TrainingReservoir (same seed, same output)
3. Verify EwcState penalty/gradient values match pre-refactor values for identical inputs
4. Round-trip persistence: save state with old code, load with refactored code (binary compatibility)

**Coverage Requirement**: Full unimatrix-adapt test suite pass. Regression test comparing pre/post refactor output for deterministic inputs.

### R-02: EwcState Parameter Ordering Mismatch
**Severity**: High
**Likelihood**: Low
**Impact**: EWC regularization penalizes wrong parameters, causing training instability or catastrophic forgetting.

**Test Scenarios**:
1. Verify penalty(params) returns same value before and after flattening refactor for known inputs
2. Verify gradient_contribution produces correct gradients for a 3-element test case with hand-computed expected values
3. Cross-validate: flatten MicroLoRA params, compute penalty via shared EwcState, compare with old EwcState

**Coverage Requirement**: Known-value unit tests with hand-computed expected results. Cross-validation against pre-refactor behavior.

### R-03: Hand-Rolled Backpropagation Gradient Errors
**Severity**: Med
**Likelihood**: Med
**Impact**: Models train on incorrect gradients (crt-008), converging to wrong solutions or diverging. Inference (crt-007) is unaffected -- only forward pass is used initially.

**Test Scenarios**:
1. Numerical gradient check: compare hand-rolled backward pass output against finite-difference approximation for each layer type (sigmoid, relu, softmax, linear)
2. Known-value gradient test: 2-layer network with specific weights, compute expected gradients by hand, verify match
3. Gradient flow test: verify gradients propagate through full classifier network (non-zero gradients reach input layer)

**Coverage Requirement**: Numerical gradient tests for each activation function. Known-value end-to-end gradient test for each model.

### R-04: Baseline Weights Produce Degenerate Output
**Severity**: Med
**Likelihood**: Med
**Impact**: Classifier always says Noise (or uniform), rendering neural enhancement useless.

**Test Scenarios**:
1. All-zero digest: classifier should produce Noise (by design) -- verify Noise probability > 0.5
2. Non-zero typical digest (simulating real ProposedEntry): classifier should produce non-Noise category with measurable probability
3. Convention scorer on all-zero digest: should produce score < 0.3
4. Convention scorer on high-confidence digest: should produce score > 0.1 (not completely dead)

**Coverage Requirement**: Smoke test with 3+ representative digests covering expected input distributions.

### R-05: Incorrect Shadow Promotion Degrades Extraction
**Severity**: High
**Likelihood**: Low
**Impact**: Neural model suppresses valid extractions or promotes noise entries.

**Test Scenarios**:
1. Simulate shadow evaluation with a model that agrees with rules 100% -- verify promotion
2. Simulate shadow evaluation with a model that disagrees on 1 category -- verify per-category regression check blocks promotion
3. Simulate post-promotion accuracy drop > 5% -- verify auto-rollback triggers
4. Verify entries stored after rollback use rule-only trust_source "auto" (not "neural")

**Coverage Requirement**: Unit tests for all promotion criteria (accuracy threshold, minimum evaluations, per-category check). Integration test for rollback flow.

### R-06: Model Deserialization Fails After Struct Layout Change
**Severity**: Med
**Likelihood**: Med
**Impact**: Models become unloadable after weight struct changes (e.g., adding a layer). System cold-starts with baseline weights, losing learned accuracy.

**Test Scenarios**:
1. Save model via NeuralModel::serialize, verify NeuralModel::deserialize round-trip succeeds
2. Verify ModelVersion includes schema_version field
3. Simulate schema version mismatch: ModelRegistry detects and cold-starts with baseline weights instead of panicking
4. Verify corrupt model file produces graceful fallback (warning log, baseline weights)

**Coverage Requirement**: Save/load round-trip test. Corrupt file handling. Schema version mismatch detection.

### R-07: Neural Inference Exceeds Latency Budget
**Severity**: Low
**Likelihood**: Low
**Impact**: Extraction tick takes longer. Not critical (15-min interval) but undesirable.

**Test Scenarios**:
1. Benchmark: 100 classifier inferences, p99 < 50ms
2. Benchmark: 100 scorer inferences, p99 < 10ms
3. Combined: 10 entries through full neural enhancement path, total < 100ms

**Coverage Requirement**: Benchmark tests with timing assertions (can be gated behind #[cfg(not(debug_assertions))] for CI).

### R-08: shadow_evaluations Write Contention
**Severity**: Low
**Likelihood**: Low
**Impact**: Extraction tick slows down or fails due to SQLite lock contention.

**Test Scenarios**:
1. Write 100 shadow evaluation rows in a batch -- verify completes < 100ms
2. Concurrent read of shadow_evaluations during write -- verify no deadlock

**Coverage Requirement**: Basic write performance test. No dedicated contention test needed (rate limited to 10/hour).

### R-09: Zero-Padding Gradient Dominance
**Severity**: Med
**Likelihood**: Low
**Impact**: Models cannot learn meaningful features because gradients from 25 zero slots overwhelm 7 active slots.

**Test Scenarios**:
1. Forward pass with only active slots non-zero -- verify output is not uniform
2. Verify weight gradients for active slots have larger magnitude than zero-slot gradients (in a training step, crt-008 scope)

**Coverage Requirement**: Smoke test for non-degenerate output. Full gradient analysis deferred to crt-008.

### R-10: Spurious Auto-Rollback
**Severity**: Med
**Likelihood**: Med
**Impact**: Model cycles between promotion and rollback, never stabilizing.

**Test Scenarios**:
1. Simulate accuracy that fluctuates by 3% -- verify no rollback (within 5% tolerance)
2. Simulate accuracy that drops 6% -- verify rollback triggers
3. Verify rollback requires minimum window size (50 predictions) before triggering

**Coverage Requirement**: Unit tests for rolling accuracy calculation with edge cases (window boundary, exactly 5% drop, 4.9% drop).

## Integration Risks

- **unimatrix-learn <-> unimatrix-adapt boundary**: TrainingReservoir generic type parameter changes the add() signature. unimatrix-adapt must pass `&[(u64, u64, u32)]` via a conversion. Test: existing add() call sites compile and produce same reservoir state.
- **unimatrix-learn <-> unimatrix-server**: NeuralEnhancer initialization requires models directory path from server config. Test: server creates NeuralEnhancer with correct path during startup.
- **unimatrix-observe <-> unimatrix-learn**: ProposedEntry -> SignalDigest conversion must handle all 5 extraction rule categories. Test: each rule's output produces a valid non-zero digest.
- **SQLite schema version**: shadow_evaluations table requires schema bump. Test: migration creates table without disrupting existing tables.

## Edge Cases

- Empty extraction tick (no observations): NeuralEnhancer should not be called. No shadow logs written.
- All models fail to load: Pipeline degrades to rule-only. No crash, warning log emitted.
- Model file partially written (crash during save): atomic save (temp + rename) prevents corruption. Test: incomplete tmp file does not affect production model.
- Promotion attempted with < 20 evaluations: rejected, model stays in Shadow.
- Rollback when no Previous model exists: rollback fails gracefully, production model stays (with warning).
- SignalDigest from ProposedEntry with empty tags/source_features: normalized values = 0.0, valid input.

## Security Risks

- **Model file injection**: An attacker could place a malicious model file in the models directory. Mitigation: model files are deserialized by bincode into f32 weight matrices, not arbitrary code execution. Blast radius: incorrect classification (not RCE).
- **Shadow evaluation data**: shadow_evaluations table is append-only, written by background tick only. No MCP tool exposes it. No external input path.
- **SignalDigest construction**: Input derived from ProposedEntry fields (already validated by quality gate checks 1-4). No untrusted external input reaches SignalDigest directly.
- **trust_source "neural"**: Neural entries get 0.40 confidence weight vs 0.35 for "auto". An attacker who can control model weights could boost confidence of extracted entries. Mitigation: shadow mode evaluation period, auto-rollback, and all entries still pass quality gate checks 5-6 (near-duplicate, contradiction).

## Failure Modes

| Failure | Expected Behavior |
|---------|-------------------|
| Model inference produces NaN/panics | Caught by tokio::spawn_blocking. Log error, skip neural enhancement for this tick. Pipeline continues rule-only. |
| Model file corrupt/missing | ModelRegistry returns None for model slot. NeuralEnhancer cold-starts with baseline weights. Warning logged. |
| Shadow evaluation write fails | Log error, skip shadow log. Non-fatal -- no impact on extraction. |
| Promotion criteria never met | Model stays in Shadow indefinitely. Pipeline operates rule-only. Acceptable -- conservative by design. |
| Rollback with no Previous | Rollback fails, production model retained. Warning logged. Manual cold-restart available via file deletion. |
| Schema migration fails | Server startup fails with clear error. User must address (backup + retry). |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (backprop correctness) | R-03 | Numerical gradient checks + known-value tests for each layer type |
| SR-02 (ndarray version conflict) | -- | Eliminated: single math library (ndarray only) |
| SR-03 (serialization fragility) | R-06 | schema_version in ModelVersion; NeuralModel::deserialize handles mismatches |
| SR-04 (degenerate baseline weights) | R-04 | Smoke test with representative digests |
| SR-05 (shared infra extraction breaks adapt) | R-01 | All 174+ adapt tests as hard gate |
| SR-06 (zero-padding gradient dominance) | R-09 | Smoke test for non-degenerate output. Full analysis in crt-008 |
| SR-07 (feature count tracking for shadow promotion) | R-05, R-10 | Shadow mode uses evaluation count (20 minimum), not feature count |
| SR-08 (inference latency in tick) | R-07 | Benchmark tests with SLA assertions |
| SR-09 (EwcState shared interface) | R-02 | ADR-002: flat Vec<f32> interface, hand-computed known-value tests |
| SR-10 (shadow log write pressure) | R-08 | Batch writes, low volume (10/hour rate limit) |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| High | 3 (R-01, R-02, R-05) | 11 scenarios |
| Medium | 5 (R-03, R-04, R-06, R-09, R-10) | 14 scenarios |
| Low | 2 (R-07, R-08) | 5 scenarios |
| **Total** | **10** | **30 scenarios** |
