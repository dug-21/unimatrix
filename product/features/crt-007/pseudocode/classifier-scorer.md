# Pseudocode: classifier-scorer (Signal Classifier + Convention Scorer)

## Pattern: Hand-rolled ndarray MLP with forward/backward

Both models use ndarray Array2<f32> weight matrices and Array1<f32> bias vectors.
Forward pass is explicit matrix multiplication + element-wise activation.
Backward pass computes gradients layer-by-layer for train_step.

## Files

### crates/unimatrix-learn/src/models/classifier.rs

```pseudo
/// 5-class signal classifier: Convention, Pattern, Gap, Dead, Noise.
///
/// Topology: Linear(32,64) -> Sigmoid -> Linear(64,32) -> ReLU -> Linear(32,5) -> Softmax
pub struct SignalClassifier {
    // Layer 1: input -> hidden1
    w1: Array2<f32>,  // [32, 64]
    b1: Array1<f32>,  // [64]
    // Layer 2: hidden1 -> hidden2
    w2: Array2<f32>,  // [64, 32]
    b2: Array1<f32>,  // [32]
    // Layer 3: hidden2 -> output
    w3: Array2<f32>,  // [32, 5]
    b3: Array1<f32>,  // [5]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SignalCategory {
    Convention = 0,
    Pattern = 1,
    Gap = 2,
    Dead = 3,
    Noise = 4,
}

impl SignalCategory {
    pub fn from_index(i: usize) -> Self {
        match i {
            0 => Convention, 1 => Pattern, 2 => Gap, 3 => Dead, _ => Noise,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClassificationResult {
    pub category: SignalCategory,
    pub probabilities: [f32; 5],
    pub confidence: f32,  // max probability
}

impl SignalClassifier {
    pub fn new_with_baseline() -> Self {
        // Xavier/Glorot initialization for weights using deterministic seed
        let mut rng = StdRng::seed_from_u64(42);
        let w1 = xavier_init(&mut rng, 32, 64);
        let b1 = Array1::zeros(64);
        let w2 = xavier_init(&mut rng, 64, 32);
        let b2 = Array1::zeros(32);
        let w3 = xavier_init(&mut rng, 32, 5);
        // Baseline bias: Noise class (index 4) gets +2.0
        let mut b3 = Array1::zeros(5);
        b3[4] = 2.0;
        Self { w1, b1, w2, b2, w3, b3 }
    }

    pub fn classify(&self, digest: &SignalDigest) -> ClassificationResult {
        let output = self.forward(digest.as_slice());
        // output is post-softmax probabilities
        let mut probs = [0.0_f32; 5];
        for (i, v) in output.iter().enumerate().take(5) {
            probs[i] = *v;
        }
        let (max_idx, &max_val) = probs.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or((4, &0.0));
        ClassificationResult {
            category: SignalCategory::from_index(max_idx),
            probabilities: probs,
            confidence: max_val,
        }
    }

    /// Forward pass implementation
    fn forward_layers(&self, input: &Array1<f32>) -> (Array1<f32>, Array1<f32>, Array1<f32>, Array1<f32>) {
        // Layer 1: Linear + Sigmoid
        let z1 = self.w1.t().dot(input) + &self.b1;  // [64]
        let a1 = z1.mapv(sigmoid);

        // Layer 2: Linear + ReLU
        let z2 = self.w2.t().dot(&a1) + &self.b2;  // [32]
        let a2 = z2.mapv(relu);

        // Layer 3: Linear + Softmax
        let z3 = self.w3.t().dot(&a2) + &self.b3;  // [5]
        let a3 = softmax(&z3);

        (a1, a2, z2, a3)
    }
}

impl NeuralModel for SignalClassifier {
    fn forward(&self, input: &[f32]) -> Vec<f32> {
        let x = Array1::from(input.to_vec());
        let (_, _, _, output) = self.forward_layers(&x);
        output.to_vec()
    }

    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32 {
        let x = Array1::from(input.to_vec());
        let t = Array1::from(target.to_vec());
        let (a1, a2, z2, a3) = self.forward_layers(&x);

        // Cross-entropy loss
        let loss = -t.iter().zip(a3.iter())
            .map(|(ti, ai)| ti * ai.max(1e-7).ln())
            .sum::<f32>();

        // Backward pass (softmax + cross-entropy shortcut: da3 = a3 - t)
        let da3 = &a3 - &t;  // [5]

        // Layer 3 gradients
        let dw3 = a2.view().insert_axis(ndarray::Axis(1)).dot(&da3.view().insert_axis(ndarray::Axis(0)));  // [32,5]
        let db3 = da3.clone();

        // Backprop through layer 3
        let da2 = self.w3.dot(&da3);  // [32]

        // ReLU derivative
        let dz2 = da2 * z2.mapv(relu_derivative);  // [32]

        // Layer 2 gradients
        let dw2 = a1.view().insert_axis(ndarray::Axis(1)).dot(&dz2.view().insert_axis(ndarray::Axis(0)));  // [64,32]
        let db2 = dz2.clone();

        // Backprop through layer 2
        let da1 = self.w2.dot(&dz2);  // [64]

        // Sigmoid derivative
        let dz1 = &da1 * &a1.mapv(|a| a * (1.0 - a));  // [64]

        // Layer 1 gradients
        let dw1 = x.view().insert_axis(ndarray::Axis(1)).dot(&dz1.view().insert_axis(ndarray::Axis(0)));  // [32,64]
        let db1 = dz1;

        // Update weights (SGD)
        self.w1 = &self.w1 - &(lr * &dw1);
        self.b1 = &self.b1 - &(lr * &db1);
        self.w2 = &self.w2 - &(lr * &dw2);
        self.b2 = &self.b2 - &(lr * &db2);
        self.w3 = &self.w3 - &(lr * &dw3);
        self.b3 = &self.b3 - &(lr * &db3);

        loss
    }

    fn flat_parameters(&self) -> Vec<f32> {
        // Order: w1, b1, w2, b2, w3, b3
        let mut params = Vec::new();
        params.extend(self.w1.iter());
        params.extend(self.b1.iter());
        params.extend(self.w2.iter());
        params.extend(self.b2.iter());
        params.extend(self.w3.iter());
        params.extend(self.b3.iter());
        params
    }

    fn set_parameters(&mut self, params: &[f32]) {
        // Reverse of flat_parameters, slicing by expected sizes
        let mut offset = 0;
        // w1: 32*64
        let s = 32 * 64;
        self.w1 = Array2::from_shape_vec((32, 64), params[offset..offset+s].to_vec()).unwrap();
        offset += s;
        // b1: 64
        self.b1 = Array1::from(params[offset..offset+64].to_vec());
        offset += 64;
        // w2: 64*32
        let s = 64 * 32;
        self.w2 = Array2::from_shape_vec((64, 32), params[offset..offset+s].to_vec()).unwrap();
        offset += s;
        // b2: 32
        self.b2 = Array1::from(params[offset..offset+32].to_vec());
        offset += 32;
        // w3: 32*5
        let s = 32 * 5;
        self.w3 = Array2::from_shape_vec((32, 5), params[offset..offset+s].to_vec()).unwrap();
        offset += s;
        // b3: 5
        self.b3 = Array1::from(params[offset..offset+5].to_vec());
    }

    fn serialize(&self) -> Vec<u8> {
        bincode::serde::encode_to_vec(&self.flat_parameters(), bincode::config::standard()).unwrap()
    }

    fn deserialize(data: &[u8]) -> Result<Self, String> {
        let (params, _): (Vec<f32>, _) = bincode::serde::decode_from_slice(data, bincode::config::standard())
            .map_err(|e| format!("classifier deserialize: {e}"))?;
        let mut model = Self::new_with_baseline();
        model.set_parameters(&params);
        Ok(model)
    }
}
```

