# Pseudocode: training (Training Pipeline)

## Structs

```
struct TrainingPair {
    entry_id_a: u64,
    entry_id_b: u64,
    count: u32,      // co-access frequency
}

struct TrainingReservoir {
    pairs: Vec<TrainingPair>,
    capacity: usize,         // default 512
    total_seen: u64,
    rng: StdRng,
}
```

## TrainingReservoir

```
fn TrainingReservoir::new(capacity: usize, seed: u64) -> Self:
    return TrainingReservoir { pairs: Vec::with_capacity(capacity), capacity, total_seen: 0, rng: StdRng::seed_from_u64(seed) }

fn TrainingReservoir::add(&mut self, pairs: &[(u64, u64, u32)]):
    for (id_a, id_b, count) in pairs:
        self.total_seen += 1
        let pair = TrainingPair { entry_id_a: id_a, entry_id_b: id_b, count }

        if self.pairs.len() < self.capacity:
            self.pairs.push(pair)
        else:
            // Reservoir sampling: replace with probability capacity / total_seen
            let j = self.rng.gen_range(0..self.total_seen)
            if j < self.capacity as u64:
                self.pairs[j as usize] = pair

fn TrainingReservoir::sample_batch(&self, batch_size: usize) -> Vec<&TrainingPair>:
    let actual_size = min(batch_size, self.pairs.len())
    // Sample without replacement
    let indices = rand::seq::index::sample(&mut self.rng.clone(), self.pairs.len(), actual_size)
    return indices.iter().map(|i| &self.pairs[i]).collect()

fn TrainingReservoir::len(&self) -> usize:
    return self.pairs.len()
```

## InfoNCE Loss

```
fn infonce_loss(
    anchors: &[Array1<f32>],     // adapted embeddings of entry_id_a in each pair
    positives: &[Array1<f32>],   // adapted embeddings of entry_id_b in each pair
    temperature: f32,
) -> Result<f32>:
    let batch_size = anchors.len()
    if batch_size == 0:
        return Ok(0.0)

    let mut total_loss = 0.0f32

    for i in 0..batch_size:
        // Positive similarity
        let pos_sim = dot(&anchors[i], &positives[i]) / temperature

        // All negatives: other positives in the batch
        // Collect all similarities for log-sum-exp
        let mut all_sims = Vec::with_capacity(batch_size)
        all_sims.push(pos_sim)  // positive at index 0

        for j in 0..batch_size:
            if j != i:
                let neg_sim = dot(&anchors[i], &positives[j]) / temperature
                all_sims.push(neg_sim)

        // Log-sum-exp trick for numerical stability
        let max_sim = all_sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        let log_sum_exp = max_sim + all_sims.iter()
            .map(|s| (s - max_sim).exp())
            .sum::<f32>()
            .ln()

        // Loss for this sample: -log(exp(pos_sim) / sum(exp(all_sims)))
        // = -(pos_sim - log_sum_exp)
        let loss_i = -(pos_sim - log_sum_exp)

        if loss_i.is_nan() || loss_i.is_infinite():
            return Err("NaN/Inf in InfoNCE loss")

        total_loss += loss_i

    return Ok(total_loss / batch_size as f32)
```

## InfoNCE Gradients

```
fn infonce_gradients(
    anchors: &[Array1<f32>],
    positives: &[Array1<f32>],
    temperature: f32,
) -> Result<Vec<Array1<f32>>>:
    // Returns gradient of loss wrt each anchor embedding
    let batch_size = anchors.len()
    let dim = anchors[0].len()
    let mut grads = vec![Array1::zeros(dim); batch_size]

    for i in 0..batch_size:
        // Compute softmax probabilities over all candidates
        let mut sims = Vec::new()
        let mut candidates = Vec::new()

        // Positive
        sims.push(dot(&anchors[i], &positives[i]) / temperature)
        candidates.push(&positives[i])

        // Negatives
        for j in 0..batch_size:
            if j != i:
                sims.push(dot(&anchors[i], &positives[j]) / temperature)
                candidates.push(&positives[j])

        // Softmax
        let max_sim = sims.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
        let exp_sims: Vec<f32> = sims.iter().map(|s| (s - max_sim).exp()).collect()
        let sum_exp: f32 = exp_sims.iter().sum()
        let probs: Vec<f32> = exp_sims.iter().map(|e| e / sum_exp).collect()

        // Gradient: (1/temperature) * (sum(prob_j * candidate_j) - positive_i)
        let mut weighted_sum = Array1::zeros(dim)
        for (j, (prob, cand)) in probs.iter().zip(candidates.iter()).enumerate():
            weighted_sum = weighted_sum + *prob * (*cand)

        grads[i] = (1.0 / temperature) * (&weighted_sum - &positives[i])

    // Average over batch
    for g in &mut grads:
        *g = g / batch_size as f32

    return Ok(grads)
```

