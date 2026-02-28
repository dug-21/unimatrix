# Risk-Based Test Strategy: crt-006

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | MicroLoRA gradient computation error: incorrect gradients produce adaptation that degrades search quality instead of improving it | High | Medium | Critical |
| R-02 | InfoNCE numerical instability: NaN/Inf propagation from exp(sim/tau) overflow corrupts weights silently | High | Low | High |
| R-03 | Training-induced regression: adapted embeddings reduce retrieval quality for some query patterns even as they improve others | Medium | Medium | High |
| R-04 | Adaptation state deserialization failure on version upgrade: state file from older version fails to load, losing accumulated training | Medium | Medium | High |
| R-05 | Race condition between concurrent reads and training write: readers see partially swapped weights during atomic update | High | Low | High |
| R-06 | Reservoir sampling bias: non-uniform sampling of co-access pairs leads to adaptation biased toward dominant topic clusters | Medium | Medium | Medium |
| R-07 | EWC++ numerical drift: after thousands of training steps, Fisher diagonal values become numerically degenerate (all near-zero or all near-max) | Medium | Low | Medium |
| R-08 | Prototype centroid instability: running-mean centroids oscillate when entries are frequently corrected or topics evolve rapidly | Low | Medium | Medium |
| R-09 | Forward pass latency regression: adaptation adds measurable latency to every embedding operation, degrading tool response times | Medium | Low | Medium |
| R-10 | Embedding consistency check false positives: adapted re-embeddings differ slightly from indexed embeddings due to training between index time and check time | Medium | High | High |
| R-11 | Memory leak in training reservoir: pairs accumulate beyond capacity bounds due to reservoir sampling implementation error | High | Low | Medium |
| R-12 | Cold-start performance: fresh project with no co-access data has measurably different behavior than before crt-006 (near-identity should be transparent) | Low | Medium | Low |
| R-13 | ndarray edition 2024 incompatibility: ndarray or its dependencies fail to compile under edition 2024 / MSRV 1.89 | High | Low | High |

## Risk-to-Scenario Mapping

### R-01: Gradient Computation Error
**Severity**: High
**Likelihood**: Medium
**Impact**: Adaptation degrades search quality. Entries that should be found are missed. The system becomes worse with training.

**Test Scenarios**:
1. Finite-difference gradient validation: compute analytical gradients for A and B, compare against numerical gradients (perturbation-based). Max difference < 1e-4.
2. Convergence test: train on synthetic co-access data where ground truth is known. After N steps, verify loss decreases monotonically.
3. Round-trip test: after training, search quality for co-accessed entries should improve (not degrade) vs identity baseline.

**Coverage Requirement**: Gradient correctness must be validated for both A and B matrices, at rank 2, 4, 8, and 16. Loss convergence tested on at least 3 synthetic datasets.

### R-02: InfoNCE Numerical Instability
**Severity**: High
**Likelihood**: Low
**Impact**: NaN weights render all subsequent embeddings as NaN vectors. HNSW search returns garbage.

**Test Scenarios**:
1. Extreme similarity input: pairs with similarity > 0.99 and tau=0.07 (exp(14.1) challenge). Verify no NaN in loss.
2. Extreme dissimilarity input: pairs with similarity < 0.01. Verify no NaN.
3. Mixed batch: batch containing both extreme similarity and dissimilarity pairs. Verify loss is finite.
4. NaN guard trigger: inject NaN into input embeddings. Verify training step is aborted and weights unchanged.

**Coverage Requirement**: Every code path that computes exp() must be tested with values that would overflow without log-sum-exp.

### R-03: Training-Induced Regression
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Some queries return worse results after adaptation training.

**Test Scenarios**:
1. Baseline comparison: establish search quality metrics before training. After training, verify no query degrades by more than a threshold.
2. Cross-topic interference: train on topic A pairs. Verify topic B search quality is not degraded.
3. EWC effectiveness: train on topic A, then topic B. Verify topic A quality is preserved by EWC regularization.

**Coverage Requirement**: Integration test A-03 validates core value proposition. Unit tests verify EWC prevents forgetting.

### R-04: State Deserialization Failure
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Accumulated training is lost. System falls back to identity (functional but loses adaptation).

**Test Scenarios**:
1. Version upgrade: serialize state with version N, add a new field, load with version N+1 code. Verify `serde(default)` fills missing fields.
2. Corrupt file: truncated state file. Verify graceful fallback to identity.
3. Zero-byte file: empty state file. Verify graceful fallback.
4. Wrong-dimension state: state with dimension=768 loaded by dimension=384 server. Verify rejection with clear error.

