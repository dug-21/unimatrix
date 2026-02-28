# Test Plan: training (Training Pipeline)

## Component Under Test

`crates/unimatrix-adapt/src/training.rs` -- TrainingReservoir, InfoNCE loss, InfoNCE gradients, execute_training_step.

## Risks Covered

- **R-02** (High): InfoNCE numerical instability
- **R-06** (Medium): Reservoir sampling bias
- **R-11** (Medium): Memory leak in training reservoir

## Test Cases

### T-TRN-01: Reservoir construction and basic add

**Purpose**: Verify reservoir initializes correctly and accepts pairs.
**Setup**: Create TrainingReservoir with capacity=10, seed=42.
**Method**: Add 5 pairs.
**Assertions**:
- `len()` == 5
- `total_seen` == 5
- All 5 pairs are retrievable via sample_batch(5)

### T-TRN-02: Reservoir capacity bound

**Purpose**: Verify reservoir does not exceed capacity (R-11).
**Setup**: Create TrainingReservoir with capacity=10.
**Method**: Add 100 pairs.
**Assertions**:
- `len()` == 10 (never exceeds capacity)
- `total_seen` == 100
- Memory usage bounded (pairs.len() <= capacity)

### T-TRN-03: Reservoir sample_batch returns correct size

**Purpose**: Verify batch sampling returns requested size.
**Setup**: Create reservoir with capacity=100, add 50 pairs.
**Method**: sample_batch(32), sample_batch(50), sample_batch(100).
**Assertions**:
- sample_batch(32) returns 32 pairs
- sample_batch(50) returns 50 pairs
- sample_batch(100) returns 50 pairs (capped at len)

### T-TRN-04: InfoNCE loss with extreme positive similarity

**Purpose**: Verify no NaN with near-identical vectors (R-02).
**Setup**: Create 4 anchor-positive pairs where each pair has cosine similarity > 0.99 (nearly identical vectors). Temperature = 0.07, so sim/tau > 14.
**Assertions**:
- Loss is finite (not NaN, not Inf)
- Loss is >= 0.0
- Log-sum-exp trick prevents overflow

### T-TRN-05: InfoNCE loss with extreme dissimilarity

**Purpose**: Verify no NaN with near-orthogonal vectors (R-02).
**Setup**: Create 4 pairs where anchors and positives are near-orthogonal (sim ~ 0.0).
**Assertions**:
- Loss is finite
- Loss > 0 (should be near ln(batch_size) for random-like inputs)

### T-TRN-06: InfoNCE loss with mixed batch

**Purpose**: Verify stability with mixed similarity levels (R-02).
**Setup**: Create batch of 8 pairs: 2 highly similar (sim > 0.95), 2 moderately similar (sim ~ 0.5), 2 dissimilar (sim < 0.1), 2 negative similarity.
**Assertions**:
- Loss is finite
- No NaN in loss
- Loss is positive

### T-TRN-07: InfoNCE loss with NaN input

**Purpose**: Verify NaN propagation is caught (R-02).
**Setup**: Create batch with one anchor containing NaN values.
**Assertions**:
- infonce_loss returns Err (not a NaN result)

### T-TRN-08: Reservoir sampling uniformity (R-06)

**Purpose**: Verify uniform sampling over all observed pairs.
**Setup**: Create reservoir with capacity=100. Add 1000 pairs, each with a unique ID pair.
**Method**: Sample 100 batches of 10 pairs each. Count how often each stored pair is sampled.
**Assertions**:
- Each stored pair is sampled at least once (with high probability)
- Chi-squared test on sample frequencies: p-value > 0.01 (fail to reject uniform null)
- No pair is sampled more than 3x the expected rate

### T-TRN-09: Reservoir with skewed input (R-06)

**Purpose**: Verify reservoir does not over-represent early pairs.
**Setup**: Create reservoir with capacity=50. Add 500 pairs in order.
**Assertions**:
- Pairs from the last 250 additions are represented proportionally (~50% of reservoir)
- Pairs from the first 250 additions are also represented (~50%)
- Reservoir sampling gives equal probability to all positions in the stream

### T-TRN-10: Reservoir at capacity with continued adds (R-11)

**Purpose**: Verify no memory growth beyond capacity.
**Setup**: Create reservoir with capacity=100.
**Method**: Add 10,000 pairs one at a time.
**Assertions**:
- `len()` never exceeds 100
- `total_seen` == 10,000
- No panic or allocation failure

### T-TRN-11: InfoNCE gradient correctness

**Purpose**: Verify gradient computation matches finite difference approximation.
**Setup**: Create 4 anchor-positive pairs at dimension 16 (small for efficiency).
**Method**:
- Compute analytical gradients via infonce_gradients
- For each anchor element: perturb by epsilon, recompute loss, compute numerical gradient
**Assertions**:
- Max absolute difference between analytical and numerical < 1e-3
- This validates the softmax-based gradient derivation

### T-TRN-12: InfoNCE loss with single pair

**Purpose**: Verify degenerate batch (size 1) is handled.
**Setup**: Create 1 anchor-positive pair (no negatives).
**Assertions**:
- Loss == 0.0 (only one element in softmax denominator == positive, so -log(1) = 0)

### T-TRN-13: InfoNCE loss with empty batch

**Purpose**: Verify empty batch returns zero.
**Setup**: Empty anchor and positive arrays.
**Assertions**:
- Loss == 0.0
- No panic

### T-TRN-14: execute_training_step skips when insufficient pairs

**Purpose**: Verify training does not trigger below batch_size threshold.
**Setup**: Create MicroLoRA + reservoir with 10 pairs, batch_size=32.
**Assertions**:
- `execute_training_step` returns false
- Weights unchanged
- Generation unchanged

### T-TRN-15: execute_training_step succeeds with valid pairs

**Purpose**: Verify complete training step executes.
**Setup**: Create MicroLoRA (rank 4, dim 16 for speed) + reservoir with 40 pairs + EWC + prototypes. Provide embed_fn that returns random vectors.
**Assertions**:
- `execute_training_step` returns true
- Generation incremented by 1
- Weights changed (not identical to before)

### T-TRN-16: execute_training_step handles missing embeddings

**Purpose**: Verify graceful handling when embed_fn returns None.
**Setup**: Create reservoir with 40 pairs. Provide embed_fn that returns None for all entries.
**Assertions**:
- `execute_training_step` returns false (no valid pairs)
- Weights unchanged

## Edge Cases

| Case | Test | Expected |
|------|------|----------|
| EC-02 Single entry | T-TRN-12 | Single pair produces 0 loss |
| EC-03 Fewer pairs than batch | T-TRN-14 | Training does not trigger |
| EC-10 Identical pairs | T-TRN-08 variant | Reservoir handles duplicates |

## Total: 16 unit tests
