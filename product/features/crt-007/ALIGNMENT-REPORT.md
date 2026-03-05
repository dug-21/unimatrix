# Vision Alignment Report: crt-007 Neural Extraction Pipeline

## Assessment Summary

| Dimension | Status | Notes |
|-----------|--------|-------|
| Vision alignment | PASS | Directly implements "self-learning" vision statement |
| Roadmap consistency | PASS | Matches PRODUCT-VISION.md crt-007 description exactly |
| Dependency chain | PASS | col-013 -> crt-007 -> crt-008 ordering preserved |
| Scope boundaries | PASS | Non-goals correctly defer crt-008/009 concerns |
| Architecture principles | PASS | Follows established patterns (trait abstraction, SQLite, spawn_blocking) |
| Risk posture | PASS | Conservative by design (shadow mode, baseline bias, auto-rollback) |

**Overall: PASS** (0 variances, 0 warnings)

---

## Detailed Alignment Analysis

### 1. Vision Statement Alignment

**Vision**: "Unimatrix is a self-learning expertise engine for multi-agent software development."

**crt-007 contribution**: Introduces the first neural models that can learn from domain data. While crt-007 itself ships with baseline weights (no training), it builds the infrastructure (ModelRegistry, ShadowEvaluator, TrainingReservoir<T>) that enables crt-008 (Continuous Self-Retraining) to close the self-learning loop. The shadow mode validation protocol ensures models prove themselves before influencing the knowledge base.

**Assessment**: PASS -- directly serves the "self-learning" aspect of the vision. The conservative approach (observation -> shadow -> production) is appropriate for a system that must maintain trust.

### 2. Roadmap Feature Description Match

**PRODUCT-VISION.md states**:
> Neural Extraction Pipeline (crt-007): Integrate ruv-fann neural models for knowledge extraction from behavioral signals. Two initial models: Signal Classifier MLP (~5MB) and Convention Scorer MLP (~2MB). Shadow mode validation before activation. Shared training infrastructure refactor: extract TrainingReservoir, EwcState, ModelRegistry from unimatrix-adapt into shared unimatrix-learn module. Cold start with hand-tuned baseline weights. Model versioning: production/shadow/previous with auto-rollback (>5% accuracy drop). Open risk: ruv-fann v0.2.0.

**crt-007 SCOPE.md delivers**:
- Two models (Signal Classifier, Convention Scorer) -- MATCH
- Shadow mode validation -- MATCH
- Shared training infrastructure extraction -- MATCH (unimatrix-learn crate)
- Cold start with baseline weights -- MATCH (ADR-006)
- Model versioning with auto-rollback -- MATCH (ADR-005)
- ruv-fann with fallback -- MATCH (ADR-002)

**Assessment**: PASS -- scope exactly matches vision description. No scope creep, no missing elements.

### 3. Dependency Chain Integrity

**Vision ordering**: col-012 -> col-013 -> crt-007 -> crt-008 -> crt-009

**crt-007 dependencies**:
- col-013 (extraction pipeline, background tick) -- correct
- crt-006 (unimatrix-adapt exists for refactoring) -- correct

**crt-007 enables**:
- crt-008 (continuous self-retraining uses ModelRegistry, TrainingReservoir<T>, EwcState, ShadowEvaluator)
- crt-009 (advanced models added to unimatrix-learn, consume same infrastructure)

**Assessment**: PASS -- clean dependency chain, no ordering violations.

### 4. Scope Boundary Alignment

**Correctly in scope**:
- Shared training infra extraction (vision explicitly calls for this)
- Two models (not five -- vision says "two initial models")
- Shadow mode (vision: "shadow mode validation before activation")
- Model versioning (vision: "production/shadow/previous with auto-rollback")
- trust_source "neural" (vision: CRT integration refactors)

**Correctly out of scope**:
- Duplicate Detector, Pattern Merger, Entry Writer Scorer (crt-009)
- Continuous self-retraining (crt-008)
- LLM API integration (crt-009)
- context_review MCP tool (crt-009)
- Lesson extraction (permanently agent-driven per vision)

