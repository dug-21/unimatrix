# Pseudocode: feedback-hooks (Wave 4)

## Purpose

Add feedback capture points in server MCP handlers that emit FeedbackSignal to TrainingService. Modified files in `crates/unimatrix-server/`.

## Server State Wiring (lib.rs)

```pseudo
// Add to server state struct:
struct ServerState {
    // existing fields...
    training_service: Option<Arc<TrainingService>>,
}

// In initialization:
let training_service = if learn_config_enabled {
    let registry = Arc::new(Mutex::new(ModelRegistry::new(config.models_dir.clone())));
    Some(Arc::new(TrainingService::new(learn_config, registry)))
} else {
    None
};
```

## Helpful/Unhelpful Vote Hook (services/usage.rs)

```pseudo
// After recording helpful/unhelpful vote:
fn after_vote(&self, entry_id: u64, is_helpful: bool, store: &Store) {
    if let Some(training_service) = &self.training_service {
        // Load entry to check trust_source
        if let Ok(Some(entry)) = store.get_entry(entry_id) {
            let trust_source = entry.trust_source.as_deref().unwrap_or("");
            if trust_source != "auto" && trust_source != "neural" {
                return; // Only train on auto/neural entries
            }

            // Reconstruct or retrieve SignalDigest
            let digest = reconstruct_digest(&entry);

            let signal = if is_helpful {
                FeedbackSignal::HelpfulVote {
                    entry_id,
                    category: entry.category.clone(),
                    digest,
                }
            } else {
                FeedbackSignal::UnhelpfulVote {
                    entry_id,
                    category: entry.category.clone(),
                    digest,
                }
            };
            training_service.record_feedback(signal);
        }
    }
}
```

## Category Correction Hook (mcp/correct.rs)

```pseudo
// After successful correction:
fn after_correction(&self, entry_id: u64, old_category: &str, new_category: &str, store: &Store) {
    if let Some(training_service) = &self.training_service {
        if let Ok(Some(entry)) = store.get_entry(entry_id) {
            let trust_source = entry.trust_source.as_deref().unwrap_or("");
            if trust_source != "auto" && trust_source != "neural" {
                return;
            }
            let digest = reconstruct_digest(&entry);
            let signal = FeedbackSignal::CategoryCorrection {
                entry_id,
                old_category: old_category.to_string(),
                new_category: new_category.to_string(),
                digest,
            };
            training_service.record_feedback(signal);
        }
    }
}
```

## Deprecation Hook (mcp/deprecate.rs)

```pseudo
// After successful deprecation:
fn after_deprecation(&self, entry_id: u64, store: &Store) {
    if let Some(training_service) = &self.training_service {
        if let Ok(Some(entry)) = store.get_entry(entry_id) {
            let trust_source = entry.trust_source.as_deref().unwrap_or("");
            if trust_source != "auto" && trust_source != "neural" {
                return;
            }
            let digest = reconstruct_digest(&entry);
            let signal = FeedbackSignal::Deprecation {
                entry_id,
                category: entry.category.clone(),
                digest,
            };
            training_service.record_feedback(signal);
        }
    }
}
```

## Outcome Recording Hook (mcp/store.rs)

```pseudo
// When an outcome is stored:
fn after_outcome_stored(&self, feature_cycle: &str, result: OutcomeResult, store: &Store) {
    if let Some(training_service) = &self.training_service {
        // Query entries with this feature_cycle and auto/neural trust_source
        let entries = store.get_feature_entries(feature_cycle)
            .unwrap_or_default()
            .into_iter()
            .filter(|e| matches!(e.trust_source.as_deref(), Some("auto") | Some("neural")))
            .collect::<Vec<_>>();

        if entries.is_empty() {
            return;
        }

        let entry_ids: Vec<u64> = entries.iter().map(|e| e.id).collect();
        let digests: Vec<SignalDigest> = entries.iter().map(|e| reconstruct_digest(e)).collect();
        let categories: Vec<String> = entries.iter().map(|e| e.category.clone()).collect();

        let signal = FeedbackSignal::FeatureOutcome {
            feature_cycle: feature_cycle.to_string(),
            result,
            entry_ids,
            digests,
            categories,
        };
        training_service.record_feedback(signal);
    }
}
```

## Background Tick Hook (background.rs)

```pseudo
// In maintenance_tick, after standard maintenance:
fn emit_background_signals(&self, store: &Store) {
    if let Some(training_service) = &self.training_service {
        // Stale entry detection: entries not accessed in 10+ features
        // (Implementation depends on how "features" are counted --
        //  use entry last_accessed timestamp vs current feature count)
        // Simplified: emit StaleEntry for entries with low recent access

        // Convention follow/deviate: ADR-005
        // Query active convention entries with trust_source auto/neural
        // Match against recent observation data using topic+tag matching
        // Emit ConventionFollowed or ConventionDeviated signals
    }
}
```

## Digest Reconstruction Helper

```pseudo
fn reconstruct_digest(entry: &EntryRecord) -> SignalDigest {
    // Attempt to get digest from shadow_evaluations table first
    // Fallback: reconstruct from entry fields using SignalDigest::from_fields()
    SignalDigest::from_fields(
        entry.confidence as f32,
        entry.helpful_count as u32,
        entry.content.len() as u32,
        &entry.category,
        entry.topic.as_deref().unwrap_or(""),
        entry.tags.as_ref().map(|t| t.len()).unwrap_or(0) as u32,
        0, // co-access count placeholder
    )
}
```

## Trust Source Filter Pattern

Every hook follows the same pattern:
1. Load entry
2. Check trust_source is "auto" or "neural"
3. If not, return early (no signal)
4. Build signal and call record_feedback

This is the central pattern for R-04 mitigation.
