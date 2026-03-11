//! Training pipeline: InfoNCE contrastive loss, reservoir sampling, batch training.

use ndarray::{Array1, Array2};

use unimatrix_learn::reservoir::TrainingReservoir;

use crate::config::AdaptConfig;
use crate::lora::MicroLoRA;
use crate::prototypes::PrototypeManager;
use crate::regularization::EwcState;

/// A co-access training pair.
#[derive(Debug, Clone)]
pub struct TrainingPair {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub count: u32,
}

/// Compute InfoNCE contrastive loss with log-sum-exp stability.
///
/// Returns the average loss over the batch, or an error if NaN/Inf is detected.
pub fn infonce_loss(
    anchors: &[Array1<f32>],
    positives: &[Array1<f32>],
    temperature: f32,
) -> Result<f32, &'static str> {
    let batch_size = anchors.len();
    if batch_size == 0 {
        return Ok(0.0);
    }

    let mut total_loss = 0.0_f32;

    for i in 0..batch_size {
        // Positive similarity
        let pos_sim = dot(&anchors[i], &positives[i]) / temperature;

        // All similarities (positive + negatives)
        let mut all_sims = Vec::with_capacity(batch_size);
        all_sims.push(pos_sim);

        for (j, positive) in positives.iter().enumerate().take(batch_size) {
            if j != i {
                let neg_sim = dot(&anchors[i], positive) / temperature;
                all_sims.push(neg_sim);
            }
        }

        // Log-sum-exp for numerical stability
        let max_sim = all_sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let log_sum_exp = max_sim
            + all_sims
                .iter()
                .map(|s| (s - max_sim).exp())
                .sum::<f32>()
                .ln();

        let loss_i = -(pos_sim - log_sum_exp);

        if loss_i.is_nan() || loss_i.is_infinite() {
            return Err("NaN/Inf in InfoNCE loss");
        }

        total_loss += loss_i;
    }

    Ok(total_loss / batch_size as f32)
}

/// Compute InfoNCE gradients with respect to anchor embeddings.
///
/// Returns a gradient vector for each anchor.
pub fn infonce_gradients(
    anchors: &[Array1<f32>],
    positives: &[Array1<f32>],
    temperature: f32,
) -> Result<Vec<Array1<f32>>, &'static str> {
    let batch_size = anchors.len();
    if batch_size == 0 {
        return Ok(Vec::new());
    }
    let dim = anchors[0].len();
    let mut grads = vec![Array1::zeros(dim); batch_size];

    for i in 0..batch_size {
        // Compute softmax probabilities over all candidates
        let mut sims = Vec::new();
        let mut candidates: Vec<&Array1<f32>> = Vec::new();

        // Positive at index 0
        sims.push(dot(&anchors[i], &positives[i]) / temperature);
        candidates.push(&positives[i]);

        // Negatives
        for (j, positive) in positives.iter().enumerate().take(batch_size) {
            if j != i {
                sims.push(dot(&anchors[i], positive) / temperature);
                candidates.push(positive);
            }
        }

        // Softmax with log-sum-exp stability
        let max_sim = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let exp_sims: Vec<f32> = sims.iter().map(|s| (s - max_sim).exp()).collect();
        let sum_exp: f32 = exp_sims.iter().sum();
        if sum_exp.abs() < 1e-12 {
            return Err("NaN/Inf in InfoNCE gradients: zero denominator");
        }
        let probs: Vec<f32> = exp_sims.iter().map(|e| e / sum_exp).collect();

        // Gradient: (1/temperature) * (sum(prob_j * candidate_j) - positive_i)
        let mut weighted_sum = Array1::zeros(dim);
        for (prob, cand) in probs.iter().zip(candidates.iter()) {
            weighted_sum = weighted_sum + *prob * (*cand);
        }

        grads[i] = (1.0 / temperature) * (&weighted_sum - &positives[i]);
    }

    // Average over batch
    let batch_f = batch_size as f32;
    for g in &mut grads {
        *g = &*g / batch_f;
    }

    Ok(grads)
}