**Coverage Requirement**: Every failure mode of `load_state()` must be tested. Integration test A-02 validates persistence round-trip.

### R-05: Concurrent Read/Write Race
**Severity**: High
**Likelihood**: Low
**Impact**: Reader sees inconsistent weight matrices (new A, old B). Produces corrupted adapted embeddings.

**Test Scenarios**:
1. Concurrent read during write: spawn 100 concurrent forward passes while a training step runs. Verify no panics, no NaN in any output.
2. Atomic swap verification: after training step, verify all readers see either the full old weights or the full new weights (never a mix).

**Coverage Requirement**: RwLock atomic swap must be verified under contention.

### R-06: Reservoir Sampling Bias
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Adaptation overfits to dominant topics; minority topics get poor retrieval quality.

**Test Scenarios**:
1. Uniform sampling test: insert 10K pairs from 10 equal-sized topics. Verify sampled batch has approximately equal topic representation (chi-squared test).
2. Skewed input: insert 9K pairs from topic A, 1K from topic B. Verify topic B pairs appear in samples proportionally.
3. Capacity overflow: insert more pairs than capacity. Verify final sample is uniform over all observed pairs.

**Coverage Requirement**: Statistical validation of reservoir uniformity.

### R-07: EWC++ Numerical Drift
**Severity**: Medium
**Likelihood**: Low
**Impact**: Regularization becomes ineffective after many training steps, leading to catastrophic forgetting.

**Test Scenarios**:
1. Long-sequence stability: run 10K simulated EWC updates with alpha=0.95. Verify Fisher diagonal values remain in reasonable range (no underflow/overflow).
2. Regularization effectiveness: after 1K updates, verify EWC penalty still meaningfully constrains weight changes.

**Coverage Requirement**: Fisher diagonal values checked for NaN/Inf and reasonable magnitude after extended training.

### R-10: Embedding Consistency False Positives
**Severity**: Medium
**Likelihood**: High
**Impact**: crt-005 coherence gate reports false inconsistencies because training changed weights between embedding and check.

**Test Scenarios**:
1. Stable-weight check: with no active training, re-embed and re-adapt an entry. Verify consistency score is 1.0 (identical).
2. Post-training check: immediately after a training step, run consistency check. Measure the magnitude of inconsistency caused by weight change. Define threshold.
3. Weight snapshot: verify that consistency check snapshots weights before starting the batch check.

**Coverage Requirement**: Integration test A-04 validates consistency check with adaptation active. Unit test verifies snapshot behavior.

### R-13: ndarray Edition 2024 Compatibility
**Severity**: High
**Likelihood**: Low
**Impact**: Crate does not compile, blocking the entire feature.

**Test Scenarios**:
1. Compile check: `cargo check` succeeds with ndarray under edition 2024.
2. All tests pass: `cargo test` in unimatrix-adapt succeeds.

**Coverage Requirement**: CI must compile and test the new crate.

## Integration Risks

| Risk ID | Risk | Test Scenario |
|---------|------|---------------|
| IR-01 | Adaptation inserts between embed and vector on write path: dimension mismatch, normalization error, or type mismatch could break HNSW insertion | Store an entry via context_store, verify it appears in context_search results |
| IR-02 | Query adaptation must match entry adaptation: if queries skip adaptation or use different weights, the embedding spaces diverge and search returns garbage | Search for an entry immediately after storing it; verify it appears in top-3 results |
| IR-03 | Co-access pair recording must feed the training reservoir: if the connection is broken, training never triggers | Generate co-access pairs via searches, verify reservoir receives them |
| IR-04 | Shutdown persistence order: adaptation state must be saved before process exit. If HNSW dump fails first and triggers panic, adaptation state may not save | Verify adaptation state file exists after clean shutdown |
| IR-05 | Maintenance re-indexing must use current adaptation weights: stale entries must be re-adapted with current weights, not the weights they were originally indexed with | After training, run maintenance, verify re-indexed entries use current generation |

## Edge Cases

| Case | Description | Expected Behavior |
|------|-------------|-------------------|
| EC-01 | Empty knowledge base (zero entries, zero pairs) | Adaptation is identity (near-zero B). No training triggers. No errors. |
| EC-02 | Single entry in knowledge base | No co-access pairs possible. Adaptation is identity. |
| EC-03 | Fewer pairs than batch size | Training does not trigger until sufficient pairs accumulate. |
| EC-04 | All pairs from one topic | Training biased to one topic. EWC should prevent forgetting others, but cross-topic quality may not improve. |
| EC-05 | Entry corrected multiple times | Each correction re-embeds through adaptation. Correction chain intact. |
| EC-06 | Unicode content in embeddings | ONNX tokenizer handles Unicode. Adaptation operates on f32 vectors, Unicode-agnostic. |
| EC-07 | Rank change between restarts | State file has rank=4 but config now says rank=8. State load fails, fallback to identity. |
| EC-08 | Concurrent training step and HNSW compact | Both acquire write locks on different resources. No deadlock (independent locks). |
| EC-09 | Prototype eviction during forward pass | Forward pass reads prototypes under read lock. Eviction requires write lock. No data race. |
| EC-10 | Reservoir at capacity with identical pairs | Reservoir sampling maintains uniform distribution. Duplicates are replaced proportionally. |

