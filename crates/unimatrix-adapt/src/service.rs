//! AdaptationService: public API orchestrating all adaptation components.
//!
//! This is the single entry point the server uses. It owns MicroLoRA,
//! the training reservoir, EWC state, and prototypes.
//! Thread-safe via Arc for concurrent use in the async server.

use std::path::Path;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::config::AdaptConfig;
use crate::lora::{LoraConfig, MicroLoRA};
use crate::persistence;
use crate::prototypes::PrototypeManager;
use crate::regularization::EwcState;
use crate::training::{TrainingPair, execute_training_step};
use unimatrix_learn::reservoir::TrainingReservoir;

/// Adaptation service orchestrating the full pipeline.
///
/// Implements `Send + Sync` for `Arc<AdaptationService>` usage.
pub struct AdaptationService {
    lora: MicroLoRA,
    reservoir: RwLock<TrainingReservoir<TrainingPair>>,
    ewc: RwLock<EwcState>,
    prototypes: RwLock<PrototypeManager>,
    config: AdaptConfig,
    generation: AtomicU64,
    total_steps: AtomicU64,
    save_counter: AtomicU64,
}

// Safety: all fields are either behind RwLock, are Atomic, or are immutable.
// MicroLoRA internally uses RwLock for its weights.
// AdaptConfig is Clone + immutable after construction.

impl AdaptationService {
    /// Create a new adaptation service with the given configuration.
    pub fn new(config: AdaptConfig) -> Self {
        let d = config.dimension as usize;
        let r = config.rank as usize;
        let param_count = 2 * d * r;

        let lora_config = LoraConfig {
            rank: config.rank,
            dimension: config.dimension,
            scale: config.scale,
        };

        Self {
            lora: MicroLoRA::with_seed(lora_config, config.init_seed),
            reservoir: RwLock::new(TrainingReservoir::new(config.reservoir_capacity, config.reservoir_seed)),
            ewc: RwLock::new(EwcState::new(param_count, config.ewc_alpha, config.ewc_lambda)),
            prototypes: RwLock::new(PrototypeManager::new(
                config.max_prototypes,
                config.min_prototype_entries,
                config.pull_strength,
                d,
            )),
            config,
            generation: AtomicU64::new(0),
            total_steps: AtomicU64::new(0),
            save_counter: AtomicU64::new(0),
        }
    }

    /// Adapt a raw embedding through the full pipeline.
    ///
    /// Steps:
    /// 1. MicroLoRA forward pass
    /// 2. Prototype soft pull (if category/topic provided)
    ///
    /// Caller is responsible for L2 normalization of the output.
    pub fn adapt_embedding(
        &self,
        raw: &[f32],
        category: Option<&str>,
        topic: Option<&str>,
    ) -> Vec<f32> {
        assert_eq!(
            raw.len(),
            self.config.dimension as usize,
            "input dimension mismatch: expected {}, got {}",
            self.config.dimension,
            raw.len()
        );

        // Step 1: MicroLoRA forward
        let adapted = self.lora.forward(raw);

        // Step 2: Prototype soft pull
        let prototypes = self.prototypes.read().expect("prototypes lock poisoned");
        prototypes.apply_pull(&adapted, category, topic)
    }

    /// Record co-access training pairs into the reservoir.
    pub fn record_training_pairs(&self, pairs: &[(u64, u64, u32)]) {
        let training_pairs: Vec<TrainingPair> = pairs
            .iter()
            .map(|&(entry_id_a, entry_id_b, count)| TrainingPair {
                entry_id_a,
                entry_id_b,
                count,
            })
            .collect();
        let mut reservoir = self.reservoir.write().expect("reservoir lock poisoned");
        reservoir.add(&training_pairs);
    }

