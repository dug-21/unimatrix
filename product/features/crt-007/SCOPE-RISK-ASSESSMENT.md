# Scope Risk Assessment: crt-007

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | burn framework adds significant binary size and compile time; NdArray CPU backend pulls in substantial dependencies | Med | High | Architect should measure binary delta and compile time; consider feature-gating burn behind a cargo feature flag |
| SR-02 | burn's `burn-ndarray` backend and unimatrix-adapt's direct `ndarray` usage may cause version conflicts or duplicate types in the dependency tree | High | Med | Pin ndarray version across workspace; architect should verify burn's ndarray version compatibility before committing |
| SR-03 | burn v0.16+ API is evolving; model serialization format may change between versions, breaking persisted model files | Med | Med | Pin exact burn version; ModelRegistry should include burn version in model metadata for compatibility detection |
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
| SR-08 | Neural models run inside col-013's background tick; burn tensor allocation and inference latency may exceed tick budget expectations | Med | Low | Specify inference latency SLA (<50ms classifier, <10ms scorer); architect should benchmark burn NdArray MLP forward pass |
| SR-09 | unimatrix-adapt uses ndarray `Array2<f32>` for EWC Fisher matrices; shared EwcState must bridge to burn tensor parameters without copying overhead | Med | Med | Design EwcState around `Vec<f32>` flat parameter vectors; both ndarray and burn consumers convert at their boundary |
| SR-10 | Shadow evaluation logs in SQLite add write pressure to the same database used by the extraction pipeline and MCP server | Low | Low | Use batched inserts; architect should confirm shadow log writes do not contend with extraction pipeline transactions |

## Assumptions

- **A1** (Proposed Approach, Phase 1): unimatrix-adapt's `TrainingReservoir`, `EwcState`, and persistence helpers are cleanly separable from MicroLoRA-specific code without changing their semantics.
- **A2** (Resolved Q1): burn's NdArray backend provides sufficient CPU performance for MLP inference on small models (<5MB) without GPU acceleration.
- **A3** (Constraints): The col-013 background tick runs at 15-minute intervals, providing ample time budget for neural inference that completes in <100ms.
- **A4** (Resolved Q5): Zero-initialized reserved slots in the 32-float SignalDigest do not meaningfully degrade model accuracy for the initial 6-7 active features.

## Design Recommendations

- **For architect** (SR-01, SR-02): Investigate burn's dependency tree before committing. If binary size exceeds 10MB delta or ndarray version conflicts arise, consider isolating burn behind a cargo feature flag or in a separate process.
- **For architect** (SR-05, SR-09): Design the unimatrix-learn API contract as `Vec<f32>` flat parameter vectors, not framework-specific tensor types. Both ndarray (adapt) and burn (learn) consumers own their conversion.
- **For spec writer** (SR-04, SR-07): Define concrete smoke-test signals for baseline weight validation. Specify the feature-count tracking mechanism for shadow mode promotion.
- **For architect** (SR-03): Include burn crate version in ModelVersion metadata so ModelRegistry can detect and handle version mismatches during deserialization.