/// Execute a complete training step.
///
/// Returns `true` if the step was executed, `false` if skipped.
pub fn execute_training_step(
    lora: &MicroLoRA,
    reservoir: &mut TrainingReservoir<TrainingPair>,
    ewc: &mut EwcState,
    prototypes: &mut PrototypeManager,
    config: &AdaptConfig,
    embed_fn: &dyn Fn(u64) -> Option<Vec<f32>>,
    generation: &mut u64,
) -> bool {
    if reservoir.len() < config.batch_size {
        return false;
    }

    // 1. Sample batch
    let batch = reservoir.sample_batch(config.batch_size);

    // 2. Get raw embeddings for all entries in batch
    let mut anchors_raw = Vec::new();
    let mut positives_raw = Vec::new();

    for pair in &batch {
        match (embed_fn(pair.entry_id_a), embed_fn(pair.entry_id_b)) {
            (Some(raw_a), Some(raw_b)) => {
                anchors_raw.push(raw_a);
                positives_raw.push(raw_b);
            }
            _ => continue,
        }
    }

    if anchors_raw.is_empty() {
        return false;
    }

    // 3. Apply current MicroLoRA forward to all embeddings
    let anchors: Vec<Array1<f32>> = anchors_raw
        .iter()
        .map(|raw| Array1::from(lora.forward(raw)))
        .collect();
    let positives: Vec<Array1<f32>> = positives_raw
        .iter()
        .map(|raw| Array1::from(lora.forward(raw)))
        .collect();

    // 4. Compute InfoNCE loss
    let _loss = match infonce_loss(&anchors, &positives, config.temperature) {
        Ok(l) => l,
        Err(_) => return false,
    };

    // 5. Compute InfoNCE gradients wrt anchor embeddings
    let anchor_grads = match infonce_gradients(&anchors, &positives, config.temperature) {
        Ok(g) => g,
        Err(_) => return false,
    };

    // 6. Backpropagate through MicroLoRA to get weight gradients
    let d = config.dimension as usize;
    let r = config.rank as usize;
    let mut total_grad_a = Array2::zeros((d, r));
    let mut total_grad_b = Array2::zeros((r, d));

    for (i, grad_out) in anchor_grads.iter().enumerate() {
        let grad_out_slice: Vec<f32> = grad_out.iter().cloned().collect();
        let (ga, gb) = lora.backward(&anchors_raw[i], &grad_out_slice);
        total_grad_a = total_grad_a + ga;
        total_grad_b = total_grad_b + gb;
    }

    // 7. Add EWC gradient contribution
    let ewc_grad = ewc.gradient_contribution(&lora.parameters_flat());
    add_ewc_gradient(&mut total_grad_a, &mut total_grad_b, &ewc_grad, d, r);

    // 8. Update weights (LoRA+ learning rates)
    let lr_a = config.lr_a;
    let lr_b = config.lr_a * config.lr_ratio;
    lora.update_weights(&total_grad_a, &total_grad_b, lr_a, lr_b);

    // 9. Update EWC state
    ewc.update(&lora.parameters_flat(), &total_grad_a, &total_grad_b);

    // 10. Update prototypes (basic update without category/topic info from training)
    // Prototype updates happen during adapt_embedding calls, not during training
    let _ = prototypes;

    // 11. Increment generation
    *generation += 1;

    true
}

/// Add EWC gradient to the LoRA weight gradients.
fn add_ewc_gradient(
    grad_a: &mut Array2<f32>,
    grad_b: &mut Array2<f32>,
    ewc_grad: &[f32],
    d: usize,
    r: usize,
) {
    let a_size = d * r;
    // Add EWC gradients for A
    for (i, val) in ewc_grad.iter().take(a_size).enumerate() {
        let row = i / r;
        let col = i % r;
        grad_a[[row, col]] += val;
    }
    // Add EWC gradients for B
    for (i, val) in ewc_grad.iter().skip(a_size).enumerate() {
        let row = i / d;
        let col = i % d;
        if row < r && col < d {
            grad_b[[row, col]] += val;
        }
    }
}

