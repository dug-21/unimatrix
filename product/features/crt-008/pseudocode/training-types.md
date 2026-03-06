# Pseudocode: training-types (Wave 1)

## Purpose

Define the feedback-to-label pipeline types and LabelGenerator. New file: `crates/unimatrix-learn/src/training.rs`.

## Types

```pseudo
use models::digest::SignalDigest;

#[derive(Clone, Debug)]
enum OutcomeResult {
    Success,
    Rework,
}

#[derive(Clone, Debug)]
enum TrainingTarget {
    Classification([f32; 5]),  // one-hot: [convention, pattern, gap, dead, noise]
    ConventionScore(f32),      // target score in [0.0, 1.0]
}

#[derive(Clone, Debug)]
enum FeedbackSignal {
    HelpfulVote { entry_id: u64, category: String, digest: SignalDigest },
    UnhelpfulVote { entry_id: u64, category: String, digest: SignalDigest },
    CategoryCorrection { entry_id: u64, old_category: String, new_category: String, digest: SignalDigest },
    ContentCorrection { entry_id: u64, category: String, digest: SignalDigest },
    Deprecation { entry_id: u64, category: String, digest: SignalDigest },
    StaleEntry { entry_id: u64, category: String, digest: SignalDigest },
    ConventionFollowed { entry_id: u64, digest: SignalDigest },
    ConventionDeviated { entry_id: u64, digest: SignalDigest },
    FeatureOutcome { feature_cycle: String, result: OutcomeResult, entry_ids: Vec<u64>, digests: Vec<SignalDigest> },
}

#[derive(Clone, Debug)]
struct TrainingSample {
    digest: SignalDigest,
    target: TrainingTarget,
    weight: f32,
    source: FeedbackSignal,
    entry_id: u64,
    timestamp: u64,
}
```

## LabelGenerator

```pseudo
struct LabelGenerator {
    weak_label_weight: f32,  // default 0.3
}

impl LabelGenerator {
    fn new(weak_label_weight: f32) -> Self { ... }

    fn generate(&self, signal: &FeedbackSignal) -> Vec<(String, TrainingSample)> {
        let now = current_epoch_millis();
        match signal {
            HelpfulVote { entry_id, category, digest } => {
                // One-hot for the entry's actual category
                let target = one_hot_for_category(category);
                vec![("signal_classifier".into(), TrainingSample {
                    digest: digest.clone(), target: Classification(target),
                    weight: 1.0, source: signal.clone(), entry_id: *entry_id, timestamp: now
                })]
            }
            UnhelpfulVote { entry_id, category, digest } => {
                // Target = noise (entry was unhelpful = likely misclassified)
                vec![("signal_classifier".into(), TrainingSample {
                    digest: digest.clone(), target: Classification([0,0,0,0,1]),
                    weight: 1.0, source: signal.clone(), entry_id: *entry_id, timestamp: now
                })]
            }
            CategoryCorrection { entry_id, new_category, digest, .. } => {
                let target = one_hot_for_category(new_category);
                vec![("signal_classifier".into(), TrainingSample {
                    digest: digest.clone(), target: Classification(target),
                    weight: 1.0, source: signal.clone(), entry_id: *entry_id, timestamp: now
                })]
            }
            ContentCorrection { entry_id, digest, .. } => {
                // Content was wrong enough to correct -> noise label at 0.7 weight
                vec![("signal_classifier".into(), TrainingSample {
                    digest: digest.clone(), target: Classification([0,0,0,0,1]),
                    weight: 0.7, source: signal.clone(), entry_id: *entry_id, timestamp: now
                })]
            }
            Deprecation { entry_id, category, digest } => {
                // Dual label: noise for classifier, 0.0 for scorer
                let mut labels = vec![("signal_classifier".into(), TrainingSample {
                    digest: digest.clone(), target: Classification([0,0,0,0,1]),
                    weight: 1.0, source: signal.clone(), entry_id: *entry_id, timestamp: now
                })];
                labels.push(("convention_scorer".into(), TrainingSample {
                    digest: digest.clone(), target: ConventionScore(0.0),
                    weight: 1.0, source: signal.clone(), entry_id: *entry_id, timestamp: now
                }));
                labels
            }
            StaleEntry { entry_id, digest, .. } => {
                // Weak dead label
                vec![("signal_classifier".into(), TrainingSample {
                    digest: digest.clone(), target: Classification([0,0,0,1,0]),
                    weight: self.weak_label_weight, source: signal.clone(),
                    entry_id: *entry_id, timestamp: now
                })]
            }
            ConventionFollowed { entry_id, digest } => {
                vec![("convention_scorer".into(), TrainingSample {
                    digest: digest.clone(), target: ConventionScore(1.0),
                    weight: 1.0, source: signal.clone(), entry_id: *entry_id, timestamp: now
                })]
            }
            ConventionDeviated { entry_id, digest } => {
                vec![("convention_scorer".into(), TrainingSample {
                    digest: digest.clone(), target: ConventionScore(0.0),
                    weight: 1.0, source: signal.clone(), entry_id: *entry_id, timestamp: now
                })]
            }
            FeatureOutcome { entry_ids, digests, result, .. } => {
                // One label per entry in the feature
                entry_ids.iter().zip(digests.iter()).map(|(id, digest)| {
                    let target = match result {
                        Success => {
                            // Weak positive: use entry's category (we don't have it here,
                            // so the caller must provide category info or we use a generic positive)
                            // Per spec: one-hot for entry's category -- but we don't have
                            // category in the signal. Use noise as fallback per spec table:
                            // FeatureOutcome(Rework) -> noise, FeatureOutcome(Success) -> entry category
                            // Since we lack category, we'll need it in the signal or default.
                            // DECISION: For Success, we'd need categories. Simplification:
                            // store categories alongside entry_ids. But spec says "one-hot for entry's category".
                            // The signal doesn't carry categories. We'll need to add them.
                            // Actually, looking at spec more carefully, the FeatureOutcome signal
                            // should carry enough context. We'll add categories to the signal.
                            Classification([0.2, 0.2, 0.2, 0.2, 0.2]) // placeholder -- see note
                        }
                        Rework => Classification([0,0,0,0,1]),  // noise
                    };
                    ("signal_classifier".into(), TrainingSample {
                        digest: digest.clone(), target,
                        weight: self.weak_label_weight, source: signal.clone(),
                        entry_id: *id, timestamp: now
                    })
                }).collect()
            }
        }
    }
}

// Helper: map category string to one-hot [f32; 5]
fn one_hot_for_category(category: &str) -> [f32; 5] {
    match category.to_lowercase().as_str() {
        "convention" => [1.0, 0.0, 0.0, 0.0, 0.0],
        "pattern"    => [0.0, 1.0, 0.0, 0.0, 0.0],
        "gap"        => [0.0, 0.0, 1.0, 0.0, 0.0],
        "dead"       => [0.0, 0.0, 0.0, 1.0, 0.0],
        _            => [0.0, 0.0, 0.0, 0.0, 1.0],  // noise for unknown
    }
}
```

## Note on FeatureOutcome categories

The `FeatureOutcome` signal needs categories for Success labels. Two options:
1. Add `categories: Vec<String>` to the FeatureOutcome variant
2. Default Success to a uniform distribution

Option 1 is more correct. Add `categories: Vec<String>` to `FeatureOutcome`. The feedback hook in Wave 4 can look up entry categories when building the signal.

## Module Registration

Add `pub mod training;` to `lib.rs` and re-export key types.
