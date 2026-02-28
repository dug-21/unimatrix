# Pseudocode: regularization (EWC++)

## Structs

```
struct EwcState {
    fisher: Array1<f32>,           // Diagonal Fisher information, length = 2 * d * r
    reference_params: Array1<f32>, // theta*, length = 2 * d * r
    alpha: f32,                    // Decay factor, default 0.95
    lambda: f32,                   // Penalty weight, default 0.5
    initialized: bool,            // False until first update
}
```

## Construction

```
fn EwcState::new(param_count: usize, alpha: f32, lambda: f32) -> Self:
    return EwcState {
        fisher: Array1::zeros(param_count),
        reference_params: Array1::zeros(param_count),
        alpha,
        lambda,
        initialized: false,
    }
```

## Penalty Computation

```
fn EwcState::penalty(&self, current_params: &[f32]) -> f32:
    if !self.initialized:
        return 0.0

    let current = ArrayView1::from(current_params)
    let diff = &current - &self.reference_params
    let weighted = &self.fisher * &diff * &diff
    return (self.lambda / 2.0) * weighted.sum()
```

## Gradient Contribution

```
fn EwcState::gradient_contribution(&self, current_params: &[f32]) -> Vec<f32>:
    // Returns dL_ewc/d(params) = lambda * F_i * (theta_i - theta*_i)
    if !self.initialized:
        return vec![0.0; current_params.len()]

    let current = ArrayView1::from(current_params)
    let diff = &current - &self.reference_params
    let grad = self.lambda * &self.fisher * &diff
    return grad.to_vec()
```

## Online Update (EWC++)

```
fn EwcState::update(&mut self, current_params: &[f32], grad_a: &Array2<f32>, grad_b: &Array2<f32>, config: &AdaptConfig):
    // Compute batch Fisher approximation: F_batch = grad^2
    let mut batch_fisher = Vec::with_capacity(current_params.len())
    for v in grad_a.iter().chain(grad_b.iter()):
        batch_fisher.push(v * v)
    let batch_fisher = Array1::from(batch_fisher)

    let current = Array1::from(current_params.to_vec())

    if !self.initialized:
        // First update: initialize directly
        self.fisher = batch_fisher
        self.reference_params = current
        self.initialized = true
    else:
        // Online EWC++ update:
        // F_new = alpha * F_old + (1 - alpha) * F_batch
        self.fisher = self.alpha * &self.fisher + (1.0 - self.alpha) * &batch_fisher

        // theta*_new = alpha * theta*_old + (1 - alpha) * theta_current
        self.reference_params = self.alpha * &self.reference_params + (1.0 - self.alpha) * &current
```

## Serialization Helpers

```
fn EwcState::to_vecs(&self) -> (Vec<f32>, Vec<f32>):
    return (self.fisher.to_vec(), self.reference_params.to_vec())

fn EwcState::from_vecs(fisher: Vec<f32>, reference: Vec<f32>, alpha: f32, lambda: f32) -> Self:
    let initialized = fisher.iter().any(|v| *v != 0.0)
    return EwcState {
        fisher: Array1::from(fisher),
        reference_params: Array1::from(reference),
        alpha,
        lambda,
        initialized,
    }
```
