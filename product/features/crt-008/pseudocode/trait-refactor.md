# Pseudocode: trait-refactor (Wave 0)

## Purpose

Split `NeuralModel::train_step` into `compute_gradients` + `apply_gradients` with `train_step` as default impl. Enables EWC gradient injection between computation and weight update.

## Changes to models/traits.rs

```pseudo
trait NeuralModel: Send + Sync {
    fn forward(&self, input: &[f32]) -> Vec<f32>;

    // NEW: compute loss and flat gradient vector without updating weights
    fn compute_gradients(&self, input: &[f32], target: &[f32]) -> (f32, Vec<f32>);

    // NEW: apply a flat gradient vector as SGD weight update
    fn apply_gradients(&mut self, gradients: &[f32], lr: f32);

    // CHANGED: now a default impl calling compute_gradients + apply_gradients
    fn train_step(&mut self, input: &[f32], target: &[f32], lr: f32) -> f32 {
        let (loss, grads) = self.compute_gradients(input, target);
        self.apply_gradients(&grads, lr);
        loss
    }

    fn flat_parameters(&self) -> Vec<f32>;
    fn set_parameters(&mut self, params: &[f32]);
    fn serialize(&self) -> Vec<u8>;
    fn deserialize(data: &[u8]) -> Result<Self, String> where Self: Sized;
}
```

## Changes to models/classifier.rs

Extract from existing `train_step` (lines 137-193):

```pseudo
impl NeuralModel for SignalClassifier {
    fn compute_gradients(&self, input: &[f32], target: &[f32]) -> (f32, Vec<f32>) {
        let x = Array1::from(input.to_vec());
        let t = Array1::from(target.to_vec());
        let (a1, a2, z2, a3) = self.forward_layers(&x);

        // Cross-entropy loss (same as before, lines 142-147)
        let loss = -t.iter().zip(a3.iter())
            .map(|(ti, ai)| ti * ai.max(1e-7).ln())
            .sum::<f32>();

        // Backward pass (same as before, lines 149-183)
        let da3 = &a3 - &t;
        // ... layer 3, 2, 1 gradient computation ...
        // dw3, db3, dw2, db2, dw1, db1

        // Flatten gradients in canonical order: w1, b1, w2, b2, w3, b3
        let mut grads = Vec::with_capacity(self.flat_parameters().len());
        grads.extend(dw1.iter());
        grads.extend(db1.iter());
        grads.extend(dw2.iter());
        grads.extend(db2.iter());
        grads.extend(dw3.iter());
        grads.extend(db3.iter());

        (loss, grads)
    }

    fn apply_gradients(&mut self, gradients: &[f32], lr: f32) {
        // Parse flat gradient vector back into per-layer shapes
        let mut offset = 0;

        // w1: [32, 64] = 2048 elements
        let s = 32 * 64;
        let dw1 = Array2::from_shape_vec((32, 64), gradients[offset..offset+s].to_vec());
        offset += s;
        // b1: [64]
        let db1 = Array1::from(gradients[offset..offset+64].to_vec());
        offset += 64;
        // w2: [64, 32] = 2048
        let s = 64 * 32;
        let dw2 = Array2::from_shape_vec((64, 32), gradients[offset..offset+s].to_vec());
        offset += s;
        // b2: [32]
        let db2 = Array1::from(gradients[offset..offset+32].to_vec());
        offset += 32;
        // w3: [32, 5] = 160
        let s = 32 * 5;
        let dw3 = Array2::from_shape_vec((32, 5), gradients[offset..offset+s].to_vec());
        offset += s;
        // b3: [5]
        let db3 = Array1::from(gradients[offset..offset+5].to_vec());

        // SGD update (same as before, lines 186-191)
        self.w1 = &self.w1 - &(lr * &dw1);
        self.b1 = &self.b1 - &(lr * &db1);
        self.w2 = &self.w2 - &(lr * &dw2);
        self.b2 = &self.b2 - &(lr * &db2);
        self.w3 = &self.w3 - &(lr * &dw3);
        self.b3 = &self.b3 - &(lr * &db3);
    }

    // REMOVE explicit train_step -- use default impl from trait
}
```

## Changes to models/scorer.rs

Same pattern for ConventionScorer. Extract from `train_step` (lines 65-103):

```pseudo
impl NeuralModel for ConventionScorer {
    fn compute_gradients(&self, input: &[f32], target: &[f32]) -> (f32, Vec<f32>) {
        let x = Array1::from(input.to_vec());
        let t = target[0];
        let (a1, z1, a2) = self.forward_layers(&x);
        let y = a2[0];

        // BCE loss (same as before, line 72)
        let loss = -(t * y.max(1e-7).ln() + (1.0 - t) * (1.0 - y).max(1e-7).ln());

        // Backward pass (same as before, lines 75-95)
        // dw2, db2, dw1, db1

        // Flatten: w1, b1, w2, b2
        let mut grads = Vec::with_capacity(self.flat_parameters().len());
        grads.extend(dw1.iter());
        grads.extend(db1.iter());
        grads.extend(dw2.iter());
        grads.extend(db2.iter());

        (loss, grads)
    }

    fn apply_gradients(&mut self, gradients: &[f32], lr: f32) {
        let mut offset = 0;
        // w1: [32, 32] = 1024
        let s = 32 * 32;
        let dw1 = Array2::from_shape_vec((32, 32), gradients[offset..offset+s].to_vec());
        offset += s;
        // b1: [32]
        let db1 = Array1::from(gradients[offset..offset+32].to_vec());
        offset += 32;
        // w2: [32, 1] = 32
        let s = 32;
        let dw2 = Array2::from_shape_vec((32, 1), gradients[offset..offset+s].to_vec());
        offset += s;
        // b2: [1]
        let db2 = Array1::from(gradients[offset..offset+1].to_vec());

        // SGD update
        self.w1 = &self.w1 - &(lr * &dw1);
        self.b1 = &self.b1 - &(lr * &db1);
        self.w2 = &self.w2 - &(lr * &dw2);
        self.b2 = &self.b2 - &(lr * &db2);
    }

    // REMOVE explicit train_step -- use default impl from trait
}
```

## Invariant

`compute_gradients` gradient vector ordering MUST match `flat_parameters()` ordering (ADR-002):
- Classifier: w1(2048), b1(64), w2(2048), b2(32), w3(160), b3(5) = 4357 total
- Scorer: w1(1024), b1(32), w2(32), b2(1) = 1089 total
