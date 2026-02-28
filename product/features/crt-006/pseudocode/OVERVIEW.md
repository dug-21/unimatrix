# Pseudocode Overview: crt-006 Adaptive Embedding

## Component Interaction

```
AdaptationService (service.rs)
  |
  |-- owns MicroLoRA (lora.rs)
  |-- owns TrainingReservoir + InfoNCE (training.rs)
  |-- owns EwcState (regularization.rs)
  |-- owns PrototypeManager (prototypes.rs)
  |-- owns EpisodicAugmenter (episodic.rs)
  |-- delegates persistence to AdaptationState (persistence.rs)
  |-- configured by AdaptConfig (config.rs)
```

## Data Flow

### Forward Pass (adapt_embedding)
1. Input: raw 384d f32 from ONNX
2. MicroLoRA: `output = input + scale * (input @ A @ B)`
3. PrototypeManager: soft pull toward nearest prototype (if category/topic provided)
4. Return: adapted 384d f32 (caller normalizes with l2_normalize)

### Training Step (try_train_step)
1. Sample batch of pairs from TrainingReservoir
2. For each pair: get raw embeddings via embed_fn callback, apply current MicroLoRA forward
3. Compute InfoNCE loss with within-batch negatives
4. Compute EWC penalty from EwcState
5. Compute gradients via MicroLoRA backward
6. Validate gradients (NaN/Inf check)
7. Update weights atomically (RwLock write)
8. Update EwcState (Fisher + reference params)
9. Update PrototypeManager centroids
10. Increment training_generation
11. Debounced save

## Shared Types

```rust
// config.rs
pub struct AdaptConfig { rank, dimension, scale, lr_a, lr_ratio, temperature, batch_size, reservoir_capacity, ewc_alpha, ewc_lambda, max_prototypes, min_prototype_entries, pull_strength }

// persistence.rs
pub struct AdaptationState { version, rank, dimension, scale, weights_a, weights_b, fisher_diagonal, reference_params, prototypes, training_generation, total_training_steps, config }

// training.rs
pub struct TrainingPair { entry_id_a, entry_id_b, count }
```

## Crate Structure

```
crates/unimatrix-adapt/
  src/
    lib.rs          -- #![forbid(unsafe_code)], module declarations, re-exports
    config.rs       -- AdaptConfig with defaults
    lora.rs         -- MicroLoRA forward/backward
    training.rs     -- TrainingReservoir, InfoNCE, batch step
    regularization.rs -- EwcState, Fisher update, penalty
    prototypes.rs   -- PrototypeManager, soft pull, LRU eviction
    episodic.rs     -- EpisodicAugmenter, score adjustment
    persistence.rs  -- AdaptationState, save/load
    service.rs      -- AdaptationService, public API
```

## Server Integration Points

- `server.rs`: Add `adapt_service: Arc<AdaptationService>` field
- `tools.rs` context_store: insert `adapt_service.adapt_embedding()` between embed and vector insert
- `tools.rs` context_search: insert `adapt_service.adapt_embedding()` between embed and vector search
- `main.rs` or startup: load adaptation state alongside HNSW
- `main.rs` or shutdown: save adaptation state alongside HNSW
- Co-access recording: add `adapt_service.record_training_pairs()` + `adapt_service.try_train_step()`
