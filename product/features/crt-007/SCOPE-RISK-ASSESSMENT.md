# Scope Risk Assessment: crt-007

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Hand-rolled MLP backpropagation may contain gradient computation errors that silently produce incorrect training (crt-008) | Med | Med | Require known-value gradient tests with hand-computed expected results for each layer type |
| SR-02 | (Eliminated) ndarray version conflict -- single math library, no conflict possible | -- | -- | -- |
| SR-03 | Model serialization via bincode may be fragile across struct layout changes; adding fields to weight struct breaks deserialization | Med | Med | ModelRegistry should include schema_version in model metadata; NeuralModel::deserialize must handle version mismatches gracefully |
| SR-04 | Hand-tuned baseline weights may produce degenerate outputs (all-noise classification) without empirical validation on real signal distributions | Med | Med | Define a smoke-test signal set during architecture; validate baseline weights produce non-trivial distributions |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-05 | Shared infra extraction (~250 lines from unimatrix-adapt) risks breaking the MicroLoRA training pipeline if API boundaries are drawn incorrectly | High | Med | Define the shared API contract before moving code; all 174+ adapt tests must pass as a hard gate |
| SR-06 | Fixed-width 32-slot SignalDigest couples model topology to feature vector layout; "reserved slots" initialized to zero may bias training | Low | Low | Document slot assignment table in specification; architect should confirm zero-padding does not dominate gradient flow for small feature counts |
| SR-07 | Shadow mode "5 features observation-only" requires feature counting across sessions; no existing mechanism tracks feature delivery count | Med | Med | Architect should define how feature count is tracked -- session metadata, config file counter, or Unimatrix outcome entries |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-08 | Neural models run inside col-013's background tick; ndarray matrix multiply and activation latency should be negligible but must be verified | Low | Low | Specify inference latency SLA (<50ms classifier, <10ms scorer); ndarray MLP forward pass should be sub-millisecond |
| SR-09 | (Simplified) EwcState shared via flat Vec<f32> interface -- no framework bridging needed since all consumers use ndarray | Low | Low | Design EwcState around Vec<f32> flat parameter vectors; unimatrix-adapt flattens grad_a/grad_b at call site |
| SR-10 | Shadow evaluation logs in SQLite add write pressure to the same database used by the extraction pipeline and MCP server | Low | Low | Use batched inserts; architect should confirm shadow log writes do not contend with extraction pipeline transactions |

## Assumptions

- **A1** (Proposed Approach, Phase 1): unimatrix-adapt's `TrainingReservoir`, `EwcState`, and persistence helpers are cleanly separable from MicroLoRA-specific code without changing their semantics.
- **A2** (Resolved Q1): ndarray `Array2<f32>` matrix multiplies provide sufficient CPU performance for MLP inference on small models (< 100KB weights) without GPU acceleration.
- **A3** (Constraints): The col-013 background tick runs at 15-minute intervals, providing ample time budget for neural inference that completes in <100ms.
- **A4** (Resolved Q5): Zero-initialized reserved slots in the 32-float SignalDigest do not meaningfully degrade model accuracy for the initial 6-7 active features.

## Design Recommendations

- **For architect** (SR-01): Require gradient correctness tests for each activation function (sigmoid, relu, softmax) and each layer type. Compare hand-computed gradients against numerical differentiation for a small test case.
- **For architect** (SR-05, SR-09): Design the unimatrix-learn API contract as `Vec<f32>` flat parameter vectors. Both MicroLoRA and MLP consumers flatten at their boundary.
- **For spec writer** (SR-04, SR-07): Define concrete smoke-test signals for baseline weight validation. Specify the feature-count tracking mechanism for shadow mode promotion.
- **For architect** (SR-03): Include schema_version in ModelVersion metadata so ModelRegistry can detect and handle serialization mismatches.
