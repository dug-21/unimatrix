# Scope Risk Assessment: crt-006

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Pure Rust ML implementation: manual gradient computation (InfoNCE + EWC++ + MicroLoRA backward pass) is error-prone without framework auto-diff. Silent numerical bugs could produce adaption that degrades rather than improves search quality. | High | Medium | Architect should design extensive numerical validation harness: compare gradients against finite-difference approximations. Spec should require gradient correctness tests for every trainable path. |
| SR-02 | ndarray dependency introduces a large transitive dependency graph (potential BLAS backends, platform-specific build issues). Build breakage or edition 2024 incompatibility could block compilation. | Medium | Medium | Architect should evaluate ndarray edition 2024 compatibility early. Consider fallback to hand-written matmul if ndarray proves problematic (feasible at rank <= 16). |
| SR-03 | InfoNCE temperature tau=0.07 with f32 creates overflow risk in exp(sim/tau). Log-sum-exp trick mitigates but implementation errors here cause silent NaN propagation. | High | Low | Spec should require explicit NaN/Inf guards after every loss computation. Architect should design the loss function to return Result, not silently propagate NaN. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | Scope includes both MicroLoRA training AND episodic augmentation AND prototype management AND EWC++ -- four distinct ML subsystems in one feature. Risk of partial delivery or rushed implementation of later subsystems. | Medium | Medium | Architect should design clean component boundaries so subsystems can be implemented incrementally. If scope must be cut, episodic augmentation is the lowest-priority subsystem. |
| SR-05 | "Training triggered inline during usage recording" (Goal 10) is ambiguous about failure handling. What happens when training fails mid-batch? Silent swallow? Logged warning? Rollback of partial weight updates? | Medium | High | Spec must define training failure modes explicitly. Architect should ensure atomic weight updates (compute new weights fully, then swap). |
| SR-06 | Adaptation state persistence "alongside HNSW dump" couples two independent persistence concerns. If HNSW dump fails, does adaptation state also fail? If adaptation state is corrupted, does HNSW become unusable? | Medium | Low | Architect should design independent persistence for adaptation state. Failure of one should not corrupt or block the other. Server should handle missing adaptation state gracefully (fall back to identity transform). |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | Embedding consistency check (crt-005) must compare adapted re-embeddings, but adapted re-embeddings depend on current MicroLoRA weights which change with training. Consistency check results become non-deterministic during active training. | High | High | Architect must define whether consistency checks snapshot weights or tolerate training-induced variance. Spec should define acceptable consistency threshold with adaptation active. |
| SR-08 | CO_ACCESS table full scan in `get_co_access_partners()` (read.rs:244) is a known bottleneck. Training pipeline sampling strategy must avoid triggering this scan, but the scope references co-access data as the training signal. | Medium | Medium | Architect should design training data access patterns that never call `get_co_access_partners()`. Reservoir sampling should intercept pairs at recording time, not scan the table. |
| SR-09 | Adaptation layer sits between embed and vector on EVERY embedding operation. Any latency regression (even microseconds per call) multiplies across all store/search/lookup operations. At scale (100+ ops/sec), this compounds. | Medium | Low | Spec should define latency budget for adaptation forward pass (target: < 10 microseconds). Architect should benchmark pre-allocated buffer approach against allocation-per-call. |

## Assumptions

1. **ndarray works on edition 2024 with MSRV 1.89** (referenced in Background Research "ndarray vs Alternatives" section). If ndarray requires older edition or newer MSRV, the implementation approach changes significantly.
2. **Co-access pairs provide meaningful training signal** (Problem Statement). If co-access patterns are too noisy or sparse in real usage, InfoNCE training may not converge to useful adaptations. This is a fundamental bet.
3. **Rank 4 is sufficient for most projects** (Scale Scenarios section). If rank 4 is insufficient and users need rank 16 routinely, the performance and memory characteristics change (4x parameters, 4x training time).
4. **Reservoir sampling provides representative batches** (Co-Access Pair Scaling section). If the pair distribution is heavily skewed (common in real usage), reservoir sampling may oversample dominant clusters and undersample rare but important relationships.

## Design Recommendations

1. **SR-01, SR-03**: Design the training pipeline with explicit error propagation (Result types, not panics). Every numerical operation that can produce NaN/Inf should check and propagate errors. Include finite-difference gradient validation as a test requirement.
2. **SR-04**: Component boundaries should allow independent testing and incremental delivery. Architecture should define a trait or interface per subsystem so they can be enabled/disabled independently.
3. **SR-07**: Define a "training lock" or weight snapshot mechanism for consistency checks. The consistency check must see a stable view of adaptation weights.
4. **SR-08**: Reservoir sampling must operate at pair-recording time (intercept in `record_co_access_pairs`), not via table scans. The training pipeline should never read the CO_ACCESS table directly.
