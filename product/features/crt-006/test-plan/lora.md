# Test Plan: lora (MicroLoRA Engine)

## Component Under Test

`crates/unimatrix-adapt/src/lora.rs` -- MicroLoRA struct, forward pass, backward pass, weight update, initialization.

## Risks Covered

- **R-01** (Critical): Gradient computation error
- **R-05** (High): Concurrent read/write race (partial -- weight update atomicity)
- **R-09** (Medium): Forward pass latency
- **R-12** (Low): Cold-start near-identity

## Test Cases

### T-LOR-01: Construction at various ranks

**Purpose**: Verify MicroLoRA constructs correctly for all valid ranks.
**Setup**: Create MicroLoRA with config at rank 2, 4, 8, 16, dimension 384.
**Assertions**:
- Weight A has shape (384, rank)
- Weight B has shape (rank, 384)
- No NaN or Inf in either matrix
- Pre-allocated buffers have correct dimensions

### T-LOR-02: Forward pass output dimension

**Purpose**: Verify forward pass preserves dimension and produces finite output.
**Setup**: Create MicroLoRA at rank 4. Generate random 384d input.
**Assertions**:
- Output length is 384
- No NaN or Inf in output
- Output differs from input (non-zero B, even if small)

### T-LOR-03: Near-identity at initialization

**Purpose**: Verify fresh MicroLoRA produces near-identity output (R-12, EC-01).
**Setup**: Create MicroLoRA at default config. Generate 10 random 384d inputs.
**Assertions**:
- For each input: cosine_similarity(input, output) > 0.99
- For each input: L2 distance between input and output < 0.05
- This validates that near-zero B initialization gives near-identity behavior

### T-LOR-04: Gradient correctness via finite differences

**Purpose**: Validate analytical gradients match numerical approximation (R-01).
**Setup**: Create MicroLoRA at rank 4. Pick a fixed input and a fixed grad_output vector.
**Method**:
- Compute analytical gradients via `backward(input, grad_output)` -> (grad_A, grad_B)
- For each parameter in A and B:
  - Perturb parameter by +epsilon (1e-5)
  - Compute forward pass, dot product with grad_output direction
  - Perturb parameter by -epsilon
  - Compute forward pass, dot product with grad_output direction
  - Numerical gradient = (f(p+eps) - f(p-eps)) / (2*eps)
- Compare analytical vs numerical for each parameter
**Assertions**:
- Max absolute difference < 1e-3 for all parameters
- Relative difference < 1e-2 for parameters where |grad| > 1e-6
- Test at rank 2, 4, 8, 16

### T-LOR-05: Convergence on synthetic data

**Purpose**: Verify that training with correct gradients reduces loss (R-01).
**Setup**: Create MicroLoRA at rank 4. Create 5 known similar pairs and 5 known dissimilar vectors. Use simple contrastive loss.
**Method**:
- Record loss before training
- Perform 50 gradient descent steps (forward, compute loss, backward, update)
- Record loss after training
**Assertions**:
- Final loss < initial loss
- Loss decreases monotonically (or at least does not increase by more than 5% in any step)

### T-LOR-06: NaN guard in weight update

**Purpose**: Verify that NaN/Inf gradients do not corrupt weights (R-02).
**Setup**: Create MicroLoRA at rank 4. Record initial weights.
**Method**:
- Call `update_weights` with gradient matrices containing NaN values
- Call `update_weights` with gradient matrices containing Inf values
**Assertions**:
- Weights unchanged after NaN gradient update
- Weights unchanged after Inf gradient update
- Forward pass still produces valid output

### T-LOR-07: Forward pass performance

**Purpose**: Verify forward pass is fast enough for hot path (R-09, NFR-01).
**Setup**: Create MicroLoRA at rank 8.
**Method**:
- Run 10,000 forward passes with random inputs
- Measure wall-clock time
**Assertions**:
- Average time per forward pass < 50 microseconds (conservative; target is < 10us)
- No allocations detected (if benchmark tooling supports it)

### T-LOR-08: Parameters flat round-trip

**Purpose**: Verify parameters_flat produces correct concatenation for EWC.
**Setup**: Create MicroLoRA at rank 4.
**Assertions**:
- `parameters_flat()` length == 2 * 384 * 4 == 3072
- First d*r values match A matrix flattened
- Last r*d values match B matrix flattened

### T-LOR-09: Weight update correctness (SGD step)

**Purpose**: Verify SGD update applies gradients correctly with LoRA+ rates.
**Setup**: Create MicroLoRA at rank 4. Record initial weights.
**Method**:
- Create known gradient matrices (all ones)
- Call `update_weights(grad_a, grad_b, lr_a=0.01, lr_b=0.16)`
**Assertions**:
- New A = old A - 0.01 * grad_a
- New B = old B - 0.16 * grad_b
- Values match element-wise within f32 epsilon

### T-LOR-10: Forward pass determinism

**Purpose**: Verify forward pass is deterministic (same input, same weights -> same output).
**Setup**: Create MicroLoRA at rank 4. Fix a specific input.
**Assertions**:
- Two calls to `forward(same_input)` produce identical output (bitwise)
- This is important for embedding consistency checks (R-10)

## Edge Cases

| Case | Test | Expected |
|------|------|----------|
| EC-01 Zero input vector | T-LOR-02 variant | Output is near-zero (scale * 0 @ A @ B = 0), no crash |
| EC-06 Unicode-agnostic | N/A | LoRA operates on f32 vectors, no text input |
| Rank 2 (minimum) | T-LOR-04 | Gradients correct at minimum rank |
| Rank 16 (maximum) | T-LOR-04 | Gradients correct at maximum rank |

## Total: 10 unit tests
