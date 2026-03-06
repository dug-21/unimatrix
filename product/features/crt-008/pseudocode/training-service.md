# Pseudocode: training-service (Wave 2)

## Purpose

TrainingService: per-model reservoirs, EWC states, threshold-triggered retraining via spawn_blocking. New file: `crates/unimatrix-learn/src/service.rs`.

## Config Extensions (config.rs)

```pseudo
struct LearnConfig {
    // Existing fields...

    // NEW crt-008 fields
    classifier_retrain_threshold: u64,    // default 20
    classifier_batch_size: usize,         // default 16
    scorer_retrain_threshold: u64,        // default 5
    scorer_batch_size: usize,             // default 4
    ewc_alpha: f32,                       // default 0.95
    ewc_lambda: f32,                      // default 0.5
    per_class_regression_threshold: f64,  // default 0.10
    weak_label_weight: f32,               // default 0.3
    training_lr: f32,                     // default 0.01
    reservoir_capacity: usize,            // default 500
    reservoir_seed: u64,                  // default 42
}
```

## TrainingService

```pseudo
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

struct TrainingService {
    reservoirs: Mutex<HashMap<String, TrainingReservoir<TrainingSample>>>,
    ewc_states: Mutex<HashMap<String, EwcState>>,
    training_locks: HashMap<String, Arc<AtomicBool>>,
    registry: Arc<Mutex<ModelRegistry>>,
    config: LearnConfig,
    label_generator: LabelGenerator,
}

impl TrainingService {
    fn new(config: LearnConfig, registry: Arc<Mutex<ModelRegistry>>) -> Self {
        let mut reservoirs = HashMap::new();
        reservoirs.insert("signal_classifier".into(),
            TrainingReservoir::new(config.reservoir_capacity, config.reservoir_seed));
        reservoirs.insert("convention_scorer".into(),
            TrainingReservoir::new(config.reservoir_capacity, config.reservoir_seed + 1));

        let classifier_param_count = SignalClassifier::new_with_baseline().flat_parameters().len();
        let scorer_param_count = ConventionScorer::new_with_baseline().flat_parameters().len();

        let mut ewc_states = HashMap::new();
        ewc_states.insert("signal_classifier".into(),
            EwcState::new(classifier_param_count, config.ewc_alpha, config.ewc_lambda));
        ewc_states.insert("convention_scorer".into(),
            EwcState::new(scorer_param_count, config.ewc_alpha, config.ewc_lambda));

        let mut training_locks = HashMap::new();
        training_locks.insert("signal_classifier".into(), Arc::new(AtomicBool::new(false)));
        training_locks.insert("convention_scorer".into(), Arc::new(AtomicBool::new(false)));

        let label_generator = LabelGenerator::new(config.weak_label_weight);

        Self { reservoirs: Mutex::new(reservoirs), ewc_states: Mutex::new(ewc_states),
               training_locks, registry, config, label_generator }
    }

    fn record_feedback(&self, signal: FeedbackSignal) {
        let labels = self.label_generator.generate(&signal);
        let mut reservoirs = self.reservoirs.lock().unwrap_or_else(|e| e.into_inner());

        // Track which models got new samples
        let mut affected_models = Vec::new();

        for (model_name, sample) in labels {
            if let Some(reservoir) = reservoirs.get_mut(&model_name) {
                reservoir.add(&[sample]);
                affected_models.push(model_name);
            }
        }

        // Check thresholds for affected models
        for model_name in affected_models {
            let threshold = match model_name.as_str() {
                "signal_classifier" => self.config.classifier_retrain_threshold,
                "convention_scorer" => self.config.scorer_retrain_threshold,
                _ => continue,
            };
            if let Some(reservoir) = reservoirs.get(&model_name) {
                if reservoir.len() as u64 >= threshold {
                    // Drop lock before trying to train
                    drop(reservoirs);
                    self.try_train_step(&model_name);
                    return; // Only trigger one model per call
                }
            }
        }
    }

    fn try_train_step(&self, model_name: &str) {
        let lock = match self.training_locks.get(model_name) {
            Some(l) => l.clone(),
            None => return,
        };

        // Acquire training lock (non-blocking)
        if lock.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return; // Another training task is running for this model
        }

        // Clone state needed for training
        let batch_size = match model_name {
            "signal_classifier" => self.config.classifier_batch_size,
            "convention_scorer" => self.config.scorer_batch_size,
            _ => { lock.store(false, Ordering::SeqCst); return; }
        };

        let batch: Vec<TrainingSample> = {
            let mut reservoirs = self.reservoirs.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(reservoir) = reservoirs.get_mut(model_name) {
                reservoir.sample_batch(batch_size).into_iter().cloned().collect()
            } else {
                lock.store(false, Ordering::SeqCst);
                return;
            }
        };

        if batch.is_empty() {
            lock.store(false, Ordering::SeqCst);
            return;
        }

        let ewc_state = {
            let states = self.ewc_states.lock().unwrap_or_else(|e| e.into_inner());
            states.get(model_name).cloned()  // Need Clone on EwcState or clone fields
        };

        let model_name_owned = model_name.to_string();
        let lr = self.config.training_lr;
        let registry = self.registry.clone();
        let ewc_states = /* Arc to ewc_states for post-training update */;

        // Drop guard for the lock
        struct LockGuard(Arc<AtomicBool>);
        impl Drop for LockGuard {
            fn drop(&mut self) { self.0.store(false, Ordering::SeqCst); }
        }

        tokio::task::spawn_blocking(move || {
            let _guard = LockGuard(lock);

            // Reconstruct model from baseline (or load production from registry)
            let mut model: Box<dyn NeuralModel> = match model_name_owned.as_str() {
                "signal_classifier" => Box::new(SignalClassifier::new_with_baseline()),
                "convention_scorer" => Box::new(ConventionScorer::new_with_baseline()),
                _ => return,
            };

            // Try to load production model params from registry
            // If production exists, use its parameters as starting point
            // ... registry.lock().load_model(model_name, Production) ...

            let mut ewc = ewc_state.unwrap_or_else(||
                EwcState::new(model.flat_parameters().len(), 0.95, 0.5));

            let mut grad_squared_accum = vec![0.0_f32; model.flat_parameters().len()];

            // Training loop
            for sample in &batch {
                let input = sample.digest.as_slice();
                let target_slice = match &sample.target {
                    Classification(t) => t.as_slice(),
                    ConventionScore(t) => std::slice::from_ref(t),
                };

                let params = model.flat_parameters();
                let (loss, grads) = model.compute_gradients(input, target_slice);
                let ewc_grads = ewc.gradient_contribution(&params);

                // Combine: sample.weight * task_grad + ewc_grad
                let combined: Vec<f32> = grads.iter().zip(ewc_grads.iter())
                    .map(|(g, e)| sample.weight * g + e)
                    .collect();

                model.apply_gradients(&combined, lr);

                // Accumulate gradient squared for EWC update
                for (i, g) in grads.iter().enumerate() {
                    grad_squared_accum[i] += g * g;
                }
            }

            // NaN/Inf check
            let final_params = model.flat_parameters();
            if final_params.iter().any(|p| p.is_nan() || p.is_infinite()) {
                // Discard model, lock released by guard
                return;
            }

            // Normalize accumulated grad squared
            let n = batch.len() as f32;
            for g in &mut grad_squared_accum {
                *g /= n;
            }

            // Update EWC state
            ewc.update_from_flat(&final_params, &grad_squared_accum);
            // Write back ewc state to shared map
            // ... ewc_states.lock().insert(model_name, ewc) ...

            // Save as shadow
            let model_bytes = model.serialize();
            let mut reg = registry.lock().unwrap_or_else(|e| e.into_inner());
            let gen = reg.get_shadow(&model_name_owned)
                .map(|v| v.generation + 1)
                .or_else(|| reg.get_production(&model_name_owned).map(|v| v.generation + 1))
                .unwrap_or(1);
            reg.save_model(&model_name_owned, ModelSlot::Shadow, &model_bytes).ok();
            reg.register_shadow(&model_name_owned, gen, 1).ok();

            // Lock released by LockGuard drop
        });
    }

    // Accessor for reservoir length (for testing)
    fn reservoir_len(&self, model_name: &str) -> usize {
        let reservoirs = self.reservoirs.lock().unwrap_or_else(|e| e.into_inner());
        reservoirs.get(model_name).map(|r| r.len()).unwrap_or(0)
    }
}
```

## Concurrency Notes

- `reservoirs` and `ewc_states` protected by Mutex (short critical sections)
- `training_locks` are AtomicBool per model (no mutex contention for lock check)
- Poison recovery: `.unwrap_or_else(|e| e.into_inner())` on all lock() calls
- Drop guard on AtomicBool ensures lock release even on panic in spawn_blocking

## EwcState Clone

EwcState needs to be cloneable for the spawn_blocking closure. Need to either:
1. Add Clone derive to EwcState (ndarray Array1 is Clone)
2. Use Arc<Mutex<EwcState>> and clone the Arc

Option 2 is better since we need to write back the updated EWC after training. Use the shared `ewc_states: Mutex<HashMap<...>>` and clone the EwcState fields into the closure, then write back after training.

Actually, simpler: just clone the EwcState for the training task, and after training completes, lock and replace. The EwcState is small (~40KB for classifier).
