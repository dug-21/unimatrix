# Scope Risk Assessment: crt-008 Continuous Self-Retraining

## SR-01: NeuralModel Trait Split May Break crt-007 Consumers

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Compilation failures in unimatrix-learn and unimatrix-server after trait change

crt-008 Phase 0 splits `train_step` into `compute_gradients` + `apply_gradients` on the `NeuralModel` trait. Any code calling `train_step` directly (if crt-007 shipped tests or benchmarks using it) will need updating. The risk is low if `train_step` becomes a default impl calling both new methods, but the trait change touches both `SignalClassifier` and `ConventionScorer` implementations (~10 lines each). If crt-007 is still in progress when crt-008 starts, the trait split could conflict with in-flight implementation work.

**Mitigation**: Coordinate with crt-007 implementation. If crt-007 has not yet merged, include the trait split in crt-007's delivery. If crt-007 is merged, the split is a backward-compatible additive change (default impl of `train_step` preserves existing behavior).

---

## SR-02: EWC Gradient Injection Correctness

**Severity**: High
**Likelihood**: Low
**Impact**: Models learn incorrectly, catastrophic forgetting not prevented, silent quality degradation

The EWC gradient contribution must be added element-wise to the task gradients BEFORE the SGD weight update, respecting the same deterministic parameter ordering used by `flat_parameters()` and `set_parameters()`. If the ordering is misaligned between `compute_gradients` output and `EwcState.gradient_contribution()`, the penalty pushes the wrong weights, causing training instability or silent mislearning. Since both use hand-rolled ndarray code, there is no framework-level guarantee of consistent ordering.

**Mitigation**: crt-007 ADR-002 establishes deterministic layer-by-layer row-major parameter ordering. crt-008 must add a unit test that verifies `flat_parameters()` ordering matches the gradient vector ordering from `compute_gradients()` by training one step and checking that `set_parameters(flat_parameters())` is an identity operation. The EWC integration test should verify that penalty decreases over training steps as parameters converge to reference.

---

## SR-03: Feedback Signal Volume -- Cold Start and Sparse Labels

**Severity**: Medium
**Likelihood**: High
**Impact**: Retraining never triggers, or triggers with too few labels to produce meaningful improvements

The classifier needs 20 labeled signals to trigger retraining. Each signal requires an auto-extracted entry to receive a helpful/unhelpful vote, correction, or deprecation. In early project phases (features 1-10), auto-extraction may produce few entries, and those entries may receive sparse feedback. The retraining threshold may not be reached until feature 20+, significantly later than the "first meaningful retraining after ~5 features" timeline in the product vision.

**Mitigation**: (1) Feature outcome signals provide bulk labels -- one outcome can generate weak labels for multiple entries, accelerating reservoir fill. (2) The threshold is configurable via `LearnConfig.classifier_retrain_threshold` -- can be lowered if empirical data shows signal volume is too low. (3) The convention scorer has a lower threshold (5 evaluations), providing an earlier signal that the pipeline is working. (4) Shadow evaluation ground truth backfill contributes additional labeled data retroactively.

---

## SR-04: Fire-and-Forget Training Race Conditions

**Severity**: Medium
**Likelihood**: Medium
**Impact**: Concurrent training steps corrupt model state or produce conflicting shadow versions

`spawn_blocking` training is non-blocking -- if two feedback signals arrive in quick succession and both exceed the threshold, two training tasks could run concurrently on the same model. Both would clone the same production model, train on different batches, and both try to install as shadow. The second write would overwrite the first shadow model, discarding its training.

**Mitigation**: (1) Use a per-model `AtomicBool` training lock -- `try_train_step` returns immediately if training is already in progress for that model. (2) The second training attempt's samples remain in the reservoir and are included in the next training step. (3) `ModelRegistry` write operations (`promote_shadow`, `rollback`) are single-threaded through the server's event loop, preventing state corruption.

---

## SR-05: Trust Source Filtering Gaps

**Severity**: Medium
**Likelihood**: Low
**Impact**: Training on feedback from agent-stored entries pollutes model quality

Feedback capture must filter strictly for entries with `trust_source` "auto" or "neural". If the trust_source check is missed in any handler path (e.g., a bulk deprecation that includes both agent-stored and auto-extracted entries), the training pipeline ingests labels for entries the models never classified, introducing noise.

**Mitigation**: (1) `FeedbackCapture` implements the trust_source filter centrally -- all handler hooks go through one function. (2) Unit test for each feedback signal type verifies that agent-stored entries (trust_source = "agent") do NOT generate training samples. (3) The `LabelGenerator` requires the entry's trust_source as an input parameter and returns `None` for non-qualifying entries.

---

## SR-06: Shadow Evaluation Ground Truth Ambiguity

**Severity**: Low
**Likelihood**: Medium
**Impact**: Accuracy metrics are misleading, promotion decisions based on incorrect ground truth

The `ground_truth` backfill writes the feedback-derived label to `shadow_evaluations` rows where `ground_truth IS NULL`. But ground truth is ambiguous for some signals: a helpful vote on an entry classified as "convention" confirms the classification, but an unhelpful vote could mean the classification was wrong OR the content was poor (right class, bad text). Similarly, deprecation could mean the extraction was wrong OR the knowledge became stale (right extraction at the time, obsolete now).

**Mitigation**: (1) Only strong signals backfill ground truth: category corrections (explicit re-classification) and consistent multi-vote patterns (e.g., 3+ unhelpful votes). (2) Weak signals (single votes, feature outcomes) contribute to training via the reservoir but do NOT backfill ground truth in shadow_evaluations. (3) The `per_class_accuracy` check uses ground_truth only when available; shadow evaluations without ground truth are excluded from accuracy calculations, not assumed correct.

---

## SR-07: Convention Follow/Deviate Detection Complexity

**Severity**: Low
**Likelihood**: Medium
**Impact**: Convention scorer receives no positive/negative labels from observation data, limiting its ability to improve

The SCOPE identifies "convention followed by all agents" and "convention deviated from successfully" as training signals, but the detection mechanism is undefined. Detecting whether a convention entry was followed requires matching observation patterns against convention descriptions -- a semantic matching problem that may be more complex than a simple maintenance tick query.

**Mitigation**: (1) Delegated to architect as Open Question #2. (2) The convention scorer can still improve from direct feedback signals (helpful/unhelpful votes on convention entries, deprecations). Observation-based follow/deviate detection is an enhancement, not a prerequisite. (3) If detection proves too complex, scope it out of crt-008 and track as a follow-up.

---

## Top 3 Risks for Architect Attention

1. **SR-02 (EWC Gradient Injection Correctness)**: High severity. The parameter ordering contract between `flat_parameters()`, `compute_gradients()`, and `EwcState` is the most critical correctness invariant. Architecture must establish explicit ordering guarantees.

2. **SR-03 (Feedback Signal Volume)**: High likelihood. The cold start timeline depends on auto-extraction volume from col-013, which depends on project activity patterns. Architecture should define fallback behavior when retraining thresholds are not met for extended periods.

3. **SR-04 (Fire-and-Forget Training Race Conditions)**: Medium severity/likelihood. The concurrency model for background training needs explicit design -- per-model training locks, shadow version conflict resolution, and interaction with the ModelRegistry state machine.
