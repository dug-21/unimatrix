# ASS-015: Self-Retraining Neural Models — State of the Art

Research spike for Unimatrix's self-learning vision: bundling small specialized neural
models (classifiers, similarity models, text generators) that continuously retrain from
their own usage data.

**Date**: 2026-03-05
**Status**: Complete
**Scope**: Online/continual learning, self-training, retraining triggers, lightweight
CPU-based training, model versioning, feedback-loop architectures.

---

## Table of Contents

1. [Online Learning and Continual Learning for Small Models](#1-online-learning-and-continual-learning-for-small-models)
2. [Self-Training and Self-Improving Neural Networks](#2-self-training-and-self-improving-neural-networks)
3. [Retraining Schedules and Triggers](#3-retraining-schedules-and-triggers)
4. [Lightweight Retraining in Embedded/Edge Contexts](#4-lightweight-retraining-in-embeddededge-contexts)
5. [Production Patterns for Model Versioning and Rollback](#5-production-patterns-for-model-versioning-and-rollback)
6. [Feedback-Loop Training Architectures](#6-feedback-loop-training-architectures)
7. [Synthesis: Recommendations for Unimatrix](#7-synthesis-recommendations-for-unimatrix)

---

## 1. Online Learning and Continual Learning for Small Models

### 1.1 Problem Statement

When a deployed model receives new data, the naive approach — fine-tuning on the new
data — destroys performance on earlier data. This is **catastrophic forgetting**, the
central problem of continual learning. For Unimatrix's scenario (small models, CPU-only,
continuous feedback), we need techniques that incrementally update without full retraining
and without catastrophic forgetting.

### 1.2 Three Families of Continual Learning Methods

A 2025 systematic literature review of 81 online continual learning approaches
categorizes them into three families:

| Family | Mechanism | Fraction of Literature |
|--------|-----------|----------------------|
| **Replay-based** | Store or generate past examples; mix with new data during training | 62/81 (76%) |
| **Architecture-based** | Expand/isolate network capacity per task | ~12% |
| **Regularization-based** | Penalize changes to important weights | ~12% |

**Source**: [Online Continual Learning: A Systematic Literature Review (2025)](https://arxiv.org/html/2501.04897v1)

#### Replay-Based Methods (Most Practical for Unimatrix)

- **Experience Replay (ER)**: Maintain a fixed-size memory buffer of past examples.
  During training on new data, sample from the buffer and interleave with new examples.
  Reservoir sampling ensures uniform representation of all past data.
  - Even small buffers (1/200 of total data) show only minimal performance degradation.
  - **Adaptive Memory Replay** (CVPR 2024): Aligns replay scheduling with model update
    dynamics, particularly beneficial when memory capacity is constrained.
  - **Compressed Activation Replay**: Stores compressed intermediate representations
    rather than raw inputs, reducing memory 40-80% with minor performance loss.
  - **FOREVER** (2025): Uses Ebbinghaus forgetting-curve-inspired scheduling to
    determine replay frequency per example.

- **Generative Replay**: Train a small generative model to produce synthetic past
  examples instead of storing real ones. Trades storage for compute. Effective when
  buffer memory is extremely limited.

**Source**: [Adaptive Memory Replay for Continual Learning (CVPR 2024)](https://openaccess.thecvf.com/content/CVPR2024W/ELVM/papers/Smith_Adaptive_Memory_Replay_for_Continual_Learning_CVPRW_2024_paper.pdf), [Experience Replay for Continual Learning (NeurIPS)](http://papers.neurips.cc/paper/8327-experience-replay-for-continual-learning.pdf)

#### Regularization-Based Methods

- **Elastic Weight Consolidation (EWC)**: Computes Fisher information matrix after
  each task to identify which weights are important. Adds a quadratic penalty to the
  loss for modifying those weights during subsequent training. Simple to implement,
  moderate effectiveness.
  - Limitation: Fisher matrix grows with tasks, memory scales linearly.
  - **EVCL (2024)**: Elastic Variational Continual Learning combines EWC with
    variational inference for better uncertainty estimation.

- **Synaptic Intelligence (SI)**: Online variant of EWC that accumulates importance
  scores during training rather than computing them post-hoc. Lower overhead.

**Source**: [Overcoming catastrophic forgetting in neural networks (PNAS)](https://www.pnas.org/doi/10.1073/pnas.1611835114), [EVCL (2024)](https://arxiv.org/html/2406.15972v1)

#### Architecture-Based Methods

- **Progressive Neural Networks**: Add new columns for new tasks with lateral
  connections to previous columns. Previous columns are frozen. Zero forgetting but
  model size grows linearly with tasks.

- **PackNet**: Uses iterative pruning to free capacity within a fixed-size network for
  new tasks. After training on a task, prune unimportant weights and use the freed
  capacity for the next task. Fixed model size but finite capacity.

- **Nested Learning (Google, NeurIPS 2025)**: Treats a model as a set of nested
  optimization problems, each updating at different frequencies (a "continuum memory
  system"). Lower-frequency components preserve long-term knowledge; higher-frequency
  components adapt quickly. Demonstrated in the "Hope" architecture.
  - Particularly relevant for Unimatrix: the multi-timescale update concept maps
    naturally to a system where some knowledge (embeddings, similarity structure)
    should be stable while other knowledge (classifier heads, recency signals)
    should adapt quickly.

**Source**: [Nested Learning (Google Research, 2025)](https://research.google/blog/introducing-nested-learning-a-new-ml-paradigm-for-continual-learning/)

### 1.3 Online SGD for Production Classifiers

For the simplest case — incrementally updating a classifier on new labeled examples:

- **Mini-batch SGD** with small learning rate on incoming data is the baseline.
  Well-suited for streaming/online scenarios.
- Critical hyperparameters: learning rate (typically lower than initial training),
  regularization strength, batch size.
- Scikit-learn's `SGDClassifier.partial_fit()` demonstrates the pattern: call
  repeatedly with new mini-batches without resetting model state.
- For neural networks, the equivalent is running a few gradient steps on new data
  with a reduced learning rate, combined with replay from a buffer.

### 1.4 Recommendations for Unimatrix

| Model Type | Best Continual Learning Strategy |
|------------|----------------------------------|
| Classifier (category, intent) | Experience Replay + EWC hybrid |
| Similarity model (embeddings) | Nested Learning / multi-timescale updates |
| Seq2seq / generator | Replay-based with curriculum scheduling |

**Primary recommendation**: Experience Replay with reservoir sampling. It is the
dominant approach (76% of literature), simplest to implement, and works well with
small buffers. Combine with EWC regularization for critical models where forgetting
is expensive.

---

## 2. Self-Training and Self-Improving Neural Networks

### 2.1 Core Mechanism

Self-training is a semi-supervised technique where a model uses its own predictions on
unlabeled data as pseudo-labels, then retrains on the expanded labeled set. The cycle:

```
Train on labeled data -> Predict on unlabeled data -> Select high-confidence
predictions -> Add as pseudo-labels -> Retrain -> Repeat
```

A comprehensive 2024 survey covers the full landscape of self-training methods.

**Source**: [Self-training: A survey (2024)](https://www.sciencedirect.com/science/article/pii/S0925231224016758)

### 2.2 Confidence-Based Self-Labeling

The key decision in self-training is **which predictions to trust**:

- **Softmax threshold**: Accept predictions where max softmax probability exceeds a
  threshold (e.g., 0.95). Simple but neural networks are poorly calibrated — high
  softmax does not mean high accuracy.

- **Calibrated confidence**: Apply temperature scaling or Platt scaling to softmax
  outputs before thresholding. The **Top-versus-All (TvA)** method (2024) reformulates
  multiclass calibration into a single binary classification task on the max softmax
  score.

- **MC Dropout uncertainty**: Run multiple forward passes with dropout enabled,
  measure variance across predictions. Low variance = high confidence. More
  computationally expensive but better calibrated.

- **Atomic calibration (2024)**: For longer outputs, decompose into individual factual
  units and estimate confidence per unit using sampling consistency.

**Source**: [A Survey of Confidence Estimation and Calibration (NAACL 2024)](https://aclanthology.org/2024.naacl-long.366.pdf), [On Calibration of Modern Neural Networks](https://arxiv.org/abs/1706.04599)

### 2.3 Semantic Drift Problem

The primary risk of self-training: **error accumulation**. Incorrect pseudo-labels
reinforce model biases, causing performance to degrade over iterations. This is called
"confirmation bias" or "semantic drift."

Mitigations:

- **High confidence threshold**: Only add pseudo-labels with very high confidence
  (>0.95). Reduces volume but increases quality.
- **Mixup regularization**: Interpolate between pseudo-labeled and real-labeled
  examples during training.
- **Temporal ensembling**: Use exponential moving average of model predictions over
  multiple iterations rather than single-pass predictions.
- **Curriculum scheduling**: Start with easiest (highest confidence) examples, gradually
  lower threshold as model improves.

### 2.4 Curriculum Learning from Accumulated Data

Curriculum learning orders training data from easy to hard, inspired by human learning.
For a self-retraining system:

- **Difficulty scoring**: Use model loss as difficulty proxy. Low-loss examples are
  "easy," high-loss are "hard."
- **Self-paced learning**: Dynamically select examples whose current model loss is below
  an adaptively increasing threshold. The threshold rises as training progresses,
  introducing harder examples over time.
- **Anti-curriculum**: Some tasks benefit from training on hard examples first. The
  optimal strategy is task-dependent.

Practically, for Unimatrix: new feedback data arrives over time. Ordering by confidence
(easy first) during periodic retraining sessions is a simple curriculum strategy that
reduces early-stage noise.

**Source**: [Curriculum Learning: A Survey](https://arxiv.org/abs/2101.10382), [On The Power of Curriculum Learning in Training Deep Networks](https://arxiv.org/abs/1904.03626)

### 2.5 Knowledge Distillation for Continual Self-Improvement

- **Self-distillation**: The model acts as both teacher and student. After training on
  new data, the model's predictions on a held-out set become soft targets for the next
  round of training. This smooths the learning signal and reduces overfitting to noisy
  pseudo-labels.

- **SATCH (2024)**: Uses a small "assistant teacher" trained exclusively on the current
  task to provide task-specific guidance alongside the main model's self-distillation.

- **Self-Distillation Enables Continual Learning (2025)**: Demonstrated that
  self-distillation alone, without any replay buffer, can mitigate catastrophic
  forgetting by using the model's own soft predictions as a regularizer.

**Source**: [Self-Distillation Enables Continual Learning (2025)](https://arxiv.org/pdf/2601.19897), [SATCH (2024)](https://openreview.net/forum?id=CSAfU7J8Gw)

### 2.6 Recommendations for Unimatrix

For a self-improving loop:

1. **Accumulate usage data with confidence scores and user feedback signals.**
2. **Filter aggressively**: Only promote predictions to training data when confidence
   exceeds a calibrated threshold AND user feedback (if available) is positive.
3. **Apply curriculum ordering**: Sort accumulated data by confidence/difficulty for
   periodic retraining.
4. **Use self-distillation**: Before retraining, capture the current model's soft
   predictions as regularization targets.

---

## 3. Retraining Schedules and Triggers

### 3.1 Time-Driven vs Event-Driven Retraining

| Strategy | Mechanism | Pros | Cons |
|----------|-----------|------|------|
| **Periodic** (time-driven) | Retrain every N hours/days | Simple, predictable resource usage | May retrain when unnecessary or too late |
| **Trigger-based** (event-driven) | Retrain when drift detected or data threshold met | Responsive to actual need | Requires drift detection infrastructure |
| **Adaptive** | Combine: periodic baseline + event-triggered acceleration | Best of both; 9.3% avg accuracy improvement | Most complex to implement |

Research shows adaptive retraining yields 9.3% accuracy improvement vs 6.7% for
trigger-based and 4.1% for periodic.

**Source**: [Self-Healing ML Pipelines (2025)](https://www.preprints.org/manuscript/202510.2522), [Model Monitoring, Data Drift Detection, and Efficient Model Retraining (2025)](https://www.researchgate.net/publication/395703466_Model_Monitoring_Data_Drift_Detection_and_Efficient_Model_Retraining_A_Review)

### 3.2 Concept Drift Detection

Methods for detecting when a model needs retraining:

#### Statistical Tests on Input Distribution

- **Kolmogorov-Smirnov (KS) test**: Compares the distribution of incoming features
  against the training distribution. Non-parametric, works for any distribution.
- **Population Stability Index (PSI)**: Measures how much the distribution of a variable
  has shifted. PSI > 0.25 generally indicates significant drift.
- **Jensen-Shannon divergence**: Symmetric measure of distribution difference. Good for
  comparing embedding distributions.

#### Performance-Based Detection

- **DDM (Drift Detection Method)**: Monitors the model's error rate on a stream. If
  error rate increases beyond a threshold (mean + 3*std), trigger retraining.
- **EDDM (Early Drift Detection Method)**: Monitors the distance between classification
  errors rather than just error rate. Detects gradual drift earlier than DDM.
- **Page-Hinkley test**: Detects changes in the mean of a sequence (e.g., running
  accuracy). Low overhead, suitable for streaming.

#### Embedding-Space Drift

Particularly relevant for Unimatrix's similarity models:

- Monitor the centroid of incoming query embeddings. If it drifts beyond a threshold
  from the training centroid, the embedding space may no longer represent the data well.
- Track the average cosine similarity between incoming queries and the nearest training
  examples. A sustained drop indicates the model encounters out-of-distribution data.

### 3.3 Warm-Start vs Full Retraining

A critical design decision. Research from NeurIPS 2020 (Ash & Adams) revealed a
surprising finding: **warm-started neural networks generalize worse than cold-started
ones**, even when trained on identical data.

| Approach | Training Time | Generalization | When to Use |
|----------|--------------|----------------|-------------|
| **Cold start** (from scratch) | 100% | Best | Major drift, architecture change |
| **Warm start** (resume from checkpoint) | 30-60% | Degraded | Quick iteration, minor updates |
| **Shrink & Perturb** | 40-70% | Near cold-start | Recommended default |
| **DASH** (NeurIPS 2024) | 40-60% | Matches cold-start | Stationary settings |

#### Shrink & Perturb (Recommended)

Before warm-starting, apply:
```
w_new = alpha * w_old + (1 - alpha) * noise
```
Where `alpha ~ 0.5` and `noise ~ N(0, 0.01)`. This:
- Shrinks weights toward zero (reducing overfit to old data)
- Adds noise (restoring plasticity for new data)
- Achieves near-cold-start generalization at warm-start speed

#### DASH (NeurIPS 2024)

"Warm-Starting Neural Network Training in Stationary Settings without Loss of
Plasticity." Extends shrink-and-perturb with principled step-out from the previous
converging point, allowing better adaptation to new data.

**Source**: [On Warm-Starting Neural Network Training (NeurIPS 2020)](https://papers.neurips.cc/paper_files/paper/2020/file/288cd2567953f06e460a33951f55daaf-Paper.pdf), [DASH (NeurIPS 2024)](https://proceedings.neurips.cc/paper_files/paper/2024/file/4c5ce1fc8895076f49935951a630be5c-Paper-Conference.pdf)

### 3.4 Practical Retraining Triggers for Unimatrix

Given Unimatrix's context (knowledge engine with classifiers and similarity models):

| Trigger | Condition | Action |
|---------|-----------|--------|
| **Data volume** | N new feedback examples accumulated | Retrain with shrink-and-perturb |
| **Performance drop** | Running accuracy drops below threshold | Urgent retrain |
| **Embedding drift** | Centroid shift exceeds threshold | Retrain similarity model |
| **Time-based floor** | No retrain in past 7 days | Retrain regardless |
| **Schema change** | New categories/labels added | Cold-start with expanded architecture |

---

## 4. Lightweight Retraining in Embedded/Edge Contexts

### 4.1 Feasibility of CPU-Only Training

For small models (5-200MB parameter files), CPU training is viable:

- **Benchmark reference**: CPU training of MNIST-scale models completes in under
  1 minute. A simple CNN classifier on MNIST trains in ~18 minutes on CPU (TensorFlow)
  vs seconds on GPU.
- **Practical estimate for Unimatrix models**:
  - 5-10MB classifier (few-layer MLP or small CNN): 1-10 minutes per epoch on modern
    CPU, depending on dataset size.
  - 20-50MB similarity model (small transformer encoder): 10-60 minutes per epoch.
  - 100-200MB seq2seq model: 1-4 hours per epoch. May need to limit to fine-tuning
    final layers only.

- **Key optimization**: Use BLAS-accelerated matrix operations (OpenBLAS, MKL). Rust
  frameworks that leverage these see 3-5x speedup over naive implementations.

### 4.2 Rust ML Training Frameworks

Four major options exist for training neural networks in Rust:

#### Burn (Recommended for Unimatrix)

- **Repository**: [github.com/tracel-ai/burn](https://github.com/tracel-ai/burn)
- **Maturity**: Most actively developed, production-oriented.
- **Training support**: Full training loop with backpropagation, optimizers (Adam, SGD),
  learning rate schedulers.
- **Backends**: CPU (NdArray, Candle), GPU (CUDA via CubeCL, WebGPU, Vulkan, Metal).
- **CPU performance**: 67-75% latency reduction vs Python equivalents in benchmarks.
  Burn 0.20 (2025) added SIMD-optimized CPU execution.
- **Key feature**: Backend-agnostic — write training code once, run on any backend.
  Can start on CPU and migrate to GPU later without code changes.
- **ONNX import**: Can import ONNX models, enabling use of pre-trained models from
  PyTorch/TensorFlow as starting points.
- **Limitation**: Smaller ecosystem than PyTorch. Fewer pre-built model architectures.

**Source**: [Burn GitHub](https://github.com/tracel-ai/burn), [Burn 0.20 Release](https://www.phoronix.com/news/Burn-0.20-Released)

#### Candle (HuggingFace)

- **Repository**: [github.com/huggingface/candle](https://github.com/huggingface/candle)
- **Training support**: Has autograd/backpropagation. The `candle-nn` crate provides
  layers, activation functions, and optimizers. Training loops use
  `optimizer.backward_step(&loss)`.
- **Strength**: Pure Rust (no C++ dependencies), lightweight binary. Excellent for
  inference-heavy workloads with occasional fine-tuning.
- **Focus**: Primarily designed for inference ("serverless inference" is the stated
  goal). Training is supported but not the primary focus.
- **When to use**: If Unimatrix already uses Candle for inference (likely, given the
  existing embedding pipeline), adding training to the same framework avoids a second
  dependency.

**Source**: [Candle GitHub](https://github.com/huggingface/candle), [Neural Networks with Candle](https://pranitha.dev/posts/neural-networks-with-candle/)

#### tch-rs (PyTorch Bindings)

- **Repository**: Rust bindings around libtorch (PyTorch C++ backend).
- **Training support**: Full PyTorch training capabilities.
- **Limitation**: Requires libtorch shared library (~2GB). Heavy dependency, not
  suitable for lightweight/embedded deployment. Tied to PyTorch release cycle.
- **When to use**: Only if you need exact PyTorch compatibility or specific PyTorch
  model architectures.

#### dfdx

- **Repository**: Differentiable programming library for Rust.
- **Training support**: Functional/declarative style, full autograd.
- **Limitation**: Requires nightly Rust. Smaller community. API can be complex.
- **When to use**: If you want maximum type safety and compile-time dimension checking.

### 4.3 ONNX Runtime Training

The `ort` crate provides Rust bindings to ONNX Runtime, including training support:

- **Training API**: `ort::Trainer` provides on-device training/fine-tuning of ONNX
  models.
- **Workflow**: Export a PyTorch model to ONNX training artifacts (training model,
  eval model, optimizer model, checkpoint). Then train on-device using the ORT
  Training API.
- **CPU support**: Supported but currently single-threaded for some backends.
- **MultiLoRA (2025)**: ONNX Runtime added support for multiple LoRA adapters,
  enabling efficient personalization with minimal resource demands.

**Practical consideration**: ONNX Runtime Training requires generating training
artifacts from a PyTorch model first. This adds a build-time step but enables
training in pure ORT without a Python runtime.

**Source**: [ort crate](https://ort.pyke.io/), [ONNX Runtime Training](https://onnxruntime.ai/training), [On-Device Training Deep Dive](https://opensource.microsoft.com/blog/2023/07/05/on-device-training-with-onnx-runtime-a-deep-dive)

### 4.4 Framework Comparison for Unimatrix

| Criterion | Burn | Candle | ort (ONNX RT) | tch-rs |
|-----------|------|--------|---------------|--------|
| Pure Rust | Yes | Yes | No (C++ binding) | No (C++ binding) |
| Training support | Full | Basic | Full (via artifacts) | Full |
| CPU performance | Excellent (SIMD) | Good | Good | Good |
| Binary size | Small | Small | Medium (~50MB) | Large (~2GB) |
| Ecosystem | Growing | HuggingFace models | ONNX ecosystem | PyTorch ecosystem |
| Maturity for training | High | Medium | High | High |

**Recommendation**: Burn for new models, ort for fine-tuning existing ONNX models.
If Unimatrix already uses Candle for embedding inference (via `unimatrix-embed`),
consider Candle for training the embedding model and Burn for new classifier/generator
models.

### 4.5 Estimated Retraining Times on CPU

Rough estimates for a modern multi-core CPU (e.g., 8-core x86-64):

| Model | Size | Dataset | Time per Epoch | Full Retrain | Incremental Update |
|-------|------|---------|----------------|-------------|-------------------|
| MLP classifier | 5MB | 10K examples | 5-15 seconds | 1-3 minutes (10 epochs) | 5-15 seconds (1 epoch) |
| Small CNN | 10MB | 50K examples | 30-90 seconds | 5-15 minutes | 30-90 seconds |
| Tiny transformer encoder | 30MB | 10K examples | 1-3 minutes | 10-30 minutes | 1-3 minutes |
| Small seq2seq | 100MB | 10K examples | 5-15 minutes | 1-3 hours | 5-15 minutes |
| Sentence transformer (fine-tune) | 90MB | 5K pairs | 2-5 minutes | 20-50 minutes | 2-5 minutes |

These are feasible for background retraining. Even the largest model can complete an
incremental update in under 15 minutes, well within a reasonable retraining window.

---

## 5. Production Patterns for Model Versioning and Rollback

### 5.1 Model Versioning

Best practices from MLOps (2024-2025):

- **Version everything**: Weights, hyperparameters, training data hash, training code
  version, evaluation metrics.
- **Immutable artifacts**: Each model version is a frozen artifact. Never modify in
  place.
- **Naming convention**: `{model_name}_v{version}_{timestamp}_{metrics_hash}`
- **MLflow 3.0 (2025)**: Extended model registry to track not just weights but
  fine-tuned adapters, prompt templates, and evaluation runs.

For Unimatrix (embedded, no external MLflow):

```
models/
  classifier/
    v001_20260305_acc0.94.bin
    v002_20260312_acc0.91.bin   # <-- regression detected
    v003_20260312_acc0.95.bin   # <-- fixed
    current -> v003_20260312_acc0.95.bin  # symlink
    previous -> v001_20260305_acc0.94.bin  # rollback target
```

**Source**: [Model Versioning Infrastructure (2025)](https://introl.com/blog/model-versioning-infrastructure-mlops-artifact-management-guide-2025)

### 5.2 Shadow Mode Deployment

Run a new model version in parallel with production without serving its results:

1. **Production model** receives all traffic and serves responses.
2. **Shadow model** processes the same inputs but outputs are logged, not served.
3. **Compare metrics**: accuracy, latency, confidence distribution, edge cases.
4. **Promote** if shadow model meets or exceeds production metrics.

This is described as "the closest thing to safety in agent releases" — the only
reliable way to catch regressions before they hit production.

For Unimatrix, shadow mode is straightforward: run both models on each query, log
shadow model predictions alongside production predictions, compare after N queries.

**Source**: [Shadow Mode Deployment for ML Model Testing](https://mljourney.com/shadow-mode-deployment-for-ml-model-testing/), [Amazon SageMaker Shadow Testing](https://aws.amazon.com/blogs/machine-learning/minimize-the-production-impact-of-ml-model-updates-with-amazon-sagemaker-shadow-testing/)

### 5.3 A/B Testing Between Model Versions

- Hash user/session IDs for consistent routing to model variants.
- Track conversion/accuracy metrics per variant.
- Run until statistical significance is achieved (typically days to weeks depending on
  traffic volume).
- For Unimatrix's lower-traffic scenario: compare aggregated metrics rather than
  per-user A/B splits.

### 5.4 Automatic Rollback

Implement safety rails:

1. **Metric monitoring**: After promoting a new model, monitor key metrics (accuracy,
   confidence distribution, latency).
2. **Threshold-based rollback**: If accuracy drops below `previous_model_accuracy - margin`
   or if average confidence drops sharply, automatically revert.
3. **Implementation**: Keep the previous model loaded in memory (or on disk) with a
   symlink/pointer. Rollback = swap the pointer.

```
// Pseudocode for automatic rollback
if new_model.rolling_accuracy(window=100) < previous_model.final_accuracy - 0.05 {
    swap_to_previous_model();
    log_rollback_event();
    schedule_investigation();
}
```

Tools like Argo Rollouts and Flagger automate this pattern with progressive delivery
and automatic rollback on metric degradation.

**Source**: [Versioning & Rollbacks in Modern Agent Deployments](https://www.auxiliobits.com/blog/versioning-and-rollbacks-in-agent-deployments/)

### 5.5 Recommended Versioning Architecture for Unimatrix

```
ModelRegistry {
    models: HashMap<ModelName, ModelSlot>,
}

ModelSlot {
    production: LoadedModel,      // serving traffic
    shadow: Option<LoadedModel>,  // evaluation mode
    previous: PathBuf,            // rollback target
    history: Vec<ModelVersion>,   // all past versions
    metrics: RollingMetrics,      // live accuracy/confidence tracking
}
```

Lifecycle:
1. New model trains in background.
2. New model enters shadow mode (runs alongside production, results logged but not served).
3. After N queries with acceptable metrics, promote to production.
4. If metrics degrade, automatic rollback to previous.

---

## 6. Feedback-Loop Training Architectures

### 6.1 User Feedback as Training Signal

Unimatrix already tracks `helpful_count` and `unhelpful_count` per entry, plus
confidence scores. This existing infrastructure is the foundation for feedback-loop
training.

Types of feedback signal:

| Signal Type | Example in Unimatrix | Quality | Volume |
|-------------|---------------------|---------|--------|
| **Explicit binary** | "Was this helpful?" button | High | Low |
| **Explicit rating** | 1-5 star rating | Medium-High | Low |
| **Implicit positive** | User accepted suggestion, used result | Medium | High |
| **Implicit negative** | User ignored suggestion, reformulated query | Low-Medium | High |
| **Behavioral** | Time spent on result, follow-up queries | Low | Very High |

### 6.2 Handling Noisy and Sparse Feedback

Real-world feedback is noisy (users make mistakes, implicit signals are ambiguous) and
sparse (most interactions get no explicit feedback).

Mitigations:

- **Minimum vote threshold**: Only use feedback-derived labels after N votes (Unimatrix
  already uses min 5 votes for Wilson score helpfulness). This filters noise.
- **Confidence weighting**: Weight training examples by the confidence of the feedback
  signal. Explicit "helpful" = weight 1.0, implicit positive = weight 0.3.
- **Temporal smoothing**: Use exponential moving average of feedback over time rather
  than individual signals.
- **Disagreement filtering**: If explicit feedback contradicts implicit signals
  (user said "helpful" but never used the result), flag for investigation rather than
  training.

### 6.3 Confidence Calibration from Human Signals

The goal: the model's confidence scores should reflect actual accuracy. If the model
says 80% confident, it should be correct 80% of the time.

- **Post-hoc calibration**: After training, fit a calibration function (temperature
  scaling, Platt scaling) using held-out data with human labels.
- **Online calibration**: Continuously adjust calibration parameters as new feedback
  arrives. Track `predicted_confidence -> actual_accuracy` bins and fit an isotonic
  regression.
- **Wilson score interval** (already in Unimatrix): For binary feedback, Wilson score
  gives a calibrated confidence interval that naturally handles sparse data.

**Source**: [A Survey of Confidence Estimation and Calibration (NAACL 2024)](https://aclanthology.org/2024.naacl-long.366.pdf)

### 6.4 The Feedback-Loop Training Pipeline

Complete architecture for Unimatrix:

```
                    +-------------------+
                    |   User Queries    |
                    +--------+----------+
                             |
                    +--------v----------+
                    |  Production Model |---> Response
                    +--------+----------+
                             |
                    +--------v----------+
                    |  Feedback Capture |
                    |  (explicit/implicit)|
                    +--------+----------+
                             |
                    +--------v----------+
                    |  Feedback Buffer  |
                    |  (accumulate +    |
                    |   quality filter) |
                    +--------+----------+
                             |
              +--------------+--------------+
              |                             |
    +---------v---------+       +-----------v-----------+
    |  Drift Detection  |       |  Volume Threshold     |
    |  (KS, PSI, DDM)   |       |  (N new examples)     |
    +---------+---------+       +-----------+-----------+
              |                             |
              +--------+     +--------------+
                       |     |
              +--------v-----v----------+
              |   Retrain Decision      |
              |   (trigger evaluation)  |
              +--------+----------------+
                       |
              +--------v----------------+
              |   Training Pipeline     |
              |   1. Merge buffer +     |
              |      replay memory      |
              |   2. Curriculum order   |
              |   3. Shrink & Perturb   |
              |   4. Train (Burn/Candle)|
              |   5. Calibrate          |
              +--------+----------------+
                       |
              +--------v----------------+
              |   Shadow Evaluation     |
              |   (compare vs current)  |
              +--------+----------------+
                       |
              +--------v----------------+
              |   Promote / Rollback    |
              +-------------------------+
```

### 6.5 Direct Preference Optimization (DPO) for Pairwise Feedback

If Unimatrix collects pairwise preferences ("Result A was better than Result B"):

- **DPO (2024-2025)**: Became the standard for training from preferences without
  needing a separate reward model. Simpler and more stable than full RLHF.
- Applicable to re-ranking models: given user preference between two search results,
  directly optimize the similarity/ranking model to prefer the user's choice.

### 6.6 Reinforcement Learning from Verifiable Rewards (RLVR)

For tasks where correctness can be automatically verified:

- **RLVR (2025)**: Models are rewarded only when outputs pass objective checks.
  Emerged as a major paradigm shift.
- Applicable to Unimatrix's classification tasks: if downstream usage confirms or
  refutes a classification (e.g., a categorized entry is later re-categorized by a
  human), the correction signal becomes a verifiable reward.

**Source**: [Reinforcement Learning from Human Feedback (2025)](https://rlhfbook.com/book.pdf), [Interactive Training: Feedback-Driven Neural Network Optimization (2025)](https://arxiv.org/html/2510.02297v1)

---

## 7. Synthesis: Recommendations for Unimatrix

### 7.1 Architecture Overview

Unimatrix should implement a **three-tier self-retraining system**:

| Tier | Update Frequency | Mechanism | Models |
|------|-----------------|-----------|--------|
| **Online** | Per-query | Calibration adjustment, confidence recalibration | All models |
| **Incremental** | Per-N-examples or drift-triggered | Mini-batch SGD with replay buffer | Classifiers, rankers |
| **Full** | Weekly or on schema change | Shrink-and-perturb warm restart | Similarity models, generators |

### 7.2 Technology Stack

| Component | Recommendation | Rationale |
|-----------|---------------|-----------|
| **Training framework** | Burn (primary) + Candle (embedding fine-tune) | Pure Rust, CPU-optimized, ONNX import |
| **Continual learning** | Experience Replay + EWC | Dominant approach, proven at scale |
| **Warm restart** | Shrink & Perturb | Near-cold-start quality at warm-start speed |
| **Drift detection** | Page-Hinkley (performance) + JS divergence (distribution) | Low overhead, complementary signals |
| **Self-training** | Confidence-threshold pseudo-labeling with calibration | Leverages existing confidence system |
| **Feedback integration** | Wilson score filtering (existing) + DPO for preferences | Builds on existing infrastructure |
| **Model versioning** | In-process registry with symlink-based rollback | No external dependencies |
| **Safety** | Shadow mode evaluation before promotion | Prevents regressions |

### 7.3 Implementation Phases

**Phase 1: Infrastructure (Prerequisite)**
- Add model versioning to `unimatrix-core` (ModelRegistry, ModelSlot, version tracking).
- Implement feedback buffer: accumulate labeled examples from usage tracking.
- Add rolling metrics tracker (accuracy, confidence calibration).

**Phase 2: Incremental Classifier Retraining**
- Implement experience replay buffer with reservoir sampling.
- Add EWC regularization to classifier training loop.
- Implement drift detection (Page-Hinkley on classification accuracy).
- Trigger-based retraining: retrain when N new labeled examples OR drift detected.
- Shadow mode: run new classifier alongside production, promote after validation.

**Phase 3: Similarity Model Fine-Tuning**
- Implement embedding drift detection (JS divergence on query centroid).
- Contrastive fine-tuning: use positive/negative feedback pairs for triplet loss
  updates.
- Multi-timescale updates: fine-tune final layers frequently, full model rarely.

**Phase 4: Self-Training Loop**
- Implement pseudo-label generation from high-confidence predictions.
- Curriculum scheduling: order accumulated data by confidence for retraining.
- Self-distillation: use current model's soft predictions as regularization targets.
- Confidence calibration pipeline: online isotonic regression from feedback signals.

**Phase 5: Generative Model Adaptation**
- DPO training from pairwise preferences (if applicable).
- RLVR from verifiable outcomes.
- Full cold-restart capability for architecture changes.

### 7.4 Key Design Decisions to Make

| Decision | Options | Recommendation |
|----------|---------|---------------|
| Training framework | Burn vs Candle vs ort | Burn for training, existing embedding framework for inference |
| Replay buffer size | Fixed vs adaptive | Start fixed (1000 examples), make adaptive later |
| Retraining trigger | Time vs event vs hybrid | Hybrid: event-triggered with weekly floor |
| Warm restart strategy | Shrink-and-perturb vs DASH | Shrink-and-perturb (simpler, well-understood) |
| Shadow evaluation period | N queries vs time-based | N queries (100 minimum), with time cap |
| Pseudo-label threshold | Fixed vs adaptive | Start fixed (0.95), make adaptive based on calibration |

### 7.5 Risk Mitigations

| Risk | Mitigation |
|------|-----------|
| Catastrophic forgetting | Experience replay + EWC; always keep previous model version |
| Semantic drift (self-training) | High confidence threshold; minimum feedback votes; calibration |
| Regression on update | Shadow mode; automatic rollback; metrics monitoring |
| CPU training too slow | Limit model size; fine-tune only final layers; batch updates overnight |
| Noisy feedback labels | Wilson score filtering; confidence weighting; temporal smoothing |
| Storage growth | Prune old model versions; compress replay buffer; limit history depth |

### 7.6 What Already Exists in Unimatrix

Unimatrix already has foundational pieces that this system builds on:

- **Confidence system** (crt-002, crt-004, crt-005): Additive weighted composite with
  6 stored factors. Already tracks helpfulness via Wilson score.
- **Usage tracking** (crt-001): Fire-and-forget recording of helpful/unhelpful signals.
- **Co-access boosting** (crt-004): Behavioral signal (implicit feedback).
- **Contradiction detection** (crt-003): Anomaly detection in knowledge base.
- **Coherence gate / Lambda** (crt-005): Health metric that could serve as a drift
  detection proxy.
- **Embedding pipeline** (nxs-003): ONNX-based embedding inference already in Rust.
- **Outcome tracking** (col-001): Structured outcome recording per feature cycle.

The self-retraining system extends these existing subsystems rather than replacing them.

---

## Sources

### Online / Continual Learning
- [Online Continual Learning: A Systematic Literature Review (2025)](https://arxiv.org/html/2501.04897v1)
- [Recent Advances of Continual Learning in Computer Vision (2025)](https://ietresearch.onlinelibrary.wiley.com/doi/full/10.1049/cvi2.70013)
- [Hybrid Neural Networks for Continual Learning (Nature Communications, 2025)](https://www.nature.com/articles/s41467-025-56405-9)
- [Continual Learning of Large Language Models: A Comprehensive Survey (ACM, 2025)](https://dl.acm.org/doi/10.1145/3735633)
- [IBM: What is Continual Learning?](https://www.ibm.com/think/topics/continual-learning)

### Catastrophic Forgetting Mitigation
- [Overcoming Catastrophic Forgetting in Neural Networks (PNAS)](https://www.pnas.org/doi/10.1073/pnas.1611835114)
- [Mitigating Catastrophic Forgetting through Model Growth (2025)](https://arxiv.org/html/2509.01213)
- [Nested Learning (Google Research, NeurIPS 2025)](https://research.google/blog/introducing-nested-learning-a-new-ml-paradigm-for-continual-learning/)
- [EVCL: Elastic Variational Continual Learning (2024)](https://arxiv.org/html/2406.15972v1)
- [Adaptive Memory Replay for Continual Learning (CVPR 2024)](https://openaccess.thecvf.com/content/CVPR2024W/ELVM/papers/Smith_Adaptive_Memory_Replay_for_Continual_Learning_CVPRW_2024_paper.pdf)

### Self-Training
- [Self-Training: A Survey (2024)](https://www.sciencedirect.com/science/article/pii/S0925231224016758)
- [Neural Networks Against (and For) Self-Training (2024)](https://arxiv.org/abs/2401.00575)
- [Self-Distillation Enables Continual Learning (2025)](https://arxiv.org/pdf/2601.19897)
- [SATCH: Specialized Assistant Teacher Distillation (2024)](https://openreview.net/forum?id=CSAfU7J8Gw)

### Confidence Calibration
- [A Survey of Confidence Estimation and Calibration (NAACL 2024)](https://aclanthology.org/2024.naacl-long.366.pdf)
- [On Calibration of Modern Neural Networks (ICML 2017)](https://arxiv.org/abs/1706.04599)
- [Calibration in Deep Learning: A Survey (2023)](https://arxiv.org/pdf/2308.01222)

### Retraining Triggers and Drift Detection
- [Self-Healing ML Pipelines (2025)](https://www.preprints.org/manuscript/202510.2522)
- [Model Monitoring, Data Drift Detection, and Efficient Model Retraining (2025)](https://www.researchgate.net/publication/395703466_Model_Monitoring_Data_Drift_Detection_and_Efficient_Model_Retraining_A_Review)
- [Model Retraining upon Concept Drift Detection (2025)](https://www.mdpi.com/1999-5903/17/8/328)
- [Model Monitoring Drift Detection Retraining Guide (2025)](https://fxis.ai/edu/model-monitoring-drift-detection-retraining-guide/)

### Warm-Start and Retraining Strategy
- [On Warm-Starting Neural Network Training (NeurIPS 2020)](https://papers.neurips.cc/paper_files/paper/2020/file/288cd2567953f06e460a33951f55daaf-Paper.pdf)
- [DASH: Warm-Starting Without Loss of Plasticity (NeurIPS 2024)](https://proceedings.neurips.cc/paper_files/paper/2024/file/4c5ce1fc8895076f49935951a630be5c-Paper-Conference.pdf)
- [Step Out and Seek Around: Warm-Start with Incremental Data (2024)](https://arxiv.org/html/2406.04484v1)
- [Cost-Aware Retraining for Machine Learning (2024)](https://www.sciencedirect.com/science/article/pii/S0950705124002454)

### Rust ML Frameworks
- [Burn Framework](https://github.com/tracel-ai/burn) / [Burn 0.20 Release](https://www.phoronix.com/news/Burn-0.20-Released)
- [Candle (HuggingFace)](https://github.com/huggingface/candle)
- [Rust ML Framework Comparison 2025](https://markaicode.com/rust-machine-learning-framework-comparison-2025/)
- [Building Sentence Transformers in Rust](https://dev.to/mayu2008/building-sentence-transformers-in-rust-a-practical-guide-with-burn-onnx-runtime-and-candle-281k)

### ONNX Runtime Training
- [ort Crate (Rust ONNX Runtime)](https://ort.pyke.io/)
- [ONNX Runtime Training](https://onnxruntime.ai/training)
- [On-Device Training Deep Dive (Microsoft)](https://opensource.microsoft.com/blog/2023/07/05/on-device-training-with-onnx-runtime-a-deep-dive)
- [On-Device Training in Browser (2024)](https://opensource.microsoft.com/blog/2024/02/06/on-device-training-training-a-model-in-browser)

### Model Versioning and Deployment
- [Model Versioning Infrastructure (2025)](https://introl.com/blog/model-versioning-infrastructure-mlops-artifact-management-guide-2025)
- [Shadow Mode Deployment](https://mljourney.com/shadow-mode-deployment-for-ml-model-testing/)
- [Versioning & Rollbacks in Agent Deployments](https://www.auxiliobits.com/blog/versioning-and-rollbacks-in-agent-deployments/)

### Feedback-Loop Training
- [Reinforcement Learning from Human Feedback (2025)](https://rlhfbook.com/book.pdf)
- [Interactive Training: Feedback-Driven Neural Network Optimization (2025)](https://arxiv.org/html/2510.02297v1)
- [Curriculum Learning: A Survey](https://arxiv.org/abs/2101.10382)