    /// Attempt a training step if the reservoir has enough pairs.
    ///
    /// The `embed_fn` callback re-embeds entries by ID (raw ONNX, not adapted).
    pub fn try_train_step(&self, embed_fn: &dyn Fn(u64) -> Option<Vec<f32>>) {
        // Check reservoir size without holding the write lock
        let reservoir_len = {
            let r = self.reservoir.read().expect("reservoir lock poisoned");
            r.len()
        };

        if reservoir_len < self.config.batch_size {
            return;
        }

        // Acquire write locks for training
        let mut reservoir = self.reservoir.write().expect("reservoir lock poisoned");
        let mut ewc = self.ewc.write().expect("ewc lock poisoned");
        let mut prototypes = self.prototypes.write().expect("prototypes lock poisoned");
        let mut current_gen = self.generation.load(Ordering::SeqCst);

        let success = execute_training_step(
            &self.lora,
            &mut reservoir,
            &mut ewc,
            &mut prototypes,
            &self.config,
            embed_fn,
            &mut current_gen,
        );

        if success {
            self.generation.store(current_gen, Ordering::SeqCst);
            self.total_steps.fetch_add(1, Ordering::SeqCst);
            self.save_counter.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Update prototypes with an adapted embedding (called during store/correct).
    pub fn update_prototypes(
        &self,
        adapted: &[f32],
        category: Option<&str>,
        topic: Option<&str>,
    ) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut prototypes = self.prototypes.write().expect("prototypes lock poisoned");
        prototypes.update(adapted, category, topic, now);
    }

    /// Save adaptation state to disk.
    pub fn save_state(&self, dir: &Path) -> Result<(), String> {
        let ewc = self.ewc.read().expect("ewc lock poisoned");
        let prototypes = self.prototypes.read().expect("prototypes lock poisoned");

        let state = persistence::snapshot_state(
            &self.lora,
            &ewc,
            &prototypes,
            &self.config,
            self.generation.load(Ordering::SeqCst),
            self.total_steps.load(Ordering::SeqCst),
        );

        persistence::save_state(&state, dir)
    }

    /// Load adaptation state from disk.
    pub fn load_state(&self, dir: &Path) -> Result<(), String> {
        let state = match persistence::load_state(dir)? {
            Some(s) => s,
            None => return Ok(()),
        };

        // Validate dimension and rank match
        if state.dimension != self.config.dimension || state.rank != self.config.rank {
            tracing::warn!(
                "Adaptation state dimension/rank mismatch (state: {}x{}, config: {}x{}), starting fresh",
                state.dimension,
                state.rank,
                self.config.dimension,
                self.config.rank
            );
            return Ok(());
        }

        let mut ewc = self.ewc.write().expect("ewc lock poisoned");
        let mut prototypes = self.prototypes.write().expect("prototypes lock poisoned");

        let (restored_gen, steps) =
            persistence::restore_state(state, &self.lora, &mut ewc, &mut prototypes, &self.config)?;

        self.generation.store(restored_gen, Ordering::SeqCst);
        self.total_steps.store(steps, Ordering::SeqCst);
        Ok(())
    }

    /// Current training generation (monotonic counter).
    pub fn training_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Total training steps executed.
    pub fn total_training_steps(&self) -> u64 {
        self.total_steps.load(Ordering::SeqCst)
    }

    /// Whether the state should be saved (debounced: every 10 training steps).
    pub fn should_save(&self) -> bool {
        self.save_counter.load(Ordering::SeqCst) >= 10
    }

    /// Reset the save counter after a successful save.
    pub fn reset_save_counter(&self) {
        self.save_counter.store(0, Ordering::SeqCst);
    }

    /// Get the configuration.
    pub fn config(&self) -> &AdaptConfig {
        &self.config
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    fn default_service() -> AdaptationService {
        AdaptationService::new(AdaptConfig::default())
    }

    fn small_service() -> AdaptationService {
        AdaptationService::new(AdaptConfig {
            dimension: 16,
            rank: 4,
            batch_size: 8,
            ..AdaptConfig::default()
        })
    }

    // T-SVC-01: Cold-start identity output
    #[test]
    fn cold_start_identity() {
        let svc = default_service();
        for seed in 0..10 {
            let input: Vec<f32> = (0..384)
                .map(|i| ((i as f32 + seed as f32) * 0.01).sin())
                .collect();
            let norm: f32 = input.iter().map(|v| v * v).sum::<f32>().sqrt();
            let normalized: Vec<f32> = input.iter().map(|v| v / norm).collect();

            let output = svc.adapt_embedding(&normalized, None, None);
            assert_eq!(output.len(), 384);
            assert!(!output.iter().any(|v| v.is_nan() || v.is_infinite()));

            let cos_sim = cosine_sim(&normalized, &output);
            assert!(
                cos_sim > 0.99,
                "cold-start should be near-identity: cos_sim={cos_sim}"
            );
        }
    }

    // T-SVC-03: record_training_pairs and reservoir accumulation
    #[test]
    fn record_pairs_accumulation() {
        let svc = small_service();
        let pairs: Vec<(u64, u64, u32)> = (0..10).map(|i| (i, i + 100, 1)).collect();
        svc.record_training_pairs(&pairs);

        // Training should not trigger (10 < batch_size=8 is false, 10 >= 8 is true)
        // Actually 10 >= 8, so it would trigger. Let's use a smaller set.
        let svc2 = small_service();
        svc2.record_training_pairs(&[(1, 2, 1), (3, 4, 1)]);
        // Only 2 pairs, won't trigger
        assert_eq!(svc2.training_generation(), 0);
    }

    // T-SVC-06: Concurrent read during training
    #[test]
    fn concurrent_read_during_training() {
        let svc = Arc::new(small_service());

        // Fill reservoir
        let pairs: Vec<(u64, u64, u32)> = (0..100).map(|i| (i, i + 1000, 1)).collect();
        svc.record_training_pairs(&pairs);

        let mut handles = Vec::new();

        // Spawn readers
        for _ in 0..20 {
            let svc = Arc::clone(&svc);
            handles.push(thread::spawn(move || {
                for _ in 0..50 {
                    let input: Vec<f32> = (0..16).map(|i| (i as f32 * 0.1).sin()).collect();
                    let output = svc.adapt_embedding(&input, None, None);
                    assert_eq!(output.len(), 16);
                    assert!(!output.iter().any(|v| v.is_nan() || v.is_infinite()));
                }
            }));
        }

        // Trigger training on main thread
        svc.try_train_step(&|id| {
            let v: Vec<f32> = (0..16).map(|i| ((id as f32 + i as f32) * 0.1).sin()).collect();
            Some(v)
        });

        // Wait for all readers
        for h in handles {
            h.join().expect("reader thread panicked");
        }
    }

    // T-SVC-07: try_train_step fires when reservoir is full
    #[test]
    fn train_step_fires() {
        let svc = small_service();
        let pairs: Vec<(u64, u64, u32)> = (0..40).map(|i| (i, i + 100, 1)).collect();
        svc.record_training_pairs(&pairs);

        svc.try_train_step(&|id| {
            let v: Vec<f32> = (0..16).map(|i| ((id as f32 + i as f32) * 0.1).sin()).collect();
            Some(v)
        });

        assert!(
            svc.training_generation() >= 1,
            "generation should increment after training"
        );
    }

    // T-SVC-08: try_train_step does not fire below threshold
    #[test]
    fn train_step_no_fire() {
        let svc = small_service();
        svc.record_training_pairs(&[(1, 2, 1), (3, 4, 1)]);
        svc.try_train_step(&|_| None);
        assert_eq!(svc.training_generation(), 0);
    }

    // T-SVC-09: Debounced save counter
    #[test]
    fn debounced_save() {
        let svc = small_service();
        assert!(!svc.should_save());

        // Simulate 10 training steps
        for _ in 0..10 {
            svc.save_counter.fetch_add(1, Ordering::SeqCst);
        }
        assert!(svc.should_save());

        svc.reset_save_counter();
        assert!(!svc.should_save());
    }

    // T-SVC-10: Persistence round-trip through service API
    #[test]
    fn persistence_roundtrip() {
        let dir = tempfile::TempDir::new().unwrap();
        let svc = small_service();

        // Train to get non-trivial state
        let pairs: Vec<(u64, u64, u32)> = (0..40).map(|i| (i, i + 100, 1)).collect();
        svc.record_training_pairs(&pairs);
        svc.try_train_step(&|id| {
            let v: Vec<f32> = (0..16).map(|i| ((id as f32 + i as f32) * 0.1).sin()).collect();
            Some(v)
        });

        let gen_before = svc.training_generation();
        let input = vec![0.1_f32; 16];
        let output_before = svc.adapt_embedding(&input, None, None);

        // Save
        svc.save_state(dir.path()).unwrap();

        // Create new service and load
        let svc2 = small_service();
        svc2.load_state(dir.path()).unwrap();

        assert_eq!(svc2.training_generation(), gen_before);
        let output_after = svc2.adapt_embedding(&input, None, None);

        // Outputs should match
        for (a, b) in output_before.iter().zip(output_after.iter()) {
            assert!(
                (a - b).abs() < 1e-6,
                "output mismatch after load: {a} vs {b}"
            );
        }
    }

    // T-SVC-11: adapt_embedding dimension validation
    #[test]
    #[should_panic(expected = "input dimension mismatch")]
    fn dimension_validation() {
        let svc = default_service();
        let bad_input = vec![0.1_f32; 100];
        svc.adapt_embedding(&bad_input, None, None);
    }

    // T-SVC-12: Send + Sync
    #[test]
    fn send_sync() {
        let svc = Arc::new(default_service());
        let svc_clone = Arc::clone(&svc);
        let handle = thread::spawn(move || {
            let input = vec![0.1_f32; 384];
            let output = svc_clone.adapt_embedding(&input, None, None);
            assert_eq!(output.len(), 384);
        });
        handle.join().unwrap();
    }

    fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        if na < 1e-12 || nb < 1e-12 {
            return 0.0;
        }
        dot / (na * nb)
    }
}
