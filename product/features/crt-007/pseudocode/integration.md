# Pseudocode: integration (Wave 6)

## confidence.rs — trust_source "neural"

In `crates/unimatrix-engine/src/confidence.rs`, add to `trust_score()`:

```rust
pub fn trust_score(trust_source: &str) -> f64 {
    match trust_source {
        "human" => 1.0,
        "system" => 0.7,
        "agent" => 0.5,
        "neural" => 0.40,   // NEW: between auto (0.35) and agent (0.50)
        "auto" => 0.35,
        _ => 0.3,
    }
}
```

## background.rs — Neural Enhancement in extraction_tick

The neural enhancement step inserts between quality gate (step 3) and
near-duplicate check (step 4):

```rust
// After accepted = quality_gate results:

// NEW STEP: Neural enhancement
if !accepted.is_empty() {
    // Load neural models (lazy, once per tick)
    // This is optional -- if models fail to load, skip neural step

    match neural_enhance(&accepted, &model_registry, &shadow_evaluator) {
        Ok(enhanced) => {
            // Replace accepted with enhanced entries
            // (in shadow mode, entries are unchanged but evaluations are logged)
        }
        Err(e) => {
            tracing::warn!("neural enhancement failed, continuing without: {e}");
            // Fall through with original accepted entries
        }
    }
}
```

### neural_enhance function

```rust
fn neural_enhance(
    entries: &[ProposedEntry],
    registry: &ModelRegistry,
    evaluator: &ShadowEvaluator,
    classifier: &SignalClassifier,
    scorer: &ConventionScorer,
) -> Result<Vec<ProposedEntry>, String> {
    let state = registry.state("signal_classifier");

    match state {
        ModelState::Observation => {
            // Skip neural step entirely
            Ok(entries.to_vec())
        }

        ModelState::Shadow => {
            for entry in entries {
                let digest = build_signal_digest(entry);
                let class_result = classifier.predict(&digest);
                let score_result = scorer.predict(&digest);

                // Log shadow evaluation for classifier
                evaluator.evaluate(
                    "signal_classifier",
                    registry.get_production("signal_classifier")
                        .map(|m| m.version).unwrap_or(1),
                    &digest,
                    &entry_to_rule_prediction(entry),
                    class_result.predicted_class.as_str(),
                    class_result.confidence,
                    Some(&entry.feature_cycle),
                )?;

                // Log shadow evaluation for scorer
                evaluator.evaluate(
                    "convention_scorer",
                    registry.get_production("convention_scorer")
                        .map(|m| m.version).unwrap_or(1),
                    &digest,
                    &format!("{:.2}", entry.rule_confidence),
                    &format!("{:.2}", score_result.score),
                    score_result.score,
                    Some(&entry.feature_cycle),
                )?;
            }
            // Shadow mode: return entries unchanged
            Ok(entries.to_vec())
        }

        ModelState::Production => {
            let mut enhanced = Vec::with_capacity(entries.len());
            for entry in entries {
                let digest = build_signal_digest(entry);
                let class_result = classifier.predict(&digest);
                let score_result = scorer.predict(&digest);

                let mut entry = entry.clone();

                // Neural override: if confidence > threshold and disagrees
                if class_result.confidence > config.neural_override_confidence
                    && class_result.predicted_class.as_str() != entry.category
                {
                    entry.category = class_result.predicted_class.as_str().to_string();
                }

                // Convention score supplements rule confidence
                if score_result.score > entry.rule_confidence {
                    entry.rule_confidence = score_result.score;
                }

                // Mark as neurally enhanced
                entry.trust_source = "neural".to_string();

                enhanced.push(entry);
            }
            Ok(enhanced)
        }

        ModelState::RolledBack => {
            // Same as shadow mode (re-evaluate after rollback)
            // ... same shadow logic ...
            Ok(entries.to_vec())
        }
    }
}
```

### build_signal_digest

```rust
fn build_signal_digest(entry: &ProposedEntry) -> SignalDigest {
    // Extract features from ProposedEntry metadata:
    // - search_miss_count: from entry.source_features or 0
    // - co_access_density: from entry.source_features or 0.0
    // - consistency_score: from entry.consistency or 0.0
    // - feature_count: entry.source_features.len()
    // - observation_count: entry.observation_count or 0
    // - age_days: computed from entry timestamps
    // - rule_confidence: entry.rule_confidence

    SignalDigest::new(
        search_miss_count,
        co_access_density,
        consistency_score,
        feature_count,
        observation_count,
        age_days,
        rule_confidence,
        entry.source_rule.clone(),
        entry.feature_cycle.clone(),
    )
}
```

## Cargo.toml Changes

### crates/unimatrix-engine/Cargo.toml
```toml
[dependencies]
unimatrix-learn = { path = "../unimatrix-learn" }
```

### crates/unimatrix-server/Cargo.toml
```toml
[dependencies]
unimatrix-learn = { path = "../unimatrix-learn" }
```

## spawn_background_tick Changes

Add `model_registry` and `shadow_evaluator` parameters to
`spawn_background_tick` and `background_tick_loop`. Initialize them
at server startup from NeuralConfig.

```rust
pub fn spawn_background_tick(
    store: Arc<Store>,
    // ... existing params ...
    neural_config: NeuralConfig,  // NEW
) -> tokio::task::JoinHandle<()> {
    // Initialize ModelRegistry from neural_config.models_dir
    // Create SignalClassifier and ConventionScorer with baseline weights
    // Register models in registry
    // Create ShadowEvaluator

    // Pass to tick loop
}
```