/// Dot product of two Array1 vectors.
fn dot(a: &Array1<f32>, b: &Array1<f32>) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pairs(n: u64) -> Vec<TrainingPair> {
        (0..n)
            .map(|i| TrainingPair {
                entry_id_a: i,
                entry_id_b: i + 100,
                count: 1,
            })
            .collect()
    }

    // T-TRN-04: InfoNCE loss with extreme positive similarity
    #[test]
    fn infonce_extreme_positive_similarity() {
        let dim = 16;
        let mut anchors = Vec::new();
        let mut positives = Vec::new();
        for _ in 0..4 {
            let v: Vec<f32> = (0..dim).map(|i| (i as f32 * 0.1).sin()).collect();
            let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            let normalized: Vec<f32> = v.iter().map(|x| x / norm).collect();
            anchors.push(Array1::from(normalized.clone()));
            // Nearly identical positive
            let pos: Vec<f32> = normalized.iter().map(|x| x + 0.001).collect();
            let norm_p: f32 = pos.iter().map(|x| x * x).sum::<f32>().sqrt();
            let pos_norm: Vec<f32> = pos.iter().map(|x| x / norm_p).collect();
            positives.push(Array1::from(pos_norm));
        }

        let loss = infonce_loss(&anchors, &positives, 0.07).unwrap();
        assert!(loss.is_finite(), "loss should be finite: {loss}");
        assert!(loss >= 0.0, "loss should be non-negative: {loss}");
    }

    // T-TRN-05: InfoNCE loss with extreme dissimilarity
    #[test]
    fn infonce_extreme_dissimilarity() {
        let dim = 16;
        let mut anchors = Vec::new();
        let mut positives = Vec::new();
        // Create near-orthogonal pairs
        for i in 0..4 {
            let mut a = vec![0.0_f32; dim];
            a[i % dim] = 1.0;
            anchors.push(Array1::from(a));

            let mut p = vec![0.0_f32; dim];
            p[(i + dim / 2) % dim] = 1.0;
            positives.push(Array1::from(p));
        }

        let loss = infonce_loss(&anchors, &positives, 0.07).unwrap();
        assert!(loss.is_finite(), "loss should be finite: {loss}");
        assert!(loss > 0.0, "loss should be positive for dissimilar pairs");
    }

    // T-TRN-06: InfoNCE loss with mixed batch
    #[test]
    fn infonce_mixed_batch() {
        let dim = 16;
        let mut anchors = Vec::new();
        let mut positives = Vec::new();

        // High similarity pair
        let v: Vec<f32> = (0..dim).map(|i| (i as f32 * 0.1).sin()).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let v_norm: Vec<f32> = v.iter().map(|x| x / norm).collect();
        anchors.push(Array1::from(v_norm.clone()));
        positives.push(Array1::from(
            v_norm.iter().map(|x| x + 0.01).collect::<Vec<_>>(),
        ));

        // Low similarity pair
        let mut a = vec![0.0_f32; dim];
        a[0] = 1.0;
        let mut p = vec![0.0_f32; dim];
        p[dim - 1] = 1.0;
        anchors.push(Array1::from(a));
        positives.push(Array1::from(p));

        let loss = infonce_loss(&anchors, &positives, 0.07).unwrap();
        assert!(loss.is_finite(), "loss should be finite for mixed batch");
    }

    // T-TRN-12: InfoNCE loss with single pair
    #[test]
    fn infonce_single_pair() {
        let a = Array1::from(vec![1.0, 0.0, 0.0, 0.0]);
        let p = Array1::from(vec![1.0, 0.0, 0.0, 0.0]);
        let loss = infonce_loss(&[a], &[p], 0.07).unwrap();
        // Single pair: only one element in softmax, so loss = -log(1) = 0
        assert!(
            loss.abs() < 1e-6,
            "single pair loss should be ~0, got {loss}"
        );
    }

    // T-TRN-13: InfoNCE loss with empty batch
    #[test]
    fn infonce_empty_batch() {
        let loss = infonce_loss(&[], &[], 0.07).unwrap();
        assert_eq!(loss, 0.0);
    }

    // T-TRN-14: execute_training_step skips when insufficient pairs
    #[test]
    fn training_step_skips_insufficient_pairs() {
        let config = AdaptConfig {
            dimension: 16,
            rank: 4,
            batch_size: 32,
            ..AdaptConfig::default()
        };
        let d = config.dimension as usize;
        let r = config.rank as usize;
        let lora = MicroLoRA::new(crate::lora::LoraConfig {
            rank: config.rank,
            dimension: config.dimension,
            scale: config.scale,
        });
        let mut reservoir = TrainingReservoir::new(config.reservoir_capacity, 42);
        let mut ewc = EwcState::new(2 * d * r, config.ewc_alpha, config.ewc_lambda);
        let mut prototypes = PrototypeManager::new(
            config.max_prototypes,
            config.min_prototype_entries,
            config.pull_strength,
            d,
        );
        let mut generation = 0u64;

        // Add only 10 pairs (< batch_size=32)
        reservoir.add(&make_pairs(10));

        let initial_params = lora.parameters_flat();
        let result = execute_training_step(
            &lora,
            &mut reservoir,
            &mut ewc,
            &mut prototypes,
            &config,
            &|_| None,
            &mut generation,
        );
        assert!(!result, "should not train with insufficient pairs");
        assert_eq!(generation, 0);
        assert_eq!(lora.parameters_flat(), initial_params);
    }

    // T-TRN-15: execute_training_step succeeds with valid pairs
    #[test]
    fn training_step_succeeds() {
        let config = AdaptConfig {
            dimension: 16,
            rank: 4,
            batch_size: 8,
            ..AdaptConfig::default()
        };
        let d = config.dimension as usize;
        let r = config.rank as usize;
        let lora = MicroLoRA::new(crate::lora::LoraConfig {
            rank: config.rank,
            dimension: config.dimension,
            scale: config.scale,
        });
        let mut reservoir = TrainingReservoir::new(config.reservoir_capacity, 42);
        let mut ewc = EwcState::new(2 * d * r, config.ewc_alpha, config.ewc_lambda);
        let mut prototypes = PrototypeManager::new(
            config.max_prototypes,
            config.min_prototype_entries,
            config.pull_strength,
            d,
        );
        let mut generation = 0u64;

        // Add enough pairs
        reservoir.add(&make_pairs(40));

        // Provide embed_fn that returns deterministic vectors
        let initial_params = lora.parameters_flat();
        let result = execute_training_step(
            &lora,
            &mut reservoir,
            &mut ewc,
            &mut prototypes,
            &config,
            &|id| {
                let v: Vec<f32> = (0..16)
                    .map(|i| ((id as f32 + i as f32) * 0.1).sin())
                    .collect();
                Some(v)
            },
            &mut generation,
        );
        assert!(result, "training step should succeed");
        assert_eq!(generation, 1);
        // Weights should have changed
        assert_ne!(lora.parameters_flat(), initial_params);
    }

    // T-TRN-16: execute_training_step handles missing embeddings
    #[test]
    fn training_step_handles_missing_embeddings() {
        let config = AdaptConfig {
            dimension: 16,
            rank: 4,
            batch_size: 8,
            ..AdaptConfig::default()
        };
        let d = config.dimension as usize;
        let r = config.rank as usize;
        let lora = MicroLoRA::new(crate::lora::LoraConfig {
            rank: config.rank,
            dimension: config.dimension,
            scale: config.scale,
        });
        let mut reservoir = TrainingReservoir::new(config.reservoir_capacity, 42);
        let mut ewc = EwcState::new(2 * d * r, config.ewc_alpha, config.ewc_lambda);
        let mut prototypes = PrototypeManager::new(
            config.max_prototypes,
            config.min_prototype_entries,
            config.pull_strength,
            d,
        );
        let mut generation = 0u64;

        reservoir.add(&make_pairs(40));

        let result = execute_training_step(
            &lora,
            &mut reservoir,
            &mut ewc,
            &mut prototypes,
            &config,
            &|_| None, // All embeddings fail
            &mut generation,
        );
        assert!(!result, "should not train when all embeddings fail");
        assert_eq!(generation, 0);
    }
}