## Security Risks

### Input Surface Analysis

The adaptation pipeline does not accept untrusted external input directly. All inputs come from internal server components:
- Raw embeddings come from the ONNX pipeline (input text is already validated by the server)
- Co-access pairs come from the co-access recording pipeline (entry IDs are validated)
- Configuration is set at server startup (not runtime-modifiable via MCP)

**Potential vectors**:
- **Crafted co-access patterns**: An agent could deliberately generate co-access pairs to bias adaptation toward specific entries (e.g., repeatedly searching for the same entries). Mitigation: reservoir sampling provides some protection; dedicated defense is out of scope (future crt feature).
- **State file injection**: If an attacker can write to the project data directory, they could inject a crafted adaptation state. Mitigation: the data directory is local to the machine. Bincode deserialization with serde validation rejects malformed data.
- **NaN injection via embeddings**: If the ONNX model somehow produces NaN (corrupted model file), the adaptation pipeline propagates NaN. Mitigation: NaN guards in training prevent weight corruption; forward pass NaN would produce poor search results but not crash.

**Blast radius**: Adaptation affects only the current project's embedding space. Cross-project impact is impossible (per-project isolation).

## Failure Modes

| Failure | System Behavior | Recovery |
|---------|----------------|----------|
| Training step produces NaN | Weights not updated. Warning logged. System continues with previous weights. | Automatic -- no intervention needed |
| Adaptation state file missing on startup | Fresh identity state created. Warning logged. | Automatic -- training restarts from scratch |
| Adaptation state file corrupt | Fresh identity state created. Warning logged. Old file renamed to `.corrupt`. | Automatic |
| ndarray allocation failure (OOM) | Forward pass fails. Server returns error for that tool call. | Manual -- increase memory or reduce rank |
| Reservoir overflow (should not happen) | Reservoir sampling replaces existing entries. No data loss. | Automatic |
| Prototype eviction during active use | Forward pass falls back to global centroid or skips pull. | Automatic |
| HNSW index inconsistent with adaptation state (post-restart with corrupt state) | Search results may be suboptimal until maintenance re-indexes. | Run `context_status(maintain=True)` to re-index |

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (Pure Rust ML, gradient errors) | R-01, R-02 | Finite-difference gradient validation. NaN guards on every loss computation. Atomic weight swap prevents partial updates. |
| SR-02 (ndarray dependency) | R-13 | Early compile check under edition 2024. Fallback to hand-written ops identified but not implemented unless ndarray fails. |
| SR-03 (InfoNCE overflow) | R-02 | Log-sum-exp trick in InfoNCE implementation. NaN/Inf guards after every exp() call. |
| SR-04 (Scope breadth -- 4 subsystems) | R-03 | Architecture defines clean component boundaries (7 components). Each subsystem independently testable. Episodic augmentation is lowest-priority if scope must be cut. |
| SR-05 (Training failure handling) | R-05 | ADR-004: RwLock atomic swap. Training failures are no-ops. Weights never partially updated. |
| SR-06 (Persistence coupling) | R-04 | ADR-003: Independent persistence. Missing adaptation state falls back to identity. |
| SR-07 (Consistency check non-determinism) | R-10 | Consistency check snapshots weights before batch. Threshold accounts for training-induced variance. |
| SR-08 (CO_ACCESS full scan) | IR-03 | Training pipeline uses reservoir sampling at pair-recording time. Never reads CO_ACCESS table directly. |
| SR-09 (Forward pass latency) | R-09 | NFR-01: < 10 microseconds target. Pre-allocated buffers. Benchmark validation. |

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | 3 scenarios |
| High | 5 (R-02, R-03, R-04, R-05, R-13) | 12 scenarios |
| Medium | 5 (R-06, R-07, R-08, R-09, R-11) | 7 scenarios |
| Low | 2 (R-10, R-12) | 4 scenarios |
| Integration | 5 (IR-01 through IR-05) | 5 scenarios |
| Edge Cases | 10 (EC-01 through EC-10) | 10 scenarios |
| **Total** | **28** | **41 scenarios** |