### Activation functions (in classifier.rs or a shared helpers module)

```pseudo
fn sigmoid(x: f32) -> f32 { 1.0 / (1.0 + (-x).exp()) }

fn relu(x: f32) -> f32 { x.max(0.0) }

fn relu_derivative(x: f32) -> f32 { if x > 0.0 { 1.0 } else { 0.0 } }

fn softmax(z: &Array1<f32>) -> Array1<f32> {
    let max = z.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let exp = z.mapv(|v| (v - max).exp());
    let sum = exp.sum();
    exp / sum
}

fn xavier_init(rng: &mut StdRng, fan_in: usize, fan_out: usize) -> Array2<f32> {
    let scale = (2.0 / (fan_in + fan_out) as f32).sqrt();
    Array2::from_shape_fn((fan_in, fan_out), |_| {
        use rand::Rng;
        rng.random::<f32>() * 2.0 * scale - scale
    })
}
```

### crates/unimatrix-learn/src/models/scorer.rs

```pseudo
/// Binary convention confidence scorer.
///
/// Topology: Linear(32,32) -> ReLU -> Linear(32,1) -> Sigmoid
pub struct ConventionScorer {
    w1: Array2<f32>,  // [32, 32]
    b1: Array1<f32>,  // [32]
    w2: Array2<f32>,  // [32, 1]
    b2: Array1<f32>,  // [1]
}

impl ConventionScorer {
    pub fn new_with_baseline() -> Self {
        let mut rng = StdRng::seed_from_u64(123);
        let w1 = xavier_init(&mut rng, 32, 32);
        let b1 = Array1::zeros(32);
        let w2 = xavier_init(&mut rng, 32, 1);
        // Baseline bias: -2.0 biases toward low scores
        let b2 = Array1::from(vec![-2.0]);
        Self { w1, b1, w2, b2 }
    }

    pub fn score(&self, digest: &SignalDigest) -> f32 {
        let output = self.forward(digest.as_slice());
        output[0]  // single sigmoid output in [0,1]
    }

    fn forward_layers(&self, input: &Array1<f32>) -> (Array1<f32>, Array1<f32>, Array1<f32>) {
        let z1 = self.w1.t().dot(input) + &self.b1;
        let a1 = z1.mapv(relu);
        let z2 = self.w2.t().dot(&a1) + &self.b2;
        let a2 = z2.mapv(sigmoid);
        (a1, z1, a2)
    }
}

impl NeuralModel for ConventionScorer {
    fn forward(&self, input: &[f32]) -> Vec<f32> {
        let x = Array1::from(input.to_vec());
        let (_, _, output) = self.forward_layers(&x);
        output.to_vec()
    }

    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32 {
        let x = Array1::from(input.to_vec());
        let t = target[0];
        let (a1, z1, a2) = self.forward_layers(&x);
        let y = a2[0];

        // Binary cross-entropy loss
        let loss = -(t * y.max(1e-7).ln() + (1.0 - t) * (1.0 - y).max(1e-7).ln());

        // Backward: sigmoid + BCE shortcut: da2 = y - t
        let da2 = Array1::from(vec![y - t]);

        // Layer 2 gradients
        let dw2 = a1.view().insert_axis(ndarray::Axis(1)).dot(&da2.view().insert_axis(ndarray::Axis(0)));
        let db2 = da2.clone();

        // Backprop through layer 2
        let da1 = self.w2.dot(&da2);

        // ReLU derivative
        let dz1 = da1 * z1.mapv(relu_derivative);

        // Layer 1 gradients
        let dw1 = x.view().insert_axis(ndarray::Axis(1)).dot(&dz1.view().insert_axis(ndarray::Axis(0)));
        let db1 = dz1;

        // Update (SGD)
        self.w1 = &self.w1 - &(lr * &dw1);
        self.b1 = &self.b1 - &(lr * &db1);
        self.w2 = &self.w2 - &(lr * &dw2);
        self.b2 = &self.b2 - &(lr * &db2);

        loss
    }

    fn flat_parameters(&self) -> Vec<f32> {
        let mut p = Vec::new();
        p.extend(self.w1.iter());
        p.extend(self.b1.iter());
        p.extend(self.w2.iter());
        p.extend(self.b2.iter());
        p
    }

    fn set_parameters(&mut self, params: &[f32]) {
        let mut offset = 0;
        let s = 32 * 32;
        self.w2 = ... // same slicing pattern as classifier
    }

    fn serialize(&self) -> Vec<u8> { /* same pattern as classifier */ }
    fn deserialize(data: &[u8]) -> Result<Self, String> { /* same pattern */ }
}
```

## Key Design Constraints

- All weight operations use ndarray Array2/Array1 -- no raw Vec math
- Xavier/Glorot init with deterministic seed for reproducibility
- Baseline biases are the only non-zero initial bias values
- Softmax uses log-sum-exp stability pattern (subtract max before exp)
- Train step returns loss for convergence monitoring (crt-008 scope)
