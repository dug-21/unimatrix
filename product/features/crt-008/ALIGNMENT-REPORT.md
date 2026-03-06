# Vision Alignment Report: crt-008 Continuous Self-Retraining

## Alignment Assessment

### V-01: Self-Learning Vision
**Vision statement**: "The system gets better with every feature delivered."
**crt-008 alignment**: PASS

crt-008 is the feature that makes this statement literally true. Before crt-008, models are static (hand-tuned baselines). After crt-008, every helpful vote, correction, deprecation, and feature outcome feeds back into training. The feedback-to-label pipeline converts 9 utilization event types into training signals. The timeline (first retraining after ~5 features, well-calibrated after ~20, deeply domain-adapted after ~50) directly implements the vision's self-learning timeline.

### V-02: Zero External Dependency
**Vision statement**: "Self-contained embedded engine with zero cloud dependency."
**crt-008 alignment**: PASS

All training runs on CPU via ndarray (already in workspace). No new crate dependencies. No API calls. Training completes in <5s for classifier, <2s for scorer. The system retrains itself from its own usage data without any external service.

### V-03: Trustworthy and Auditable
**Vision statement**: "What agents remember is trustworthy, correctable, and auditable."
**crt-008 alignment**: PASS

Shadow mode validation ensures retrained models prove themselves before influencing the knowledge base. Auto-rollback on accuracy regression (>5% overall, >10% per-class) prevents quality degradation. Ground truth backfill creates an auditable record of model accuracy. NaN/Inf detection catches numerical instability. Every training step is traceable to specific feedback signals.

### V-04: Invisible Delivery
**Vision statement**: "Knowledge reaches agents as ambient context, injected by hooks."
**crt-008 alignment**: PASS (no change)

crt-008 does not change the delivery mechanism. It improves the quality of what gets delivered by making the extraction models better at classifying and scoring signals. Better models -> better auto-extracted entries -> better knowledge delivered via hooks. The improvement is invisible to agents.

### V-05: Passive Knowledge Acquisition
**Vision statement**: "Knowledge base self-populates from agent behavioral signals."
**crt-008 alignment**: PASS

crt-008 closes the loop on the ASS-015 self-learning pipeline. col-013 extracts entries from observation data. crt-007 adds neural models that classify and score those extractions. crt-008 makes those neural models learn from feedback on their own output. The system improves its own knowledge extraction autonomously.

### V-06: EWC++ Regularization
**Vision statement (crt-008 description)**: "EWC++ regularization per model (reusing shared infra from crt-007)."
**crt-008 alignment**: PASS

EWC++ is integrated natively into the training loop via the compute_gradients/apply_gradients trait split. Gradient contribution from EwcState is added to task gradients before weight update. Reuses the existing EwcState from unimatrix-learn (extracted from unimatrix-adapt in crt-007). No approximation needed.

### V-07: Fire-and-Forget Background Tasks
**Vision statement (crt-008 description)**: "Models retrain incrementally via fire-and-forget background tasks."
**crt-008 alignment**: PASS

Training runs via `spawn_blocking`, non-blocking to the MCP server event loop. Per-model AtomicBool locks prevent concurrent training. Failed training discards the model silently. The server continues serving uninterrupted.

### V-08: Threshold-Triggered Retraining
**Vision statement**: "Classifier every 20 signals, Convention Scorer every 5 evaluations."
**crt-008 alignment**: PASS

Exact thresholds match the product vision. Both are configurable via LearnConfig for tuning.

### V-09: Auto-Rollback
**Vision statement**: "Auto-rollback on accuracy regression."
**crt-008 alignment**: PASS

Three rollback mechanisms: (1) overall accuracy drop >5%, (2) per-class accuracy drop >10%, (3) NaN/Inf weight detection. Exceeds the vision's requirement (which only specified accuracy regression).

## Variance Summary

| Check | Result | Notes |
|-------|--------|-------|
| V-01: Self-learning | PASS | Core value proposition of crt-008 |
| V-02: Zero dependency | PASS | ndarray only, zero new crates |
| V-03: Trustworthy | PASS | Shadow validation + rollback + ground truth audit |
| V-04: Invisible delivery | PASS | No change to delivery, improves extraction quality |
| V-05: Passive acquisition | PASS | Closes the self-learning loop |
| V-06: EWC++ | PASS | Native gradient injection via trait split |
| V-07: Fire-and-forget | PASS | spawn_blocking + AtomicBool lock |
| V-08: Thresholds | PASS | Matches vision exactly, configurable |
| V-09: Auto-rollback | PASS | Exceeds vision (3 mechanisms vs 1 required) |

**Overall**: 9 PASS, 0 WARN, 0 VARIANCE, 0 FAIL

## Variances Requiring Approval

None.

## Observations

1. **Convention follow/deviate detection** (ADR-005) uses approximate metadata matching rather than semantic analysis. This is acceptable for training labels but means the convention scorer may train more slowly from observation-based signals. Direct feedback (helpful/unhelpful votes on convention entries) remains the primary training signal.

2. **Ground truth backfill** (ADR-006) is conservative -- only category corrections and consistent multi-votes backfill. This means shadow evaluation accuracy metrics may be based on a small subset of evaluations. The design prioritizes accuracy metric reliability over volume.

3. **Phase 0 trait refactor** adds ~20 lines to crt-007's models. This is a backward-compatible change (default impl preserves existing behavior) but should ideally be included in crt-007's delivery to avoid a separate migration.