## Training Step

```
fn execute_training_step(
    lora: &MicroLoRA,
    reservoir: &mut TrainingReservoir,
    ewc: &mut EwcState,
    prototypes: &mut PrototypeManager,
    config: &AdaptConfig,
    embed_fn: &dyn Fn(u64) -> Option<Vec<f32>>,
    generation: &mut u64,
) -> bool:
    // Returns true if training step was executed

    if reservoir.len() < config.batch_size:
        return false

    // 1. Sample batch
    let batch = reservoir.sample_batch(config.batch_size)

    // 2. Get raw embeddings for all entries in batch
    let mut anchors_raw = Vec::new()
    let mut positives_raw = Vec::new()
    let mut valid_pairs = Vec::new()

    for pair in batch:
        match (embed_fn(pair.entry_id_a), embed_fn(pair.entry_id_b)):
            (Some(raw_a), Some(raw_b)):
                anchors_raw.push(raw_a)
                positives_raw.push(raw_b)
                valid_pairs.push(pair)
            _ -> continue  // skip pairs where embedding lookup fails

    if valid_pairs.is_empty():
        return false

    // 3. Apply current MicroLoRA forward to all embeddings
    let anchors: Vec<Array1<f32>> = anchors_raw.iter()
        .map(|raw| Array1::from(lora.forward(raw)))
        .collect()
    let positives: Vec<Array1<f32>> = positives_raw.iter()
        .map(|raw| Array1::from(lora.forward(raw)))
        .collect()

    // 4. Compute InfoNCE loss
    let loss = match infonce_loss(&anchors, &positives, config.temperature):
        Ok(l) -> l
        Err(_) -> return false  // NaN/Inf, abort

    // 5. Compute InfoNCE gradients wrt anchor embeddings
    let anchor_grads = match infonce_gradients(&anchors, &positives, config.temperature):
        Ok(g) -> g
        Err(_) -> return false

    // 6. Backpropagate through MicroLoRA to get weight gradients
    let mut total_grad_a = Array2::zeros((config.dimension, config.rank))
    let mut total_grad_b = Array2::zeros((config.rank, config.dimension))

    for (i, grad_out) in anchor_grads.iter().enumerate():
        let (ga, gb) = lora.backward(&anchors_raw[i], grad_out.as_slice().unwrap())
        total_grad_a = total_grad_a + ga
        total_grad_b = total_grad_b + gb

    // 7. Add EWC gradient contribution
    let ewc_grad = ewc.gradient_contribution(lora.parameters_flat())
    // Split ewc_grad into A and B portions and add to total_grad_a, total_grad_b
    add_ewc_gradient(&mut total_grad_a, &mut total_grad_b, &ewc_grad, config.dimension, config.rank)

    // 8. Update weights (LoRA+ learning rates)
    let lr_a = config.lr_a
    let lr_b = config.lr_a * config.lr_ratio
    lora.update_weights(&total_grad_a, &total_grad_b, lr_a, lr_b)

    // 9. Update EWC state
    ewc.update(lora.parameters_flat(), &total_grad_a, &total_grad_b, config)

    // 10. Update prototypes (for entries touched in this batch)
    // prototypes.update() is called by the service layer with category/topic info

    // 11. Increment generation
    *generation += 1

    return true
```