**Assessment**: PASS -- scope boundaries match the vision's feature-by-feature decomposition.

### 5. Architecture Principle Alignment

| Principle | crt-007 Compliance |
|-----------|-------------------|
| Anti-stub (CLAUDE.md) | All code paths fully implemented; no TODO/unimplemented!() |
| Test infrastructure cumulative | Extends existing test patterns; shadow_evaluations table extends migration test infrastructure |
| SQLite as storage backend | Shadow evaluations in SQLite (ADR-004), consistent with nxs-005/006/008 decisions |
| spawn_blocking for CPU work | Neural inference runs in spawn_blocking within extraction_tick (follows col-013 pattern) |
| Per-project isolation | Models scoped to {project_hash}/models/ directory |
| Confidence pipeline integration | trust_source "neural" -> 0.40 integrates with existing 6-factor composite |
| Schema versioning | v7->v8 migration follows established ALTER TABLE pattern |

**Assessment**: PASS -- no architectural principle violations.

### 6. Risk Posture Alignment

**Vision states**: "Cold start with hand-tuned baseline weights biased toward conservative extraction."

**crt-007 implements**:
- Classifier noise bias +2.0 (~60% noise probability on zero input)
- Scorer output bias -1.0 (sigmoid ~= 0.27, below 0.6 Active threshold)
- 5-feature observation period before shadow mode
- 20-evaluation minimum before promotion
- >5% accuracy drop triggers auto-rollback
- NaN/Inf detection in model parameters

**Vision states**: "Open risk: ruv-fann v0.2.0 (~4K downloads); fallback to ndarray + hand-rolled training if RPROP insufficient."

**crt-007 mitigates**: NeuralModel trait (ADR-002) confines ruv-fann to implementation files. ndarray fallback is a bounded effort. R-01 tests gate the decision: fail -> fallback.

**Assessment**: PASS -- conservative posture matches vision intent. Risk mitigations are concrete and testable.

### 7. CRT Integration Refactors

**Vision states**:
- crt-002: Add "neural" trust_source value (~5 lines)
- crt-006/unimatrix-adapt: Extract TrainingReservoir, EWC++, persistence helpers into shared module (~250 lines moved)

**crt-007 delivers**:
- trust_source "neural" -> 0.40 in confidence.rs (FR-07) -- MATCH
- unimatrix-learn crate extraction (~250 lines moved from adapt) -- MATCH

Note: crt-002 "auto" trust_source was added in col-013 (already merged). crt-007 adds only "neural".

**Assessment**: PASS -- CRT refactors match vision specification.

### 8. Self-Learning Pipeline Position

crt-007 is the second feature in the 5-feature self-learning pipeline:

```
col-012 (data unification) -> col-013 (rule engine) -> crt-007 (neural models)
    -> crt-008 (self-retraining) -> crt-009 (advanced models + LLM)
```

crt-007's specific role: introduce the neural infrastructure and models that crt-008 will train. Without crt-007, crt-008 has nothing to retrain. Without crt-008, crt-007's models are static (baseline weights only). This interdependency is by design -- crt-007 proves the inference path works before crt-008 adds the learning path.

**Assessment**: PASS -- correctly positioned in the pipeline.

---

## Variance Register

No variances identified. All scope elements, architectural decisions, and non-goals align with PRODUCT-VISION.md.

## Recommendations

1. **Monitor ruv-fann during implementation**: If R-01 tests fail early, switch to ndarray fallback before investing in shadow mode integration. The NeuralModel trait (ADR-002) makes this a localized change.

2. **Document SignalDigest slot assignments**: The SIGNAL_SLOTS const (ADR-003) should be prominently documented since it becomes a cross-feature contract used by crt-008/009. Consider a dedicated markdown file alongside the code.

3. **Shadow mode metrics dashboard**: While not in crt-007 scope, consider surfacing shadow evaluation metrics in context_status output so operators can monitor model readiness without querying SQLite directly. This could be a small addition during implementation or a crt-008 concern.
