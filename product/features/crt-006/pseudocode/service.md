# Pseudocode: service (AdaptationService)

## Structs

```
struct AdaptationService {
    lora: MicroLoRA,
    reservoir: RwLock<TrainingReservoir>,
    ewc: RwLock<EwcState>,
    prototypes: RwLock<PrototypeManager>,
    episodic: EpisodicAugmenter,
    config: AdaptConfig,
    generation: AtomicU64,
    total_steps: AtomicU64,
    save_counter: AtomicU64,  // For debounced saves
}
```

## Construction

```
fn AdaptationService::new(config: AdaptConfig) -> Self:
    let param_count = 2 * config.dimension as usize * config.rank as usize
    let lora_config = LoraConfig { rank: config.rank, dimension: config.dimension, scale: config.scale }

    return AdaptationService {
        lora: MicroLoRA::new(lora_config),
        reservoir: RwLock::new(TrainingReservoir::new(config.reservoir_capacity, 42)),
        ewc: RwLock::new(EwcState::new(param_count, config.ewc_alpha, config.ewc_lambda)),
        prototypes: RwLock::new(PrototypeManager::new(config.max_prototypes, config.min_prototype_entries, config.pull_strength, config.dimension as usize)),
        episodic: EpisodicAugmenter { max_boost: 0.02, min_affinity: 3 },
        config,
        generation: AtomicU64::new(0),
        total_steps: AtomicU64::new(0),
        save_counter: AtomicU64::new(0),
    }
```

## Adapt Embedding (Public API -- Hot Path)

```
fn AdaptationService::adapt_embedding(&self, raw: &[f32], category: Option<&str>, topic: Option<&str>) -> Vec<f32>:
    assert raw.len() == self.config.dimension as usize

    // Step 1: MicroLoRA forward pass
    let adapted = self.lora.forward(raw)

    // Step 2: Prototype soft pull
    let prototypes = self.prototypes.read()
    let adjusted = prototypes.apply_pull(&adapted, category, topic)

    // NOTE: L2 normalization is done by the CALLER (server), not here
    // This allows the server to normalize once after all adjustments
    return adjusted
```

## Record Training Pairs (Public API)

```
fn AdaptationService::record_training_pairs(&self, pairs: &[(u64, u64, u32)]):
    let mut reservoir = self.reservoir.write()
    reservoir.add(pairs)
```

## Try Train Step (Public API -- Fire-and-Forget)

```
fn AdaptationService::try_train_step(&self, embed_fn: &dyn Fn(u64) -> Option<Vec<f32>>):
    // Check if reservoir has enough pairs
    let reservoir_len = {
        let r = self.reservoir.read()
        r.len()
    };

    if reservoir_len < self.config.batch_size:
        return

    // Execute training step
    let mut reservoir = self.reservoir.write()
    let mut ewc = self.ewc.write()
    let mut prototypes = self.prototypes.write()
    let mut gen = self.generation.load(Ordering::SeqCst)

    let success = execute_training_step(
        &self.lora,
        &mut reservoir,
        &mut ewc,
        &mut prototypes,
        &self.config,
        embed_fn,
        &mut gen,
    )

    if success:
        self.generation.store(gen, Ordering::SeqCst)
        self.total_steps.fetch_add(1, Ordering::SeqCst)
        self.save_counter.fetch_add(1, Ordering::SeqCst)
```

## Persistence (Public API)

```
fn AdaptationService::save_state(&self, dir: &Path) -> Result<()>:
    let state = snapshot_state(
        &self.lora,
        &self.ewc.read(),
        &self.prototypes.read(),
        &self.config,
        self.generation.load(Ordering::SeqCst),
        self.total_steps.load(Ordering::SeqCst),
    )
    save_state(&state, dir)

fn AdaptationService::load_state(&self, dir: &Path) -> Result<()>:
    let state = match load_state(dir)?:
        Some(s) -> s
        None -> return Ok(())  // No state to load, keep defaults

    // Validate dimension and rank match
    if state.dimension != self.config.dimension || state.rank != self.config.rank:
        log_warning("Adaptation state dimension/rank mismatch, starting fresh")
        return Ok(())

    let mut ewc = self.ewc.write()
    let mut prototypes = self.prototypes.write()
    let (gen, steps) = restore_state(state, &self.lora, &mut ewc, &mut prototypes)

    self.generation.store(gen, Ordering::SeqCst)
    self.total_steps.store(steps, Ordering::SeqCst)
    return Ok(())

fn AdaptationService::training_generation(&self) -> u64:
    return self.generation.load(Ordering::SeqCst)

fn AdaptationService::should_save(&self) -> bool:
    // Debounce: save every 10 training steps
    return self.save_counter.load(Ordering::SeqCst) >= 10

fn AdaptationService::reset_save_counter(&self):
    self.save_counter.store(0, Ordering::SeqCst)
```
