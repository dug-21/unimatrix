# Test Plan: regularization (EWC++)

## Component Under Test

`crates/unimatrix-adapt/src/regularization.rs` -- EwcState, penalty computation, gradient contribution, online EWC++ update.

## Risks Covered

- **R-07** (Medium): EWC++ numerical drift after many training steps

## Test Cases

### T-REG-01: Construction and initial state

**Purpose**: Verify EwcState initializes correctly.
**Setup**: Create EwcState with param_count=3072 (384 * 4 * 2), alpha=0.95, lambda=0.5.
**Assertions**:
- Fisher diagonal is all zeros, length 3072
- Reference params is all zeros, length 3072
- `initialized` is false
- `penalty(any_params)` returns 0.0 (not initialized)
- `gradient_contribution(any_params)` returns all zeros

### T-REG-02: Penalty computation with known values

**Purpose**: Verify penalty formula correctness.
**Setup**: Create EwcState, manually set fisher=[1.0, 2.0, 3.0], reference_params=[0.0, 0.0, 0.0], lambda=0.5, initialized=true.
**Method**: Compute penalty with current_params=[1.0, 1.0, 1.0].
**Assertions**:
- Penalty = (0.5/2) * (1.0*(1-0)^2 + 2.0*(1-0)^2 + 3.0*(1-0)^2) = 0.25 * (1+2+3) = 1.5
- Exact match within f32 epsilon

### T-REG-03: Gradient contribution with known values

**Purpose**: Verify gradient formula correctness.
**Setup**: Same as T-REG-02.
**Method**: Compute gradient_contribution with current_params=[1.0, 1.0, 1.0].
**Assertions**:
- Gradient = lambda * F * (theta - theta*) = 0.5 * [1,2,3] * [1,1,1] = [0.5, 1.0, 1.5]
- Exact match within f32 epsilon

### T-REG-04: Long-sequence Fisher stability (R-07)

**Purpose**: Verify Fisher diagonal does not degenerate after many updates.
**Setup**: Create EwcState with alpha=0.95, lambda=0.5, param_count=100.
**Method**: Perform 10,000 simulated updates with random batch Fisher values uniformly in [0, 1].
**Assertions**:
- No NaN or Inf in Fisher diagonal
- All Fisher values >= 0 (Fisher information is non-negative)
- Fisher values are in reasonable range [0, 2] (bounded by EMA convergence)
- Reference params are also NaN/Inf free

### T-REG-05: EWC regularization effectiveness (R-07)

**Purpose**: Verify EWC penalty actually constrains weight changes after training.
**Setup**: Create EwcState, perform 100 update cycles, then freeze reference.
**Method**:
- Compute penalty for params close to reference (small perturbation)
- Compute penalty for params far from reference (large perturbation)
**Assertions**:
- Large perturbation penalty > small perturbation penalty (constraint works)
- Penalty is proportional to distance from reference (not constant)
- After 1000 more updates, penalty still differentiates (no collapse to zero)

### T-REG-06: First update initializes state

**Purpose**: Verify first update call transitions from uninitialized to initialized.
**Setup**: Create fresh EwcState.
**Method**: Call update with some gradients and params.
**Assertions**:
- `initialized` becomes true after first update
- Fisher diagonal equals the first batch Fisher (no alpha blending on first call)
- Reference params equals the first current params

### T-REG-07: EWC++ update formula correctness

**Purpose**: Verify the exponential moving average formula.
**Setup**: Create EwcState, call update twice with known values.
**Method**:
- First update: batch_fisher=[1.0, 0.0], params=[1.0, 0.0]
- Second update: batch_fisher=[0.0, 1.0], params=[0.0, 1.0]
**Assertions**:
- After first: fisher=[1.0, 0.0], ref=[1.0, 0.0]
- After second: fisher=0.95*[1.0, 0.0] + 0.05*[0.0, 1.0] = [0.95, 0.05]
- After second: ref=0.95*[1.0, 0.0] + 0.05*[0.0, 1.0] = [0.95, 0.05]

### T-REG-08: Serialization round-trip

**Purpose**: Verify to_vecs and from_vecs preserve state.
**Setup**: Create EwcState, perform several updates to get non-trivial state.
**Method**: Call to_vecs(), then from_vecs() with the results.
**Assertions**:
- Penalty on same params produces identical result before and after round-trip
- Fisher and reference_params match element-wise

### T-REG-09: Zero gradient produces zero Fisher contribution

**Purpose**: Verify zero gradients do not corrupt Fisher.
**Setup**: Create EwcState (initialized).
**Method**: Call update with all-zero gradients.
**Assertions**:
- Fisher values decrease by factor alpha (blending with zero)
- No NaN or Inf

## Edge Cases

| Case | Test | Expected |
|------|------|----------|
| EC-01 Empty KB (no training) | T-REG-01 | Uninitialized state returns 0 penalty |
| EC-04 All pairs from one topic | T-REG-05 | EWC still constrains; Fisher from single topic |

## Total: 9 unit tests
