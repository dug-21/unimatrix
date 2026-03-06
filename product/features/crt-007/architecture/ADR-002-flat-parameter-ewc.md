## ADR-002: Flat Vec<f32> Parameter Interface for EwcState

### Context

EwcState (EWC++ regularization) currently lives in unimatrix-adapt and takes `Array2<f32>` gradient matrices shaped to MicroLoRA's (d, r) and (r, d) weight matrices. The shared version must work with both MicroLoRA (ndarray matrices) and burn models (Tensor parameters). These are incompatible types.

Options:
- **a) Generic over tensor type**: `EwcState<T: TensorLike>` -- overengineered for diagonal Fisher which only needs element-wise operations.
- **b) Flat `Vec<f32>` interface**: Both consumers flatten their parameters to `Vec<f32>`. EwcState operates on flat vectors. Consumers convert at their boundary.
- **c) ndarray throughout**: Force burn consumers to convert to ndarray. Adds ndarray dependency to burn model code.

### Decision

Use flat `Vec<f32>` for the shared EwcState parameter interface. The current `update(params, grad_a, grad_b)` method (which takes two separate Array2 matrices) is replaced with `update_from_flat(params: &[f32], grad_squared: &[f32])` which takes pre-flattened parameter vectors and pre-computed squared gradients (the batch Fisher approximation).

unimatrix-adapt's `execute_training_step` flattens its grad_a/grad_b into a single `Vec<f32>` and squares each element before calling `update_from_flat`. burn model consumers (crt-008) extract parameters via burn's parameter API and compute squared gradients via autodiff.

### Consequences

- **Easier**: Any framework can consume EwcState. No template complexity. Simple, testable API.
- **Harder**: unimatrix-adapt must add ~5 lines to flatten and square gradients before calling update. Callers must ensure consistent parameter ordering across calls.
- **Risk**: Parameter ordering is a silent correctness requirement. Mitigated by: (1) both MicroLoRA and burn models use stable, deterministic parameter iteration order, (2) EwcState validates parameter count matches on each call.
