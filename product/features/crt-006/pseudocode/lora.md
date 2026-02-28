# Pseudocode: lora (MicroLoRA Engine)

## Structs

```
struct LoraConfig {
    rank: u8,          // 2-16
    dimension: u16,    // 384
    scale: f32,        // default 1.0
}

struct MicroLoRA {
    config: LoraConfig,
    // Weights behind RwLock for concurrent read, atomic swap for write
    weights: RwLock<LoraWeights>,
}

struct LoraWeights {
    a: Array2<f32>,       // dimension x rank (down-projection)
    b: Array2<f32>,       // rank x dimension (up-projection)
    buf_down: Array1<f32>, // rank (pre-allocated)
    buf_up: Array1<f32>,   // dimension (pre-allocated)
}
```

## Construction

```
fn MicroLoRA::new(config: LoraConfig) -> Self:
    let d = config.dimension as usize
    let r = config.rank as usize

    // Xavier normal for A: std = sqrt(2 / (d + r))
    let std_a = sqrt(2.0 / (d + r) as f32)
    let a = Array2::random((d, r), Normal(0.0, std_a))

    // Near-zero for B: scale 1e-4
    let b = Array2::random((r, d), Normal(0.0, 1e-4))

    let buf_down = Array1::zeros(r)
    let buf_up = Array1::zeros(d)

    return MicroLoRA { config, weights: RwLock::new(LoraWeights { a, b, buf_down, buf_up }) }
```

## Forward Pass

```
fn MicroLoRA::forward(&self, input: &[f32]) -> Vec<f32>:
    // input is 384d f32 (raw ONNX embedding, already L2-normalized)
    assert input.len() == config.dimension

    let weights = self.weights.read()  // RwLock read
    let input_arr = ArrayView1::from(input)

    // Step 1: down-project: input (d,) @ A (d, r) -> down (r,)
    // Using pre-allocated buffer
    general_mat_vec_mul(1.0, &weights.a.t(), &input_arr, 0.0, &mut weights.buf_down)

    // Step 2: up-project: down (r,) @ B (r, d) -> up (d,)
    general_mat_vec_mul(1.0, &weights.b.t(), &weights.buf_down, 0.0, &mut weights.buf_up)

    // Step 3: residual connection: output = input + scale * up
    let mut output = input.to_vec()
    for i in 0..d:
        output[i] += self.config.scale * weights.buf_up[i]

    return output
    // NOTE: caller is responsible for L2 normalization
```

## Backward Pass

```
fn MicroLoRA::backward(&self, input: &[f32], grad_output: &[f32]) -> (Array2<f32>, Array2<f32>):
    // Computes gradients of loss wrt A and B
    // grad_output is dL/d(output) which equals dL/d(scale * input @ A @ B) = scale * dL/d(input @ A @ B)

    let weights = self.weights.read()
    let input_arr = ArrayView1::from(input)
    let grad_out = ArrayView1::from(grad_output)
    let d = self.config.dimension as usize
    let r = self.config.rank as usize
    let scale = self.config.scale

    // Forward intermediate: down = input @ A  (we recompute, cheap)
    let down = weights.a.t().dot(&input_arr)  // (r,)

    // dL/dB = scale * outer(down, grad_output)  -- (r, d)
    let grad_b = scale * outer(&down, &grad_out)

    // dL/d(down) = scale * B^T @ grad_output  -- (r,)
    let grad_down = scale * weights.b.dot(&grad_out)

    // dL/dA = outer(input, grad_down)  -- (d, r)
    let grad_a = outer(&input_arr, &grad_down)

    return (grad_a, grad_b)
```

## Weight Update (Atomic Swap)

```
fn MicroLoRA::update_weights(&self, grad_a: &Array2<f32>, grad_b: &Array2<f32>, lr_a: f32, lr_b: f32):
    // Read current weights
    let current = self.weights.read().clone()

    // Compute new weights (SGD)
    let new_a = &current.a - lr_a * grad_a
    let new_b = &current.b - lr_b * grad_b

    // NaN/Inf check
    if contains_nan_or_inf(&new_a) || contains_nan_or_inf(&new_b):
        log_warning("NaN/Inf detected in weight update, skipping")
        return

    // Atomic swap under write lock
    let mut weights = self.weights.write()
    weights.a = new_a
    weights.b = new_b
```

## Parameters Flat (for EWC)

```
fn MicroLoRA::parameters_flat(&self) -> Vec<f32>:
    let weights = self.weights.read()
    let mut flat = Vec::with_capacity(2 * d * r)
    flat.extend(weights.a.iter())
    flat.extend(weights.b.iter())
    return flat
```

## Utility

```
fn contains_nan_or_inf(arr: &Array2<f32>) -> bool:
    arr.iter().any(|v| v.is_nan() || v.is_infinite())

fn outer(a: &ArrayView1<f32>, b: &ArrayView1<f32>) -> Array2<f32>:
    // Outer product: (n,) x (m,) -> (n, m)
    let n = a.len()
    let m = b.len()
    let mut result = Array2::zeros((n, m))
    for i in 0..n:
        for j in 0..m:
            result[[i, j]] = a[i] * b[j]
    return result
```
